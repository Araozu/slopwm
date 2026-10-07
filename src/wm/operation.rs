// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Interactive pointer move/resize operations and render positioning.

use std::collections::VecDeque;

use crate::protocol::river_window_v1::{Edges, RiverWindowV1};

use super::{seat::Seat, window::Window};

#[derive(Debug, Clone)]
pub(super) enum SeatOp {
    None,
    Move {
        window_proxy: RiverWindowV1,
        start_x: i32,
        start_y: i32,
    },
    Resize {
        window_proxy: RiverWindowV1,
        start_x: i32,
        start_y: i32,
        start_width: i32,
        start_height: i32,
        edges: Edges,
    },
}

impl Seat {
    pub(super) fn op_end(&mut self) {
        if let SeatOp::Resize { window_proxy, .. } = &self.op {
            window_proxy.inform_resize_end();
        }
        self.proxy.op_end();
        self.op = SeatOp::None;
    }

    pub(super) fn op_manage(&mut self) {
        match &self.op {
            SeatOp::None | SeatOp::Move { .. } => {}
            SeatOp::Resize {
                window_proxy,
                start_width,
                start_height,
                edges,
                ..
            } => {
                let (mut width, mut height) = (*start_width, *start_height);
                if edges.contains(Edges::Left) {
                    width -= self.op_dx;
                }
                if edges.contains(Edges::Right) {
                    width += self.op_dx;
                }
                if edges.contains(Edges::Top) {
                    height -= self.op_dy;
                }
                if edges.contains(Edges::Bottom) {
                    height += self.op_dy;
                }
                window_proxy.propose_dimensions(width.max(1), height.max(1));
            }
        }
    }

    pub(super) fn pointer_move(&mut self, window: &Window) {
        self.interacted = Some(window.proxy.clone());
        self.proxy.op_start_pointer();
        self.op = SeatOp::Move {
            window_proxy: window.proxy.clone(),
            start_x: window.x,
            start_y: window.y,
        };
        self.op_dx = 0;
        self.op_dy = 0;
    }

    pub(super) fn pointer_resize(&mut self, window: &Window, edges: Edges) {
        self.interacted = Some(window.proxy.clone());
        self.proxy.op_start_pointer();
        window.proxy.inform_resize_start();
        self.op = SeatOp::Resize {
            window_proxy: window.proxy.clone(),
            start_x: window.x,
            start_y: window.y,
            start_width: window.width,
            start_height: window.height,
            edges,
        };
        self.op_dx = 0;
        self.op_dy = 0;
    }

    pub(super) fn render_operation(&self, windows: &mut VecDeque<Window>) {
        match &self.op {
            SeatOp::None => {}
            SeatOp::Move {
                window_proxy,
                start_x,
                start_y,
            } => {
                if let Some(window) = windows
                    .iter_mut()
                    .find(|window| &window.proxy == window_proxy)
                {
                    window.set_position(start_x + self.op_dx, start_y + self.op_dy);
                }
            }
            SeatOp::Resize {
                window_proxy,
                start_x,
                start_y,
                start_width,
                start_height,
                edges,
            } => {
                if let Some(window) = windows
                    .iter_mut()
                    .find(|window| &window.proxy == window_proxy)
                {
                    let (mut x, mut y) = (*start_x, *start_y);
                    if edges.contains(Edges::Left) {
                        x += start_width - window.width;
                    }
                    if edges.contains(Edges::Top) {
                        y += start_height - window.height;
                    }
                    window.set_position(x, y);
                }
            }
        }
    }
}
