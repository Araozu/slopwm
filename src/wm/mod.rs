// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Window-manager state and manage/render sequence orchestration.

mod bindings;
mod operation;
mod output;
mod seat;
mod window;

use std::collections::{HashMap, VecDeque};

use wayland_backend::client::ObjectId;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::app::AppData;
use crate::config::Config;
use crate::protocol::{
    river_output_v1::RiverOutputV1, river_seat_v1::RiverSeatV1,
    river_window_manager_v1::RiverWindowManagerV1, river_window_v1::RiverWindowV1,
    river_xkb_bindings_v1::RiverXkbBindingsV1,
};

use self::{
    operation::SeatOp,
    output::{Output, OutputGeometry},
    seat::Seat,
    window::Window,
};

#[derive(Debug, Default)]
pub(crate) struct WindowManager {
    config: Config,
    windows: VecDeque<Window>,
    outputs: HashMap<ObjectId, Output>,
    active_output: Option<ObjectId>,
    seats: HashMap<ObjectId, Seat>,
}

impl WindowManager {
    pub(crate) fn new(config: Config) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    fn handle_manage_start(
        &mut self,
        proxy: &RiverWindowManagerV1,
        river_xkb: &RiverXkbBindingsV1,
        qh: &QueueHandle<AppData>,
    ) {
        self.remove_windows();
        self.remove_seats();
        self.manage_outputs();
        self.init_new_windows();
        self.init_new_seats(river_xkb, qh);
        self.manage_windows();
        self.manage_seats(proxy);
        proxy.manage_finish();
    }

    fn handle_render_start(&mut self, proxy: &RiverWindowManagerV1) {
        for seat in self.seats.values_mut() {
            seat.render_operation(&mut self.windows);
        }
        proxy.render_finish();
    }

    fn active_output_geometry(&self) -> Option<OutputGeometry> {
        self.active_output
            .as_ref()
            .and_then(|id| self.outputs.get(id))
            .map(|output| output.geometry)
    }

    fn manage_outputs(&mut self) {
        let changed = self
            .outputs
            .values()
            .any(|output| output.removed || output.changed);
        self.outputs.retain(|_, output| {
            if output.removed {
                output.proxy.destroy();
                return false;
            }
            output.changed = false;
            true
        });

        // Monitor focus stays put until its output disappears. Pointer position
        // never selects a monitor; a future binding can change active_output.
        if self.active_output_geometry().is_none() {
            self.active_output = self
                .outputs
                .values()
                .min_by_key(|output| (output.geometry.x, output.geometry.y))
                .map(|output| output.proxy.id());
        }
        let Some(target) = self.active_output_geometry() else {
            return;
        };
        if !changed {
            return;
        }

        for window in self.windows.iter_mut().filter(|window| !window.new) {
            if self.outputs.values().any(|output| {
                output
                    .geometry
                    .intersects(window.x, window.y, window.width, window.height)
            }) {
                continue;
            }
            // Cancel an operation that would move the window back off-screen.
            for seat in self.seats.values_mut() {
                if let SeatOp::Move { window_proxy, .. } | SeatOp::Resize { window_proxy, .. } =
                    &seat.op
                    && window_proxy == &window.proxy
                {
                    seat.op_end();
                    seat.op_release = false;
                }
            }
            let (x, y) = target.clamp_position(window.x, window.y, window.width, window.height);
            window.set_position(x, y);
        }
    }

    fn remove_windows(&mut self) {
        let old_windows = std::mem::take(&mut self.windows);
        self.windows = old_windows
            .into_iter()
            .filter(|window| {
                if window.closed {
                    for seat in self.seats.values_mut() {
                        if let SeatOp::Move { window_proxy, .. }
                        | SeatOp::Resize { window_proxy, .. } = &seat.op
                            && window_proxy == &window.proxy
                        {
                            seat.op_end();
                        }
                    }
                    return false;
                }
                true
            })
            .collect();
    }

    fn remove_seats(&mut self) {
        self.seats.retain(|_, seat| {
            if seat.removed {
                seat.destroy_bindings();
                seat.proxy.destroy();
                return false;
            }
            true
        });
    }

    fn init_new_windows(&mut self) {
        let output = self.active_output_geometry();
        for window in self.windows.iter_mut().filter(|w| w.new) {
            let (x, y) = output.map_or((window.x, window.y), |output| (output.x, output.y));
            window.set_position(x, y);
            window.proxy.propose_dimensions(window.width, window.height);
            window.new = false;
        }
    }

    fn init_new_seats(&mut self, river_xkb: &RiverXkbBindingsV1, qh: &QueueHandle<AppData>) {
        for seat in self.seats.values_mut() {
            seat.init_bindings(river_xkb, qh, &self.config.keybindings);
        }
    }

    fn manage_windows(&mut self) {
        for window in self.windows.iter_mut() {
            if let Some(seat_proxy) = window.pointer_move_requested.take() {
                let seat = self
                    .seats
                    .get_mut(&seat_proxy.id())
                    .expect("Seat not found");
                seat.pointer_move(window);
            }
            if let Some(seat_proxy) = window.pointer_resize_requested.take() {
                let seat = self
                    .seats
                    .get_mut(&seat_proxy.id())
                    .expect("Seat not found");
                seat.pointer_resize(window, window.pointer_resize_requested_edges);
            }
        }
    }

    fn manage_seats(&mut self, wm_proxy: &RiverWindowManagerV1) {
        for seat in self.seats.values_mut() {
            if let Some(window_proxy) = seat.interacted.take() {
                let i = self
                    .windows
                    .iter()
                    .position(|window| window.proxy == window_proxy)
                    .expect("Interacted window not found");
                let window = self.windows.remove(i).unwrap();
                self.windows.push_back(window);
            }
            seat.focus_top(&self.windows);
            seat.do_action(&mut self.windows, wm_proxy);
            if seat.op_release {
                seat.op_end();
                seat.op_release = false;
            } else {
                seat.op_manage();
            }
        }
    }
}

impl Dispatch<RiverWindowManagerV1, ()> for AppData {
    fn event(
        state: &mut Self,
        proxy: &RiverWindowManagerV1,
        event: <RiverWindowManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_window_manager_v1::Event;
        match event {
            Event::Unavailable => {
                eprintln!("Error: Another WM is already running");
                std::process::exit(1);
            }
            Event::Finished => std::process::exit(0),
            Event::ManageStart => {
                let river_xkb = state
                    .river_xkb
                    .as_ref()
                    .expect("river_xkb_bindings_v1 missing");
                state.wm.handle_manage_start(proxy, river_xkb, qh)
            }
            Event::RenderStart => state.wm.handle_render_start(proxy),
            Event::SessionLocked => {}
            Event::SessionUnlocked => {}
            Event::Window { id } => state.wm.windows.push_back(Window::new(id, qh)),
            Event::Output { id } => {
                if state.wm.active_output.is_none() {
                    state.wm.active_output = Some(id.id());
                }
                state.wm.outputs.insert(id.id(), Output::new(id));
            }
            Event::Seat { id } => {
                state.wm.seats.insert(id.id(), Seat::new(id));
            }
        }
    }

    wayland_client::event_created_child!(AppData, RiverWindowManagerV1, [
        crate::protocol::river_window_manager_v1::EVT_WINDOW_OPCODE => (RiverWindowV1, ()),
        crate::protocol::river_window_manager_v1::EVT_OUTPUT_OPCODE => (RiverOutputV1, ()),
        crate::protocol::river_window_manager_v1::EVT_SEAT_OPCODE => (RiverSeatV1, ())
    ]);
}
