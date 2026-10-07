// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Wayland connection, registry negotiation, and event loop.

use std::collections::HashMap;
use std::io;
use std::time::Instant;

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use wayland_backend::client::WaylandError;

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    protocol::{wl_compositor, wl_output, wl_registry, wl_shm},
};

use crate::config::{Config, ConfigSource};
use crate::protocol::{
    river_input_manager_v1::RiverInputManagerV1, river_layer_shell_v1::RiverLayerShellV1,
    river_window_manager_v1::RiverWindowManagerV1, river_xkb_bindings_v1::RiverXkbBindingsV1,
};
use crate::wm::WindowManager;

mod signals;

#[derive(Debug, Default)]
pub(crate) struct AppData {
    river_wm: Option<RiverWindowManagerV1>,
    pub(crate) river_xkb: Option<RiverXkbBindingsV1>,
    pub(crate) river_input: Option<RiverInputManagerV1>,
    pub(crate) river_layer_shell: Option<RiverLayerShellV1>,
    pub(crate) compositor: Option<wl_compositor::WlCompositor>,
    pub(crate) shm: Option<wl_shm::WlShm>,
    pub(crate) wm: WindowManager,
    wl_outputs: HashMap<u32, wl_output::WlOutput>,
    stopping: bool,
    failure: Option<String>,
}

impl AppData {
    pub(crate) fn request_manage_sequence(&self) {
        if !self.stopping
            && let Some(wm) = &self.river_wm
        {
            wm.manage_dirty();
        }
    }

    fn begin_shutdown(&mut self) {
        if self.stopping {
            return;
        }
        self.stopping = true;
        self.wm.quitting = true;
        if let Some(manager) = &self.river_wm {
            manager.stop();
        }
        if let Some(input) = &self.river_input {
            input.stop();
        }
    }

    pub(crate) fn manager_finished(&mut self, unavailable: bool) {
        self.wm.destroy();
        if let Some(layer_shell) = self.river_layer_shell.take() {
            layer_shell.destroy();
        }
        if let Some(xkb) = self.river_xkb.take() {
            xkb.destroy();
        }
        if let Some(manager) = self.river_wm.take() {
            manager.destroy();
        }
        if unavailable {
            self.failure =
                Some("River window management is unavailable (another WM may be running)".into());
        }
        self.begin_shutdown();
    }

    fn reload_config(&mut self, source: &ConfigSource) {
        if self.stopping {
            return;
        }
        match source.reload() {
            Ok(config) => {
                self.wm.pending_config = Some(config);
                self.request_manage_sequence();
            }
            Err(error) => eprintln!(
                "slopwm: configuration reload failed; keeping current configuration: {error}"
            ),
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for AppData {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            const RIVER_WINDOW_MANAGER_V1_MIN_VERSION: u32 = 4;
            const RIVER_XKB_BINDINGS_V1_MIN_VERSION: u32 = 1;
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, 1, qh, ()));
                }
                "river_window_manager_v1" => {
                    if version < RIVER_WINDOW_MANAGER_V1_MIN_VERSION {
                        state.failure = Some(format!(
                            "Server river_window_manager_v1 v{version}, but we need at least v{RIVER_WINDOW_MANAGER_V1_MIN_VERSION}",
                        ));
                        return;
                    }
                    let wm = registry.bind::<RiverWindowManagerV1, _, _>(
                        name,
                        version.min(RiverWindowManagerV1::interface().version),
                        qh,
                        (),
                    );
                    state.river_wm = Some(wm);
                }
                "river_xkb_bindings_v1" => {
                    if version < RIVER_XKB_BINDINGS_V1_MIN_VERSION {
                        state.failure = Some(format!(
                            "Server supports river_xkb_bindings_v1 v{version}, but we need at least v{RIVER_XKB_BINDINGS_V1_MIN_VERSION}"
                        ));
                        return;
                    }
                    let xkb = registry.bind::<RiverXkbBindingsV1, _, _>(
                        name,
                        version.min(RiverXkbBindingsV1::interface().version),
                        qh,
                        (),
                    );
                    state.river_xkb = Some(xkb);
                }
                "river_input_manager_v1" => {
                    state.river_input = Some(registry.bind::<RiverInputManagerV1, _, _>(
                        name,
                        version.min(RiverInputManagerV1::interface().version),
                        qh,
                        (),
                    ));
                }
                // Optional: older compositors may not advertise it. Without
                // this binding River closes layer surfaces immediately, so
                // wallpaper tools like awww cannot map any output.
                "river_layer_shell_v1" => {
                    state.river_layer_shell = Some(registry.bind::<RiverLayerShellV1, _, _>(
                        name,
                        version.min(RiverLayerShellV1::interface().version),
                        qh,
                        (),
                    ));
                }
                "wl_output" => {
                    let output =
                        registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, name);
                    state.wl_outputs.insert(name, output);
                }
                _ => {}
            }
        } else if let wl_registry::Event::GlobalRemove { name } = event
            && let Some(output) = state.wl_outputs.remove(&name)
        {
            if output.version() >= 3 {
                output.release();
            }
            state.wm.output_names.remove(&name);
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name: output_name } = event {
            state.wm.output_names.insert(*name, output_name);
        }
    }
}

