// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Configured keyboard bindings and binding lifecycle.

use wayland_backend::client::ObjectId;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::action::Action;
use crate::app::AppData;
use crate::config::KeyBinding;
use crate::protocol::{
    river_seat_v1::Modifiers, river_xkb_binding_v1::RiverXkbBindingV1,
    river_xkb_bindings_v1::RiverXkbBindingsV1,
};

use super::seat::Seat;

#[derive(Debug)]
pub(super) struct XkbBinding {
    proxy: RiverXkbBindingV1,
    action: Action,
    enabled: bool,
}

impl Seat {
    pub(super) fn init_bindings(
        &mut self,
        river_xkb: &RiverXkbBindingsV1,
        qh: &QueueHandle<AppData>,
        keybindings: &[KeyBinding],
        locked: bool,
    ) {
        if self.new {
            for binding in keybindings {
                self.create_xkb_binding(
                    river_xkb,
                    qh,
                    binding.modifiers,
                    binding.keysym,
                    binding.action.clone(),
                    !locked,
                );
            }
            self.new = false;
        }
        for binding in self.xkb_bindings.values_mut() {
            if binding.enabled == locked {
                if locked {
                    binding.proxy.disable();
                } else {
                    binding.proxy.enable();
                }
                binding.enabled = !locked;
            }
        }
    }

    pub(super) fn destroy_bindings(&mut self) {
        for (_, binding) in self.xkb_bindings.drain() {
            binding.proxy.destroy();
        }
    }

    fn create_xkb_binding(
        &mut self,
        river_xkb: &RiverXkbBindingsV1,
        qh: &QueueHandle<AppData>,
        mods: Modifiers,
        keysym: u32,
        action: Action,
        enabled: bool,
    ) {
        let proxy = river_xkb.get_xkb_binding(&self.proxy, keysym, mods, qh, self.proxy.id());
        if enabled {
            proxy.enable();
        }
        let binding = XkbBinding {
            proxy,
            action,
            enabled,
        };
        self.xkb_bindings.insert(binding.proxy.id(), binding);
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
        // While locked, drop presses instead of queueing them. Queued actions
        // from before the lock are discarded separately before they can run.
        if state.wm.session_locked || state.wm.quitting {
            return;
        }
        let Some(seat) = state.wm.seats.get_mut(data).filter(|seat| !seat.removed) else {
            return;
        };
        let Some(binding) = seat.xkb_bindings.get(&proxy.id()) else {
            return;
        };
        match event {
            Event::Pressed => seat.pending_actions.push_back(binding.action.clone()),
            Event::Released => {}
            Event::StopRepeat => {}
        }
    }
}

wayland_client::delegate_noop!(AppData: ignore RiverXkbBindingsV1);
