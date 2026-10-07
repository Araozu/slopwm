// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Window geometry, lifecycle state, and window events.

use wayland_backend::client::ObjectId;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::app::AppData;
use crate::protocol::{
    river_node_v1::RiverNodeV1,
    river_window_v1::{Capabilities, Edges, RiverWindowV1},
};

use super::{
    layout::{TileGeometry, TileWidth},
    output::OutputGeometry,
};

#[derive(Debug)]
pub(super) struct Window {
    pub(super) proxy: RiverWindowV1,
    pub(super) node: RiverNodeV1,
    pub(super) new: bool,
    pub(super) closed: bool,
    pub(super) parent: Option<RiverWindowV1>,
    pub(super) dialog: bool,
    pub(super) natural_dimensions: Option<(i32, i32)>,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) output: Option<ObjectId>,
    pub(super) workspace: u64,
    pub(super) column: u64,
    pub(super) tile_width: TileWidth,
    pub(super) tile: Option<TileGeometry>,
    pub(super) requested_dimensions: Option<(i32, i32)>,
    pub(super) fullscreen: bool,
    fullscreen_output: Option<ObjectId>,
    pub(super) fullscreen_requested: Option<bool>,
    pub(super) soft_fullscreen_requested: Option<bool>,
    maximized: bool,
    visible: bool,
}

impl Window {
    pub(super) fn new(proxy: RiverWindowV1, qh: &QueueHandle<AppData>) -> Self {
        let node = proxy.get_node(qh, ());
        Window {
            proxy,
            node,
            new: true,
            closed: false,
            parent: None,
            dialog: false,
            natural_dimensions: None,
            width: 0,
            height: 0,
            output: None,
            workspace: 0,
            column: 0,
            tile_width: TileWidth::new(50),
            tile: None,
            requested_dimensions: None,
            fullscreen: false,
            fullscreen_output: None,
            fullscreen_requested: None,
            soft_fullscreen_requested: None,
            maximized: false,
            visible: true,
        }
    }

    pub(super) fn initialize(&mut self) {
        self.proxy
            .set_capabilities(Capabilities::Fullscreen | Capabilities::Maximize);
        self.proxy.set_tiled(if self.dialog {
            Edges::empty()
        } else {
            Edges::all()
        });
        self.proxy.use_ssd();
        self.node.place_top();
        self.new = false;
    }

    pub(super) fn output_removed(&mut self) {
        // River already leaves fullscreen when its output disappears.
        if self.fullscreen_output.take().is_some() {
            self.proxy.inform_not_fullscreen();
            self.fullscreen = false;
        }
        self.requested_dimensions = None;
        self.tile = None;
    }

    pub(super) fn apply_requests(&mut self) {
        if let Some(fullscreen) = self.fullscreen_requested.take() {
            self.fullscreen = fullscreen;
        }
        if let Some(soft) = self.soft_fullscreen_requested.take() {
            self.tile_width.soft_fullscreen = soft;
        }
    }

    pub(super) fn manage(&mut self, output: &super::output::Output) {
        if self.maximized != self.tile_width.soft_fullscreen {
            if self.tile_width.soft_fullscreen {
                self.proxy.inform_maximized();
            } else {
                self.proxy.inform_unmaximized();
            }
            self.maximized = self.tile_width.soft_fullscreen;
        }
        if self.fullscreen {
            if self.fullscreen_output.as_ref() != Some(&output.proxy.id()) {
                self.proxy.inform_fullscreen();
                self.proxy.fullscreen(&output.proxy);
                self.fullscreen_output = Some(output.proxy.id());
            }
            return;
        }
        if self.fullscreen_output.take().is_some() {
            self.proxy.exit_fullscreen();
            self.proxy.inform_not_fullscreen();
            self.requested_dimensions = None;
        }
        if self.dialog && self.natural_dimensions.is_none() {
            if self.requested_dimensions.is_none() {
                self.proxy.propose_dimensions(0, 0);
                self.requested_dimensions = Some((0, 0));
            }
            return;
        }
        if let Some(tile) = self.tile {
            let dimensions = tile.content_size();
            if self.requested_dimensions != Some(dimensions) {
                self.proxy.propose_dimensions(dimensions.0, dimensions.1);
                self.requested_dimensions = Some(dimensions);
            }
            let (x, y) = tile.content_position();
            self.node.set_position(x, y);
        }
    }

