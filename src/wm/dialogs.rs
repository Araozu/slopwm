// SPDX-License-Identifier: 0BSD

//! Transient windows follow their parent and float above its tile.

use crate::protocol::river_window_v1::{Edges, RiverWindowV1};

use super::{
    WindowManager,
    layout::{TileGeometry, TileWidth, dialog_tile, outer_area},
    window::Window,
};

impl WindowManager {
    pub(super) fn tiled_window(&self, proxy: &RiverWindowV1) -> Option<&Window> {
        let mut window = self.windows.iter().find(|window| &window.proxy == proxy)?;
        for _ in 0..self.windows.len() {
            if let Some(parent) = window.parent.as_ref().and_then(|parent| {
                self.windows
                    .iter()
                    .find(|window| &window.proxy == parent && !window.closed)
            }) {
                window = parent;
            } else {
                return Some(window);
            }
        }
        None
    }

    pub(super) fn insert_new_dialog(&mut self, window: &mut Window) -> bool {
        let Some(parent) = window.parent.as_ref().and_then(|parent| {
            self.windows
                .iter()
                .find(|window| &window.proxy == parent && !window.new)
        }) else {
            return false;
        };
        let Some(output_id) = parent
            .output
            .clone()
            .filter(|id| self.outputs.contains_key(id))
        else {
            return false;
        };
        let output = &self.outputs[&output_id];
        let focused_family = output
            .workspaces
            .current()
            .focused
            .as_ref()
            .and_then(|proxy| self.tiled_window(proxy));
        let parent_family = self.tiled_window(&parent.proxy);
        let focus = self.active_output.as_ref() == Some(&output_id)
            && output.workspaces.current().id == parent.workspace
            && focused_family
                .zip(parent_family)
                .is_some_and(|(focused, parent)| focused.proxy == parent.proxy);
        window.dialog = true;
        window.output = Some(output_id.clone());
        window.workspace = parent.workspace;
        window.column = parent.column;
        window.tile_width = TileWidth::new(self.config.scrolling.default_width_percent);
        window.initialize();
        if focus {
            self.outputs
                .get_mut(&output_id)
                .unwrap()
                .workspaces
                .current_mut()
                .focused = Some(window.proxy.clone());
        }
        true
    }

    pub(super) fn reconcile_dialogs(&mut self) {
        for index in 0..self.windows.len() {
            if self.windows[index].new {
                continue;
            }
            let root = self
                .tiled_window(&self.windows[index].proxy)
                .filter(|root| root.proxy != self.windows[index].proxy && !root.new)
                .map(|root| (root.output.clone(), root.workspace, root.column));
            if let Some((output, workspace, column)) = root {
                let window = &mut self.windows[index];
                if !window.dialog {
                    window.dialog = true;
                    window.proxy.set_tiled(Edges::empty());
                    window.natural_dimensions = (window.width > 0 && window.height > 0)
                        .then_some((window.width, window.height));
                    window.requested_dimensions = None;
                }
                if window.output != output {
                    window.output_removed();
                }
                window.output = output;
                window.workspace = workspace;
                window.column = column;
            } else if self.windows[index].dialog {
                let column = self.allocate_column();
                let window = &mut self.windows[index];
                window.dialog = false;
                window.column = column;
                window.tile = None;
                window.requested_dimensions = None;
                window.proxy.set_tiled(Edges::all());
            }
        }
    }

    /// Resolve parents before children, regardless of the global deque order.
    pub(super) fn dialog_order(&self) -> Vec<usize> {
        let mut focus_family = Vec::new();
        let mut focused = self
            .active_output
            .as_ref()
            .and_then(|id| self.outputs.get(id))
            .and_then(|output| output.workspaces.current().focused.clone());
        while let Some(proxy) = focused.take() {
            if focus_family.contains(&proxy) {
                break;
            }
            focused = self
                .windows
                .iter()
                .find(|window| window.proxy == proxy)
                .and_then(|window| window.parent.clone());
            focus_family.push(proxy);
        }
        let mut dialogs: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(_, window)| window.dialog)
            .map(|(index, _)| index)
            .collect();
        dialogs.sort_by_key(|index| {
            let mut depth = 0;
            let mut window = &self.windows[*index];
            while let Some(parent) = window
                .parent
                .as_ref()
                .and_then(|parent| self.windows.iter().find(|window| &window.proxy == parent))
            {
                depth += 1;
                if depth >= self.windows.len() {
                    break;
                }
                window = parent;
            }
            (
                depth,
                !focus_family.contains(&self.windows[*index].proxy),
                std::cmp::Reverse(*index),
            )
        });
        dialogs
    }

    pub(super) fn layout_dialogs(&mut self) {
        for index in self.dialog_order() {
            self.windows[index].tile = self.dialog_geometry(index, false);
        }
    }

    pub(super) fn dialog_geometry(&self, index: usize, displayed: bool) -> Option<TileGeometry> {
        let window = &self.windows[index];
        let parent = window
            .parent
            .as_ref()
            .and_then(|parent| self.windows.iter().find(|window| &window.proxy == parent))?;
        let area = outer_area(
            self.outputs.get(window.output.as_ref()?)?.work_area(),
            self.config.gaps.outer,
        );
        let parent_tile = if parent.fullscreen {
            let geometry = self.outputs.get(parent.output.as_ref()?)?.geometry;
            Some(TileGeometry {
                x: geometry.x,
                y: geometry.y,
                width: geometry.width,
                height: geometry.height,
                border: 0,
            })
        } else if displayed {
            parent.animation.tile()
        } else {
            parent.tile
        }?;
        dialog_tile(
            parent_tile,
            area,
            window.natural_dimensions.unwrap_or((640, 480)),
            self.config.border.width,
            window.tile_width.soft_fullscreen,
        )
    }

    pub(super) fn raise_dialogs(&self) {
        // Focused branches stay above sibling dialogs; otherwise newest wins.
        // Nested children follow their parent's final position.
        let dialogs = self.dialog_order();
        for index in dialogs {
            let window = &self.windows[index];
            if let Some(parent) = window
                .parent
                .as_ref()
                .and_then(|parent| self.windows.iter().find(|window| &window.proxy == parent))
            {
                window.node.place_above(&parent.node);
            }
        }
    }

    pub(super) fn reconcile_closed_parents(&mut self) {
        let surviving_parent = |proxy: &RiverWindowV1| {
            let mut window = self.windows.iter().find(|window| &window.proxy == proxy)?;
            for _ in 0..self.windows.len() {
                if !window.closed {
                    return Some(window.proxy.clone());
                }
                window = window.parent.as_ref().and_then(|parent| {
                    self.windows.iter().find(|window| &window.proxy == parent)
                })?;
            }
            None
        };
        for output in self.outputs.values_mut() {
            for workspace in &mut output.workspaces.entries {
                workspace.focused = workspace.focused.as_ref().and_then(surviving_parent);
            }
        }
        let parents: Vec<_> = self
            .windows
            .iter()
            .map(|window| window.parent.as_ref().and_then(surviving_parent))
            .collect();
        for (window, parent) in self.windows.iter_mut().zip(parents) {
            window.parent = parent;
        }
    }
}
