// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Output geometry, lifecycle state, and output events.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::app::AppData;
use crate::protocol::river_output_v1::RiverOutputV1;

#[derive(Debug)]
pub(super) struct Output {
    pub(super) proxy: RiverOutputV1,
    pub(super) removed: bool,
    pub(super) changed: bool,
    pub(super) geometry: OutputGeometry,
}

impl Output {
    pub(super) fn new(proxy: RiverOutputV1) -> Self {
        Self {
            proxy,
            removed: false,
            changed: true,
            geometry: OutputGeometry::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct OutputGeometry {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
}

impl OutputGeometry {
    pub(super) fn intersects(self, x: i32, y: i32, width: i32, height: i32) -> bool {
        // Use wider arithmetic for logical outputs with negative or large origins.
        let (x, y) = (i64::from(x), i64::from(y));
        let (left, top) = (i64::from(self.x), i64::from(self.y));
        self.width > 0
            && self.height > 0
            && x < left + i64::from(self.width)
            && y < top + i64::from(self.height)
            && x + i64::from(width.max(1)) > left
            && y + i64::from(height.max(1)) > top
    }

    pub(super) fn clamp_position(self, x: i32, y: i32, width: i32, height: i32) -> (i32, i32) {
        let (left, top) = (i64::from(self.x), i64::from(self.y));
        let max_x = left + (i64::from(self.width) - i64::from(width.max(1))).max(0);
        let max_y = top + (i64::from(self.height) - i64::from(height.max(1))).max(0);
        (
            i64::from(x).clamp(left, max_x) as i32,
            i64::from(y).clamp(top, max_y) as i32,
        )
    }
}

impl Dispatch<RiverOutputV1, ()> for AppData {
    fn event(
        state: &mut Self,
        proxy: &RiverOutputV1,
        event: <RiverOutputV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_output_v1::Event;
        let output = state
            .wm
            .outputs
            .get_mut(&proxy.id())
            .expect("Output not found");
        match event {
            Event::Removed => output.removed = true,
            Event::WlOutput { name: _ } => {}
            Event::Position { x, y } => {
                (output.geometry.x, output.geometry.y) = (x, y);
                output.changed = true;
            }
            Event::Dimensions { width, height } => {
                (output.geometry.width, output.geometry.height) = (width, height);
                output.changed = true;
            }
            Event::CaptureSessions { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OutputGeometry;

    const LEFT: OutputGeometry = OutputGeometry {
        x: -1920,
        y: -200,
        width: 1920,
        height: 1080,
    };

    #[test]
    fn visibility_uses_negative_origins_and_excludes_touching_edges() {
        assert!(LEFT.intersects(-1800, -100, 800, 600));
        assert!(LEFT.intersects(-50, 0, 800, 600));
        assert!(!LEFT.intersects(0, 0, 800, 600));
        assert!(!LEFT.intersects(-2720, 0, 800, 600));
        assert!(!LEFT.intersects(-1800, 880, 800, 600));
        assert!(!OutputGeometry::default().intersects(0, 0, 800, 600));
    }

    #[test]
    fn recovery_keeps_windows_inside_the_remaining_output() {
        assert_eq!(LEFT.clamp_position(1920, 1080, 800, 600), (-800, 280));
        assert_eq!(LEFT.clamp_position(-3000, -1000, 800, 600), (-1920, -200));
        assert_eq!(LEFT.clamp_position(-1800, -100, 800, 600), (-1800, -100));
    }

    #[test]
    fn oversized_windows_keep_their_top_left_corner_accessible() {
        assert_eq!(LEFT.clamp_position(0, 0, 2560, 1440), (-1920, -200));
    }

    #[test]
    fn geometry_does_not_overflow_at_large_origins() {
        let output = OutputGeometry {
            x: i32::MAX - 100,
            y: i32::MIN,
            width: 1920,
            height: 1080,
        };
        assert!(output.intersects(i32::MAX, i32::MIN, 800, 600));
        assert_eq!(
            output.clamp_position(i32::MAX, 0, 800, 600),
            (i32::MAX, i32::MIN + 480)
        );
    }
}