pub(crate) fn run(config: Config, source: ConfigSource) -> Result<(), Box<dyn std::error::Error>> {
    let mut signals = signals::Signals::new()?;
    // Queue up a get_registry event.
    let conn = Connection::connect_to_env()?;
    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let _registry = display.get_registry(&event_queue.handle(), ());

    // Initial state
    let mut app_data = AppData {
        wm: WindowManager::new(config),
        ..AppData::default()
    };

    // Roundtrip to process the get_registry event and bind interfaces.
    event_queue.roundtrip(&mut app_data)?;
    let startup_error = app_data.failure.clone().or_else(|| {
        if app_data.river_wm.is_none() {
            Some("river_window_manager_v1 global not found; is River running?")
        } else if app_data.river_xkb.is_none() {
            Some("river_xkb_bindings_v1 global not found; is River running with XKB support?")
        } else if app_data.river_input.is_none() {
            Some("River must expose river_input_manager_v1 for keyboard repeat settings")
        } else if app_data.compositor.is_none() || app_data.shm.is_none() {
            Some("River must expose wl_compositor and wl_shm for spawn previews")
        } else {
            None
        }
        .map(String::from)
    });
    if let Some(error) = startup_error {
        app_data.failure = Some(error);
        app_data.begin_shutdown();
    }

    loop {
        event_queue.dispatch_pending(&mut app_data)?;
        let (reload, stop) = signals.pending()?;
        if stop || app_data.wm.quitting {
            app_data.begin_shutdown();
        }
        let reload_requested = std::mem::take(&mut app_data.wm.reload_requested);
        if reload || reload_requested {
            app_data.reload_config(&source);
        }
        if app_data.wm.animation_frame_due(Instant::now()) {
            app_data.request_manage_sequence();
        }
        let complete =
            app_data.stopping && app_data.river_wm.is_none() && app_data.river_input.is_none();
        if complete {
            for (_, output) in app_data.wl_outputs.drain() {
                if output.version() >= 3 {
                    output.release();
                }
            }
        }
        // Always flush finish/destructor requests, including the last iteration.
        let writable = match conn.flush() {
            Ok(()) => false,
            Err(WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => true,
            Err(error) => return Err(error.into()),
        };
        if complete && !writable {
            return match app_data.failure.take() {
                Some(error) => Err(error.into()),
                None => Ok(()),
            };
        }
        let Some(read) = event_queue.prepare_read() else {
            continue;
        };
        let flags = PollFlags::IN
            | if writable {
                PollFlags::OUT
            } else {
                PollFlags::empty()
            };
        let mut fds = [
            PollFd::from_borrowed_fd(read.connection_fd(), flags),
            PollFd::new(&signals.socket, PollFlags::IN),
        ];
        let timeout = app_data
            .wm
            .animation_timeout(Instant::now())
            .map(Timespec::try_from)
            .transpose()?;
        match poll(&mut fds, timeout.as_ref()) {
            Ok(_) => {
                if fds[0]
                    .revents()
                    .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR)
                {
                    match read.read() {
                        Ok(_) => {}
                        Err(WaylandError::Io(error))
                            if matches!(
                                error.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                            ) => {}
                        Err(error) => return Err(error.into()),
                    }
                }
            }
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
}
