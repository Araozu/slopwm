// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Wayland connection, registry negotiation, and event loop.

use std::collections::HashMap;

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    protocol::{wl_output, wl_registry},
};

use crate::config::Config;
use crate::protocol::{
    river_window_manager_v1::RiverWindowManagerV1, river_xkb_bindings_v1::RiverXkbBindingsV1,
};
use crate::wm::WindowManager;

#[derive(Debug, Default)]
pub(crate) struct AppData {
    river_wm: Option<RiverWindowManagerV1>,
    pub(crate) river_xkb: Option<RiverXkbBindingsV1>,
    pub(crate) wm: WindowManager,
    wl_outputs: HashMap<u32, wl_output::WlOutput>,
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

    loop {
        event_queue.blocking_dispatch(&mut app_data)?;
    }
}