    pub(super) fn render(&mut self, output: Option<OutputGeometry>, color: [u32; 4]) {
        let Some(output) = output else {
            self.set_visible(false);
            return;
        };
        if self.fullscreen && self.fullscreen_output.is_some() {
            self.set_visible(true);
            return;
        }
        let intersection = self.tile.and_then(|tile| tile.intersection(output));
        let Some((x, y, width, height)) = intersection else {
            self.set_visible(false);
            return;
        };
        let tile = self.tile.unwrap();
        let (content_width, content_height) = tile.content_size();
        let (node_x, node_y) = tile.content_position();
        self.node.set_position(node_x, node_y);
        // Applications can round or reject a proposal. Clip confirmed content
        // to its allocation so it cannot overlap its neighbors or another output.
        self.proxy.set_content_clip_box(
            0,
            0,
            self.width.max(1).min(content_width),
            self.height.max(1).min(content_height),
        );
        self.proxy.set_clip_box(x, y, width, height);
        self.proxy.set_borders(
            Edges::all(),
            tile.border,
            color[0],
            color[1],
            color[2],
            color[3],
        );
        self.set_visible(true);
    }

    fn set_visible(&mut self, visible: bool) {
        if self.visible != visible {
            if visible {
                self.proxy.show();
            } else {
                self.proxy.hide();
            }
            self.visible = visible;
        }
    }
}

impl Dispatch<RiverWindowV1, ()> for AppData {
    fn event(
        state: &mut Self,
        proxy: &RiverWindowV1,
        event: <RiverWindowV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_window_v1::Event;
        let window = match state.wm.windows.iter_mut().find(|o| &o.proxy == proxy) {
            Some(window) => window,
            None => return,
        };
        let previous_natural = window.natural_dimensions;
        match event {
            Event::Closed => window.closed = true,
            Event::DimensionsHint {
                min_width: _,
                min_height: _,
                max_width: _,
                max_height: _,
            } => {}
            Event::Dimensions { width, height } => {
                (window.width, window.height) = (width, height);
                if window.dialog
                    && (window.natural_dimensions.is_none()
                        || window.requested_dimensions != Some((width, height)))
                    && !window.fullscreen
                    && !window.tile_width.soft_fullscreen
                {
                    window.natural_dimensions = Some((width, height));
                }
            }
            Event::AppId { app_id: _ } => {}
            Event::Title { title: _ } => {}
            Event::Parent { parent } => window.parent = parent,
            Event::DecorationHint { hint: _ } => {}
            Event::PointerMoveRequested { .. } | Event::PointerResizeRequested { .. } => {}
            Event::ShowWindowMenuRequested { x: _, y: _ } => {}
            Event::MaximizeRequested => window.soft_fullscreen_requested = Some(true),
            Event::UnmaximizeRequested => window.soft_fullscreen_requested = Some(false),
            Event::FullscreenRequested { .. } => window.fullscreen_requested = Some(true),
            Event::ExitFullscreenRequested => window.fullscreen_requested = Some(false),
            Event::MinimizeRequested => {}
            Event::UnreliablePid { unreliable_pid: _ } => {}
            Event::PresentationHint { .. } => {}
            Event::Identifier { .. } => {}
            Event::CaptureSessions { .. } => {}
        }
        if window.natural_dimensions != previous_natural {
            state.request_manage_sequence();
        }
    }
}

wayland_client::delegate_noop!(AppData: ignore RiverNodeV1);
