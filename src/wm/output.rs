// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Output geometry, lifecycle state, and output events.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::app::AppData;
use crate::protocol::{
    river_layer_shell_output_v1::RiverLayerShellOutputV1, river_output_v1::RiverOutputV1,
};

use super::workspaces::Workspaces;

#[derive(Debug)]
pub(super) struct Output {
    pub(super) proxy: RiverOutputV1,
    pub(super) removed: bool,
    pub(super) wl_output_name: Option<u32>,
    pub(super) layer_output: Option<RiverLayerShellOutputV1>,
    pub(super) workspaces: Workspaces,
    pub(super) geometry: OutputGeometry,
}

impl Output {
    pub(super) fn new(proxy: RiverOutputV1) -> Self {
        Self {
            proxy,
            removed: false,
            wl_output_name: None,
            layer_output: None,
            workspaces: Workspaces::default(),
            geometry: OutputGeometry::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct OutputGeometry {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
}

impl Dispatch<RiverOutputV1, ()> for AppData {
    fn event(
        state: &mut Self,
        proxy: &RiverOutputV1,
        event: <RiverOutputV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_output_v1::Event;
        let output = state
            .wm
            .outputs
            .get_mut(&proxy.id())
            .expect("Output not found");
        match event {
            Event::Removed => output.removed = true,
            Event::WlOutput { name } => output.wl_output_name = Some(name),
            Event::Position { x, y } => {
                (output.geometry.x, output.geometry.y) = (x, y);
            }
            Event::Dimensions { width, height } => {
                (output.geometry.width, output.geometry.height) = (width, height);
            }
            Event::CaptureSessions { .. } => {}
        }
    }
}
