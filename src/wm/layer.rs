// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Optional layer-shell support for wallpapers, bars, and panels.
//!
//! Binding `river_layer_shell_v1` tells River that layer-shell clients may
//! map their surfaces. Without this binding the compositor closes layer
//! surfaces immediately, so tools like `awww` end up with no outputs and
//! report that none of the requested outputs are valid.
//!
//! Top/bottom exclusive zones reserve vertical space while preserving the
//! monitor's full horizontal scrolling area. A layer surface
//! with exclusive keyboard focus (for example a launcher) suppresses
//! window-manager focus requests until River releases exclusivity.

use wayland_backend::client::ObjectId;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::app::AppData;
use crate::protocol::{
    river_layer_shell_output_v1::RiverLayerShellOutputV1,
    river_layer_shell_seat_v1::RiverLayerShellSeatV1, river_layer_shell_v1::RiverLayerShellV1,
};

use super::WindowManager;

wayland_client::delegate_noop!(AppData: ignore RiverLayerShellV1);

impl WindowManager {
    pub(super) fn manage_layer_shell(
        &mut self,
        layer_shell: Option<&RiverLayerShellV1>,
        qh: &QueueHandle<AppData>,
    ) {
        let Some(layer_shell) = layer_shell else {
            return;
        };
        // Create one layer-shell object per output and seat. Object creation
        // is safe here; only set_default modifies policy state.
        let missing_outputs: Vec<ObjectId> = self
            .outputs
            .iter()
            .filter(|(_, output)| !output.removed && output.layer_output.is_none())
            .map(|(id, _)| id.clone())
            .collect();
        for id in missing_outputs {
            let Some(output) = self.outputs.get_mut(&id) else {
                continue;
            };
            output.layer_output = Some(layer_shell.get_output(&output.proxy, qh, id));
        }
        let missing_seats: Vec<ObjectId> = self
            .seats
            .iter()
            .filter(|(_, seat)| !seat.removed && seat.layer_seat.is_none())
            .map(|(id, _)| id.clone())
            .collect();
        for id in missing_seats {
            let Some(seat) = self.seats.get_mut(&id) else {
                continue;
            };
            seat.layer_seat = Some(layer_shell.get_seat(&seat.proxy, qh, id));
        }
        // Keep a defined fallback for layer surfaces that do not request a
        // specific output. The active monitor is the least surprising choice.
        let default = self
            .active_output
            .as_ref()
            .and_then(|id| self.outputs.get(id))
            .filter(|output| output.layer_output.is_some())
            .map(|output| output.proxy.id())
            .or_else(|| {
                self.ordered_outputs()
                    .first()
                    .and_then(|id| self.outputs.get(id))
                    .filter(|output| output.layer_output.is_some())
                    .map(|output| output.proxy.id())
            });
        if let Some(id) = default
            && let Some(output) = self.outputs.get(&id)
            && let Some(layer_output) = &output.layer_output
        {
            layer_output.set_default();
        }
    }
}

impl Dispatch<RiverLayerShellOutputV1, ObjectId> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &RiverLayerShellOutputV1,
        event: <RiverLayerShellOutputV1 as Proxy>::Event,
        data: &ObjectId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_layer_shell_output_v1::Event;
        let Some(output) = state.wm.outputs.get_mut(data) else {
            return;
        };
        match event {
            Event::NonExclusiveArea {
                x,
                y,
                width,
                height,
            } => {
                output.non_exclusive_area = Some(super::output::OutputGeometry {
                    x,
                    y,
                    width,
                    height,
                });
            }
        }
    }
}

impl Dispatch<RiverLayerShellSeatV1, ObjectId> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &RiverLayerShellSeatV1,
        event: <RiverLayerShellSeatV1 as Proxy>::Event,
        data: &ObjectId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_layer_shell_seat_v1::Event;
        let Some(seat) = state.wm.seats.get_mut(data) else {
            return;
        };
        match event {
            Event::FocusExclusive => {
                seat.layer_exclusive = true;
                seat.layer_non_exclusive = false;
            }
            Event::FocusNonExclusive => {
                seat.layer_exclusive = false;
                seat.layer_non_exclusive = true;
            }
            Event::FocusNone => {
                seat.layer_exclusive = false;
                seat.layer_non_exclusive = false;
            }
        }
        seat.focus_dirty = true;
    }
}
