// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Seat focus, queued keyboard actions, and seat events.

use std::collections::{HashMap, VecDeque};

use wayland_backend::client::ObjectId;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::action::Action;
use crate::app::AppData;
use crate::protocol::{
    river_seat_v1::RiverSeatV1, river_window_manager_v1::RiverWindowManagerV1,
    river_window_v1::RiverWindowV1,
};

use super::{WindowManager, bindings::XkbBinding};

#[derive(Debug)]
pub(super) struct Seat {
    pub(super) proxy: RiverSeatV1,
    pub(super) new: bool,
    pub(super) removed: bool,
    pub(super) focused: Option<RiverWindowV1>,
    pub(super) interacted: Option<RiverWindowV1>,
    pub(super) xkb_bindings: HashMap<ObjectId, XkbBinding>,
    pub(super) pending_actions: VecDeque<Action>,
}

impl Seat {
    pub(super) fn new(proxy: RiverSeatV1) -> Self {
        Self {
            proxy,
            new: true,
            removed: false,
            focused: None,
            interacted: None,
            xkb_bindings: HashMap::new(),
            pending_actions: VecDeque::new(),
        }
    }

    pub(super) fn do_actions(&mut self, wm: &mut WindowManager, proxy: &RiverWindowManagerV1) {
        while let Some(action) = self.pending_actions.pop_front() {
            match action {
                Action::Spawn(argv) => {
                    // Keep protocol logging out of spawned applications.
                    if let Err(error) = std::process::Command::new(&argv[0])
                        .args(&argv[1..])
                        .env_remove("WAYLAND_DEBUG")
                        .spawn()
                    {
                        eprintln!("Failed to spawn {:?}: {error}", argv[0]);
                    }
                }
                Action::Close => {
                    if let Some(window) = self.focused.as_ref() {
                        window.close();
                    }
                }
                Action::FocusNext | Action::FocusPrevious => {
                    wm.cycle_window(matches!(action, Action::FocusPrevious));
                }
                Action::FocusUp | Action::FocusDown => {
                    wm.focus_vertical(matches!(action, Action::FocusUp));
                }
                Action::StackNext | Action::StackPrevious => {
                    wm.stack_window(matches!(action, Action::StackPrevious));
                }
                Action::Unstack => wm.unstack_window(),
                Action::Preselect(direction) => wm.preselect(direction),
                Action::CancelPreselection => wm.preselection = None,
                Action::ChangeWidthPercent(delta) => {
                    if let Some(window) = self.focused.as_ref() {
                        wm.change_width(window, delta);
                    }
                }
                Action::FocusOutputNext | Action::FocusOutputPrevious => {
                    wm.cycle_output(matches!(action, Action::FocusOutputPrevious));
                }
                Action::FocusWorkspaceUp | Action::FocusWorkspaceDown => {
                    wm.focus_workspace(matches!(action, Action::FocusWorkspaceUp));
                }
                Action::MoveToWorkspaceUp | Action::MoveToWorkspaceDown => {
                    wm.move_to_workspace(matches!(action, Action::MoveToWorkspaceUp));
                }
                Action::ToggleSoftFullscreen | Action::ToggleFullscreen => {
                    if let Some(window) = wm
                        .windows
                        .iter_mut()
                        .find(|window| Some(&window.proxy) == self.focused.as_ref())
                    {
                        match action {
                            Action::ToggleSoftFullscreen => {
                                window.fullscreen = false;
                                window.fullscreen_requested = None;
                                window.soft_fullscreen_requested = None;
                                window.tile_width.soft_fullscreen =
                                    !window.tile_width.soft_fullscreen;
                            }
                            Action::ToggleFullscreen => {
                                window.fullscreen =
                                    !window.fullscreen_requested.unwrap_or(window.fullscreen);
                                window.fullscreen_requested = None;
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                Action::Exit => proxy.exit_session(),
            }
            self.sync_focus(wm);
        }
    }

    pub(super) fn sync_focus(&mut self, wm: &WindowManager) {
        let focused = wm
            .active_output
            .as_ref()
            .and_then(|id| wm.outputs.get(id))
            .and_then(|output| output.workspaces.current().focused.as_ref());
        if self.focused.as_ref() == focused {
            return;
        }
        match focused {
            Some(window) => {
                self.proxy.focus_window(window);
                if let Some(window) = wm
                    .windows
                    .iter()
                    .find(|candidate| &candidate.proxy == window)
                {
                    window.node.place_top();
                }
                self.focused = Some(window.clone());
            }
            None => {
                self.proxy.clear_focus();
                self.focused = None;
            }
        }
    }
}

impl Dispatch<RiverSeatV1, ()> for AppData {
    fn event(
        state: &mut Self,
        proxy: &RiverSeatV1,
        event: <RiverSeatV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_seat_v1::Event;
        let Some(seat) = state.wm.seats.get_mut(&proxy.id()) else {
            return;
        };
        match event {
            Event::Removed => seat.removed = true,
            Event::WindowInteraction { window } => seat.interacted = Some(window),
            // Pointer motion never changes keyboard or monitor focus.
            Event::PointerEnter { .. }
            | Event::PointerLeave
            | Event::PointerPosition { .. }
            | Event::WlSeat { .. }
            | Event::ShellSurfaceInteraction { .. }
            | Event::OpDelta { .. }
            | Event::OpRelease => {}
        }
    }
}
