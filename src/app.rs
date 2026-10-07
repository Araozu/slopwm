// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Wayland connection, registry negotiation, and event loop.

use std::collections::HashMap;

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    protocol::{wl_compositor, wl_output, wl_registry, wl_shm},
};

use crate::config::Config;
use crate::protocol::{
    river_input_manager_v1::RiverInputManagerV1, river_layer_shell_v1::RiverLayerShellV1,
    river_window_manager_v1::RiverWindowManagerV1, river_xkb_bindings_v1::RiverXkbBindingsV1,
};
use crate::wm::WindowManager;

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
}

impl AppData {
    pub(crate) fn request_manage_sequence(&self) {
        if let Some(wm) = &self.river_wm {
            wm.manage_dirty();
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
                        eprintln!(
                            "Server river_window_manager_v1 v{version}, but we need at least v{RIVER_WINDOW_MANAGER_V1_MIN_VERSION}",
                        );
                        std::process::exit(1);
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
                        eprintln!(
                            "Server supports river_xkb_bindings_v1 v{version}, but we need at least v{RIVER_XKB_BINDINGS_V1_MIN_VERSION}"
                        );
                        std::process::exit(1);
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

pub(crate) fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
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
    if app_data.river_wm.is_none() {
        eprintln!("river_window_manager_v1 global not found! Is river running?");
        std::process::exit(1);
    }
    if app_data.river_xkb.is_none() {
        eprintln!("river_xkb_bindings_v1 global not found! Is river running with xkb support?");
        std::process::exit(1);
    }
    if app_data.river_input.is_none() {
        return Err("River must expose river_input_manager_v1 for keyboard repeat settings".into());
    }
    if app_data.compositor.is_none() || app_data.shm.is_none() {
        return Err("River must expose wl_compositor and wl_shm for spawn previews".into());
    }

    loop {
        event_queue.blocking_dispatch(&mut app_data)?;
    }
}
