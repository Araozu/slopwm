// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Default input bindings, binding lifecycle, and queued actions.

use wayland_backend::client::ObjectId;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::app::AppData;
use crate::protocol::{
    river_pointer_binding_v1::RiverPointerBindingV1, river_seat_v1::Modifiers,
    river_xkb_binding_v1::RiverXkbBindingV1, river_xkb_bindings_v1::RiverXkbBindingsV1,
};

use super::seat::Seat;

#[derive(Debug, Clone, Copy)]
pub(super) enum Action {
    None,
    SpawnFoot,
    Close,
    FocusNext,
    Move,
    Resize,
    Exit,
}

#[derive(Debug)]
pub(super) struct XkbBinding {
    proxy: RiverXkbBindingV1,
    action: Action,
}

#[derive(Debug)]
pub(super) struct PointerBinding {
    proxy: RiverPointerBindingV1,
    action: Action,
}

impl Seat {
    pub(super) fn init_bindings(
        &mut self,
        river_xkb: &RiverXkbBindingsV1,
        qh: &QueueHandle<AppData>,
    ) {
        // See xkbcommon/xkbcommon-keysyms.h
        const SPACE: u32 = 0x20;
        const N: u32 = 0x6e;
        const Q: u32 = 0x71;
        const ESC: u32 = 0xff1b;
        // See linux/input-event-codes.h
        const BTN_LEFT: u32 = 0x110;
        const BTN_RIGHT: u32 = 0x111;
        let mods = Modifiers::Mod4;

        if self.new {
            self.create_xkb_binding(river_xkb, qh, mods, SPACE, Action::SpawnFoot);
            self.create_xkb_binding(river_xkb, qh, mods, Q, Action::Close);
            self.create_xkb_binding(river_xkb, qh, mods, N, Action::FocusNext);
            self.create_xkb_binding(river_xkb, qh, mods, ESC, Action::Exit);
            self.create_pointer_binding(qh, mods, BTN_LEFT, Action::Move);
            self.create_pointer_binding(qh, mods, BTN_RIGHT, Action::Resize);
            self.new = false;
        }
    }

    pub(super) fn destroy_bindings(&mut self) {
        self.xkb_bindings
            .values_mut()
            .for_each(|binding| binding.proxy.destroy());
        self.pointer_bindings
            .values_mut()
            .for_each(|binding| binding.proxy.destroy());
    }

    fn create_xkb_binding(
        &mut self,
        river_xkb: &RiverXkbBindingsV1,
        qh: &QueueHandle<AppData>,
        mods: Modifiers,
        keysym: u32,
        action: Action,
    ) {
        let proxy = river_xkb.get_xkb_binding(&self.proxy, keysym, mods, qh, self.proxy.id());
        proxy.enable();
        let binding = XkbBinding { proxy, action };
        self.xkb_bindings.insert(binding.proxy.id(), binding);
    }

    fn create_pointer_binding(
        &mut self,
        qh: &QueueHandle<AppData>,
        mods: Modifiers,
        button: u32,
        action: Action,
    ) {
        let proxy = self
            .proxy
            .get_pointer_binding(button, mods, qh, self.proxy.id());
        proxy.enable();
        let binding = PointerBinding { proxy, action };
        self.pointer_bindings.insert(binding.proxy.id(), binding);
    }
}

impl Dispatch<RiverXkbBindingV1, ObjectId> for AppData {
    fn event(
        state: &mut Self,
        proxy: &RiverXkbBindingV1,
        event: <RiverXkbBindingV1 as Proxy>::Event,
        data: &ObjectId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_xkb_binding_v1::Event;
        let seat = state.wm.seats.get_mut(data).expect("Seat not found");
        let binding = seat
            .xkb_bindings
            .get(&proxy.id())
            .expect("xkb_binding not found");
        match event {
            Event::Pressed => seat.pending_action = binding.action,
            Event::Released => {}
            Event::StopRepeat => {}
        }
    }
}

impl Dispatch<RiverPointerBindingV1, ObjectId> for AppData {
    fn event(
        state: &mut Self,
        proxy: &RiverPointerBindingV1,
        event: <RiverPointerBindingV1 as Proxy>::Event,
        data: &ObjectId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_pointer_binding_v1::Event;
        let seat = state.wm.seats.get_mut(data).expect("Seat not found");
        let binding = seat
            .pointer_bindings
            .get(&proxy.id())
            .expect("xkb_binding not found");
        match event {
            Event::Pressed => seat.pending_action = binding.action,
            Event::Released => {}
        }
    }
}

wayland_client::delegate_noop!(AppData: ignore RiverXkbBindingsV1);
