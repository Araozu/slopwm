// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Window-manager state and manage/render sequence orchestration.

mod bindings;
mod columns;
mod input;
mod layout;
mod output;
mod overlay;
mod preselection;
mod seat;
mod window;
mod workspaces;

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
    input::InputDevice, layout::TileWidth, output::Output, overlay::Overlay,
    preselection::Preselection, seat::Seat, window::Window, workspaces::DetachedWorkspace,
};

#[derive(Debug, Default)]
pub(crate) struct WindowManager {
    config: Config,
    windows: VecDeque<Window>,
    outputs: HashMap<ObjectId, Output>,
    detached_workspaces: Vec<DetachedWorkspace>,
    pub(crate) output_names: HashMap<u32, String>,
    active_output: Option<ObjectId>,
    seats: HashMap<ObjectId, Seat>,
    input_devices: HashMap<ObjectId, InputDevice>,
    next_column: u64,
    preselection: Option<Preselection>,
    overlay: Option<Overlay>,
    session_locked: bool,
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
        self.configure_keyboards();
        self.remove_windows();
        self.remove_seats();
        self.manage_outputs();
        self.reconcile_workspaces();
        self.reconcile_preselection();
        self.init_new_windows();
        self.reconcile_workspaces();
        for window in &mut self.windows {
            window.apply_requests();
        }
        // Application maximize requests can hide the focused row; move focus
        // to the visible soft row before seats sync focus to River.
        self.reconcile_soft_fullscreen_focus();
        // Seats are temporarily separated so actions can update manager policy.
        let mut seats = std::mem::take(&mut self.seats);
        for seat in seats.values_mut() {
            seat.init_bindings(river_xkb, qh, &self.config.keybindings);
            if let Some(window) = seat.interacted.take() {
                // Locked sessions ignore click-to-focus alongside bindings.
                if !self.session_locked {
                    self.select_window(&window);
                }
            }
            seat.sync_focus(self);
            seat.do_actions(self, proxy);
        }
        self.seats = seats;
        self.reconcile_preselection();
        self.layout_windows();
        for window in &mut self.windows {
            if let Some(output) = window.output.as_ref().and_then(|id| self.outputs.get(id)) {
                window.manage(output);
            }
        }
        proxy.manage_finish();
    }

    fn handle_render_start(
        &mut self,
        proxy: &RiverWindowManagerV1,
        compositor: &wayland_client::protocol::wl_compositor::WlCompositor,
        shm: &wayland_client::protocol::wl_shm::WlShm,
        qh: &QueueHandle<AppData>,
    ) {
        for window in &mut self.windows {
            let output = window
                .output
                .as_ref()
                .and_then(|id| self.outputs.get(id))
                .filter(|output| output.workspaces.current().id == window.workspace)
                .map(|output| output.geometry);
            window.render(output, self.config.border.color);
        }
        let preview = self.preselection_preview();
        if preview.is_some() && self.overlay.is_none() {
            self.overlay = Some(Overlay::new(proxy, compositor, qh));
        }
        if let Some(overlay) = &mut self.overlay {
            overlay.render(preview, shm, qh);
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
        let old_active = self.active_output.clone();
        let old_workspace = old_active
            .as_ref()
            .and_then(|id| self.outputs.get(id))
            .map(|output| output.workspaces.current().id);
        self.outputs.retain(|_, output| {
            if output.removed {
                for workspace in std::mem::take(&mut output.workspaces.entries) {
                    self.detached_workspaces.push(DetachedWorkspace {
                        output: output.proxy.id(),
                        workspace,
                    });
                }
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
            }
        }
        // Keep each removed monitor's occupied workspaces distinct. Missing
        // output membership survives a period with no outputs, for later rehome.
        if let Some(target) = self.active_output.clone() {
            let mut migrated = HashMap::new();
            let workspaces = &mut self.outputs.get_mut(&target).unwrap().workspaces;
            // The window deque follows column order. Import workspace groups
            // in their original vertical order instead of their first window's
            // position in that deque.
            for detached in self.detached_workspaces.drain(..) {
                if self.windows.iter().any(|window| {
                    window.output.as_ref() == Some(&detached.output)
                        && window.workspace == detached.workspace.id
                }) {
                    migrated.insert(
                        (Some(detached.output), detached.workspace.id),
                        workspaces.import(detached.workspace.focused),
                    );
                }
            }
            for window in &mut self.windows {
                if window.new
                    || window
                        .output
                        .as_ref()
                        .is_some_and(|id| outputs.contains(id))
                {
                    continue;
                }
                let origin = (window.output.clone(), window.workspace);
                let workspace = *migrated
                    .entry(origin.clone())
                    .or_insert_with(|| workspaces.import(None));
                if origin.0 == old_active && Some(origin.1) == old_workspace {
                    workspaces.activate(workspace);
                }
                window.output = Some(target.clone());
                window.workspace = workspace;
            }
        }
    }

    fn remove_windows(&mut self) {
        let old = std::mem::take(&mut self.windows);
        for window in old {
            if window.closed {
                for detached in &mut self.detached_workspaces {
                    if detached.workspace.focused.as_ref() == Some(&window.proxy) {
                        detached.workspace.focused = None;
                    }
                }
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

    fn set_session_locked(&mut self, locked: bool) {
        self.session_locked = locked;
        if locked {
            self.preselection = None;
            for seat in self.seats.values_mut() {
                seat.interacted = None;
                seat.pending_actions.clear();
            }
        }
    }

    fn init_new_windows(&mut self) {
        if self.active_output.is_none() {
            return;
        }
        let (new, existing): (VecDeque<_>, VecDeque<_>) = std::mem::take(&mut self.windows)
            .into_iter()
            .partition(|window| window.new);
        self.windows = existing;
        for mut window in new {
            window.initialize();
            window.tile_width = TileWidth::new(self.config.scrolling.default_width_percent);
            window.column = self.allocate_column();
            // Dialogs must not steal the pending placement from the next app.
            if window.parent.is_none()
                && let Some(preselection) = self.preselection.take()
            {
                self.insert_preselected_window(window, preselection);
            } else {
                let output = self.active_output.clone().unwrap();
                window.workspace = self.outputs[&output].workspaces.current().id;
                self.insert_window(window, output);
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
            Event::Finished => {
                if let Some(overlay) = state.wm.overlay.take() {
                    overlay.destroy();
                }
                std::process::exit(0);
            }
            Event::ManageStart => {
                let xkb = state
                    .river_xkb
                    .as_ref()
                    .expect("river_xkb_bindings_v1 missing");
                state.wm.handle_manage_start(proxy, xkb, qh);
            }
            Event::RenderStart => state.wm.handle_render_start(
                proxy,
                state.compositor.as_ref().expect("wl_compositor missing"),
                state.shm.as_ref().expect("wl_shm missing"),
                qh,
            ),
            Event::SessionLocked => state.wm.set_session_locked(true),
            Event::SessionUnlocked => state.wm.set_session_locked(false),
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
