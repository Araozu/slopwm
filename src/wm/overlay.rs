// SPDX-License-Identifier: 0BSD

//! Input-transparent River shell surface and immutable shared-memory buffers.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::os::fd::AsFd;

use rustix::fs::{MemfdFlags, memfd_create};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{wl_buffer, wl_compositor, wl_region, wl_shm, wl_shm_pool, wl_surface},
};

use crate::action::SpawnDirection;
use crate::app::AppData;
use crate::protocol::{
    river_node_v1::RiverNodeV1, river_shell_surface_v1::RiverShellSurfaceV1,
    river_window_manager_v1::RiverWindowManagerV1,
};

use super::preselection::Preview;

#[derive(Debug)]
pub(super) struct Overlay {
    surface: wl_surface::WlSurface,
    shell: RiverShellSurfaceV1,
    node: RiverNodeV1,
    shown: Option<Preview>,
    error_reported: bool,
}

impl Overlay {
    pub(super) fn new(
        manager: &RiverWindowManagerV1,
        compositor: &wl_compositor::WlCompositor,
        qh: &QueueHandle<AppData>,
    ) -> Self {
        let surface = compositor.create_surface(qh, ());
        let shell = manager.get_shell_surface(&surface, qh, ());
        let node = shell.get_node(qh, ());
        let region = compositor.create_region(qh, ());
        surface.set_input_region(Some(&region));
        region.destroy();
        Self {
            surface,
            shell,
            node,
            shown: None,
            error_reported: false,
        }
    }

    pub(super) fn render(
        &mut self,
        preview: Option<Preview>,
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<AppData>,
    ) {
        if preview == self.shown {
            if preview.is_some() {
                // Window focus/creation can raise a node during manage.
                self.node.place_top();
            }
            return;
        }
        let buffer = match preview
            .map(|preview| draw_buffer(shm, preview, qh))
            .transpose()
        {
            Ok(buffer) => {
                self.error_reported = false;
                buffer
            }
            Err(error) => {
                if !self.error_reported {
                    eprintln!("Could not draw spawn preview: {error}");
                    self.error_reported = true;
                }
                self.hide();
                return;
            }
        };
        if let Some(preview) = preview {
            self.node.set_position(preview.x, preview.y);
            self.node.place_top();
            self.surface.damage(0, 0, preview.width, preview.height);
        }
        // The preview disappears in the same frame that presents the new tile.
        self.shell.sync_next_commit();
        self.surface.attach(buffer.as_ref(), 0, 0);
        self.surface.commit();
        self.shown = preview;
    }

    fn hide(&mut self) {
        if self.shown.take().is_some() {
            self.shell.sync_next_commit();
            self.surface.attach(None, 0, 0);
            self.surface.commit();
        }
    }

    pub(super) fn destroy(self) {
        self.node.destroy();
        self.shell.destroy();
        self.surface.destroy();
    }
}

fn draw_buffer(
    shm: &wl_shm::WlShm,
    preview: Preview,
    qh: &QueueHandle<AppData>,
) -> io::Result<wl_buffer::WlBuffer> {
    let stride = preview
        .width
        .checked_mul(4)
        .ok_or_else(|| io::Error::other("preview stride is too large"))?;
    let size = stride
        .checked_mul(preview.height)
        .filter(|size| *size > 0)
        .ok_or_else(|| io::Error::other("preview buffer is too large"))?;
    let mut file = File::from(memfd_create("slopwm-spawn-preview", MemfdFlags::CLOEXEC)?);
    file.set_len(size as u64)?;
    {
        let mut writer = BufWriter::new(&mut file);
        let mut row = Vec::new();
        row.try_reserve_exact(stride as usize)
            .map_err(io::Error::other)?;
        row.resize(stride as usize, 0);
        for y in 0..preview.height {
            for x in 0..preview.width {
                let offset = x as usize * 4;
                row[offset..offset + 4].copy_from_slice(&pixel(preview, x, y).to_ne_bytes());
            }
            writer.write_all(&row)?;
        }
        writer.flush()?;
    }
    let pool = shm.create_pool(file.as_fd(), size, qh, ());
    let buffer = pool.create_buffer(
        0,
        preview.width,
        preview.height,
        stride,
        wl_shm::Format::Argb8888,
        qh,
        (),
    );
    pool.destroy();
    // The server keeps the backing file alive. Never rewrite an in-use buffer.
    Ok(buffer)
}

fn pixel(preview: Preview, x: i32, y: i32) -> u32 {
    let (width, height) = (preview.width, preview.height);
    if x < 2 || y < 2 || x >= width - 2 || y >= height - 2 {
        return 0xff4da6ff;
    }
    let (dx, dy) = (x - width / 2, y - height / 2);
    let (along, across) = match preview.direction {
        SpawnDirection::Right => (dx, dy),
        SpawnDirection::Left => (-dx, dy),
        SpawnDirection::Up => (-dy, dx),
        SpawnDirection::Down => (dy, dx),
    };
    let radius = (width.min(height) / 5).clamp(2, 32);
    let thickness = (radius / 8).max(1);
    let shaft = (-radius..=radius).contains(&along) && across.abs() <= thickness;
    let head =
        (0..=radius).contains(&along) && (across.abs() - (radius - along)).abs() <= thickness;
    if shaft || head {
        0xdcdcdcdc
    } else {
        // Premultiplied ARGB8888: translucent blue, with no opaque background.
        0x38112438
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for AppData {
    fn event(
        _state: &mut Self,
        proxy: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            proxy.destroy();
        }
    }
}

wayland_client::delegate_noop!(AppData: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(AppData: ignore wl_region::WlRegion);
wayland_client::delegate_noop!(AppData: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(AppData: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(AppData: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(AppData: ignore RiverShellSurfaceV1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translucent_pixels_are_premultiplied_and_arrows_rotate_with_direction() {
        let preview = Preview {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            direction: SpawnDirection::Right,
        };
        let tint = pixel(preview, 10, 10);
        let [blue, green, red, alpha] = tint.to_le_bytes();
        assert!(alpha < 255 && [blue, green, red].iter().all(|channel| *channel <= alpha));
        for (direction, head, opposite) in [
            (SpawnDirection::Right, (60, 60), (40, 60)),
            (SpawnDirection::Left, (40, 60), (60, 60)),
            (SpawnDirection::Up, (60, 40), (60, 60)),
            (SpawnDirection::Down, (60, 60), (60, 40)),
        ] {
            let preview = Preview {
                direction,
                ..preview
            };
            assert_eq!(pixel(preview, head.0, head.1), 0xdcdcdcdc);
            assert_eq!(pixel(preview, opposite.0, opposite.1), tint);
        }
    }
}
