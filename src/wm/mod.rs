// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Window-manager state and manage/render sequence orchestration.

mod bindings;
mod columns;
mod layout;
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

use self::{layout::TileWidth, output::Output, seat::Seat, window::Window};

#[derive(Debug, Default)]
pub(crate) struct WindowManager {
    config: Config,
    windows: VecDeque<Window>,
    outputs: HashMap<ObjectId, Output>,
    pub(crate) output_names: HashMap<u32, String>,
    active_output: Option<ObjectId>,
    seats: HashMap<ObjectId, Seat>,
    next_column: u64,
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
        self.reconcile_focus();
        for window in &mut self.windows {
            window.apply_requests();
        }
        // Seats are temporarily separated so actions can update manager policy.
        let mut seats = std::mem::take(&mut self.seats);
        for seat in seats.values_mut() {
            seat.init_bindings(river_xkb, qh, &self.config.keybindings);
            if let Some(window) = seat.interacted.take() {
                self.select_window(&window);
            }
            seat.sync_focus(self);
            seat.do_actions(self, proxy);
        }
        self.seats = seats;
        self.layout_windows();
        for window in &mut self.windows {
            if let Some(output) = window.output.as_ref().and_then(|id| self.outputs.get(id)) {
                window.manage(output);
            }
        }
        proxy.manage_finish();
    }

    fn handle_render_start(&mut self, proxy: &RiverWindowManagerV1) {
        for window in &mut self.windows {
            let output = window
                .output
                .as_ref()
                .and_then(|id| self.outputs.get(id))
                .map(|output| output.geometry);
            window.render(output, self.config.border.color);
        }
        proxy.render_finish();
    }

    fn ordered_outputs(&self) -> Vec<ObjectId> {
        let mut outputs: Vec<_> = self
            .outputs
            .values()
            .filter(|output| output.geometry.width > 0 && output.geometry.height > 0)
            .collect();
        outputs.sort_by_key(|output| {
            (
                output.geometry.x,
                output.geometry.y,
                output.wl_output_name.unwrap_or_default(),
            )
        });
        outputs
            .into_iter()
            .map(|output| output.proxy.id())
            .collect()
    }

    fn manage_outputs(&mut self) {
        self.outputs.retain(|_, output| {
            if output.removed {
                output.proxy.destroy();
                false
            } else {
                true
            }
        });
        let outputs = self.ordered_outputs();
        if self
            .active_output
            .as_ref()
            .is_none_or(|id| !outputs.contains(id))
        {
            self.active_output = outputs.first().cloned();
        }
        for window in &mut self.windows {
            if window
                .output
                .as_ref()
                .is_some_and(|id| !outputs.contains(id))
            {
                window.output_removed();
                window.output = None;
            }
        }
        // Rehome only windows whose monitor disappeared. Scrolled-away windows
        // retain their monitor even when their nodes are far outside its bounds.
        if let Some(target) = self.active_output.clone() {
            while let Some(index) = self
                .windows
                .iter()
                .position(|window| !window.new && window.output.is_none())
            {
                let window = self.windows.remove(index).unwrap();
                self.insert_window(window, target.clone());
            }
        }
    }

    fn remove_windows(&mut self) {
        let old = std::mem::take(&mut self.windows);
        for window in old {
            if window.closed {
                for seat in self.seats.values_mut() {
                    if seat.interacted.as_ref() == Some(&window.proxy) {
                        seat.interacted = None;
                    }
                }
                window.node.destroy();
                window.proxy.destroy();
            } else {
                self.windows.push_back(window);
            }
        }
    }

    fn remove_seats(&mut self) {
        self.seats.retain(|_, seat| {
            if seat.removed {
                seat.destroy_bindings();
                seat.proxy.destroy();
                false
            } else {
                true
            }
        });
    }

    fn init_new_windows(&mut self) {
        let Some(output) = self.active_output.clone() else {
            return;
        };
        let (new, existing): (VecDeque<_>, VecDeque<_>) = std::mem::take(&mut self.windows)
            .into_iter()
            .partition(|window| window.new);
        self.windows = existing;
        for mut window in new {
            window.initialize();
            window.tile_width = TileWidth::new(self.config.scrolling.default_width_percent);
            window.column = self.allocate_column();
            self.insert_window(window, output.clone());
        }
    }

    fn reconcile_focus(&mut self) {
        for (id, output) in &mut self.outputs {
            if output.focused.as_ref().is_none_or(|focused| {
                !self
                    .windows
                    .iter()
                    .any(|window| &window.proxy == focused && window.output.as_ref() == Some(id))
            }) {
                output.focused = self
                    .windows
                    .iter()
                    .find(|window| window.output.as_ref() == Some(id))
                    .map(|window| window.proxy.clone());
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
                eprintln!(
                    "Error: River window management is unavailable (another WM may be running)"
                );
                std::process::exit(1);
            }
            Event::Finished => std::process::exit(0),
            Event::ManageStart => {
                let xkb = state
                    .river_xkb
                    .as_ref()
                    .expect("river_xkb_bindings_v1 missing");
                state.wm.handle_manage_start(proxy, xkb, qh);
            }
            Event::RenderStart => state.wm.handle_render_start(proxy),
            Event::SessionLocked | Event::SessionUnlocked => {}
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
