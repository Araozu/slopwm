// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Output lifecycle state and output events.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::app::AppData;
use crate::protocol::river_output_v1::RiverOutputV1;

#[derive(Debug)]
pub(super) struct Output {
    pub(super) proxy: RiverOutputV1,
    pub(super) removed: bool,
}

impl Output {
    pub(super) fn new(proxy: RiverOutputV1) -> Self {
        Self {
            proxy,
            removed: false,
        }
    }
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
            Event::WlOutput { name: _ } => {}
            Event::Position { x: _, y: _ } => {}
            Event::Dimensions {
                width: _,
                height: _,
            } => {}
            Event::CaptureSessions { .. } => {}
        }
    }
}
