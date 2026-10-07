// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Seat state, focus policy, actions, and seat events.

use std::collections::{HashMap, VecDeque};

use wayland_backend::client::ObjectId;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::app::AppData;
use crate::protocol::{
    river_seat_v1::RiverSeatV1,
    river_window_manager_v1::RiverWindowManagerV1,
    river_window_v1::{Edges, RiverWindowV1},
};

use super::{
    bindings::{Action, PointerBinding, XkbBinding},
    operation::SeatOp,
    window::Window,
};

#[derive(Debug)]
pub(super) struct Seat {
    pub(super) proxy: RiverSeatV1,
    pub(super) new: bool,
    pub(super) removed: bool,
    focused: Option<RiverWindowV1>,
    hovered: Option<RiverWindowV1>,
    pub(super) interacted: Option<RiverWindowV1>,
    pub(super) xkb_bindings: HashMap<ObjectId, XkbBinding>,
    pub(super) pointer_bindings: HashMap<ObjectId, PointerBinding>,
    pub(super) pending_action: Action,
    pub(super) op: SeatOp,
    pub(super) op_dx: i32,
    pub(super) op_dy: i32,
    pub(super) op_release: bool,
}

impl Seat {
    pub(super) fn new(proxy: RiverSeatV1) -> Self {
        Self {
            proxy,
            new: true,
            removed: false,
            focused: None,
            hovered: None,
            interacted: None,
            xkb_bindings: HashMap::new(),
            pointer_bindings: HashMap::new(),
            pending_action: Action::None,
            op: SeatOp::None,
            op_dx: 0,
            op_dy: 0,
            op_release: false,
        }
    }

    pub(super) fn do_action(
        &mut self,
        windows: &mut VecDeque<Window>,
        wm_proxy: &RiverWindowManagerV1,
    ) {
        match self.pending_action {
            Action::None => {}
            // Don't pass WAYLAND_DEBUG on to children, the added noise makes
            // debugging the window manager itself impractical.
            Action::SpawnFoot => match std::process::Command::new("foot")
                .env_remove("WAYLAND_DEBUG")
                .spawn()
            {
                Ok(_) => {}
                Err(e) => eprintln!("Failed to spawn foot: {e}"),
            },
            Action::Close => {
                if let Some(window_proxy) = self.focused.as_ref() {
                    window_proxy.close();
                }
            }
            Action::FocusNext => {
                if !windows.is_empty() {
                    windows.rotate_left(1);
                    self.focus_top(windows);
                }
            }
            Action::Move => {
                if let (Some(window_proxy), SeatOp::None) = (self.hovered.as_ref(), &self.op) {
                    let window = windows
                        .iter()
                        .find(|window| &window.proxy == window_proxy)
                        .expect("Hovered window not found");
                    self.pointer_move(window);
                }
            }
            Action::Resize => {
                if let (Some(window_proxy), SeatOp::None) = (self.hovered.as_ref(), &self.op) {
                    let window = windows
                        .iter()
                        .find(|window| &window.proxy == window_proxy)
                        .expect("Hovered window not found");
                    self.pointer_resize(window, Edges::Bottom.union(Edges::Right));
                }
            }
            Action::Exit => wm_proxy.exit_session(),
        }
        self.pending_action = Action::None;
    }

    pub(super) fn focus_top(&mut self, windows: &VecDeque<Window>) {
        match windows.back() {
            Some(window) => {
                self.proxy.focus_window(&window.proxy);
                window.node.place_top();
                self.focused = Some(window.proxy.clone());
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
        let seat = state.wm.seats.get_mut(&proxy.id()).expect("Seat not found");
        match event {
            Event::Removed => seat.removed = true,
            Event::WlSeat { name: _ } => {}
            Event::PointerEnter { window } => seat.hovered = Some(window),
            Event::PointerLeave => seat.hovered = None,
            Event::WindowInteraction { window } => seat.interacted = Some(window),
            Event::ShellSurfaceInteraction {
                shell_surface: _shell_surface,
            } => {}
            Event::OpDelta { dx, dy } => (seat.op_dx, seat.op_dy) = (dx, dy),
            Event::OpRelease => seat.op_release = true,
            Event::PointerPosition { x: _, y: _ } => {}
        }
    }
}
