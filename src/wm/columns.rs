// SPDX-License-Identifier: 0BSD

//! Column ordering, vertical stacks, and keyboard layout policy.

use wayland_backend::client::ObjectId;

use crate::action::SpawnDirection;
use crate::config::GrowthDirection;
use crate::protocol::river_window_v1::RiverWindowV1;

use super::{
    WindowManager,
    layout::{
        centered_scroll, inset_for, outer_area, pixel_widths, right_aligned_scroll, scrolling_tiles,
    },
    preselection::Preselection,
    window::Window,
};

struct FocusedColumn {
    columns: Vec<Vec<usize>>,
    column: usize,
    row: usize,
}

impl WindowManager {
    pub(super) fn allocate_column(&mut self) -> u64 {
        self.next_column += 1;
        self.next_column
    }

    pub(super) fn insert_window(&mut self, window: Window, output_id: ObjectId) {
        let output = &self.outputs[&output_id];
        let name = output
            .wl_output_name
            .and_then(|id| self.output_names.get(&id))
            .map(String::as_str);
        let direction = self.config.growth_direction(name);
        let focused_column = output
            .workspaces
            .current()
            .focused
            .as_ref()
            .and_then(|focused| self.windows.iter().find(|window| &window.proxy == focused))
            .map(|window| window.column);
        let existing = self.windows.iter().rposition(|candidate| {
            candidate.output.as_ref() == Some(&output_id)
                && candidate.workspace == window.workspace
                && candidate.column == window.column
        });
        let focused = match direction {
            GrowthDirection::Left => self.windows.iter().position(|candidate| {
                candidate.output.as_ref() == Some(&output_id)
                    && candidate.workspace == window.workspace
                    && Some(candidate.column) == focused_column
            }),
            GrowthDirection::Right => self.windows.iter().rposition(|candidate| {
                candidate.output.as_ref() == Some(&output_id)
                    && candidate.workspace == window.workspace
                    && Some(candidate.column) == focused_column
            }),
        };
        let index = existing.map_or_else(
            || insertion_index(focused, self.windows.len(), direction),
            |index| index + 1,
        );
        self.insert_window_at(window, output_id, index);
    }

    pub(super) fn insert_preselected_window(
        &mut self,
        mut window: Window,
        selection: Preselection,
    ) {
        self.active_output = Some(selection.output.clone());
        self.outputs
            .get_mut(&selection.output)
            .unwrap()
            .workspaces
            .activate(selection.workspace);
        window.workspace = selection.workspace;
        let anchor = self.windows.iter().position(|candidate| {
            Some(&candidate.proxy) == selection.window.as_ref()
                && candidate.output.as_ref() == Some(&selection.output)
                && candidate.workspace == selection.workspace
        });
        let Some(anchor) = anchor else {
            // On an empty workspace all directions open the first column.
            self.insert_window(window, selection.output);
            return;
        };
        let column = self.windows[anchor].column;
        let rows: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| {
                (candidate.column == column
                    && candidate.output.as_ref() == Some(&selection.output)
                    && candidate.workspace == selection.workspace)
                    .then_some(index)
            })
            .collect();
        let index = match selection.direction {
            SpawnDirection::Left => rows[0],
            SpawnDirection::Right => rows.last().unwrap() + 1,
            SpawnDirection::Up | SpawnDirection::Down => {
                window.column = column;
                window.tile_width = self.windows[anchor].tile_width;
                window.tile_width.soft_fullscreen = false;
                for row in &rows {
                    self.windows[*row].tile_width.soft_fullscreen = false;
                }
                anchor + usize::from(selection.direction == SpawnDirection::Down)
            }
        };
        self.insert_window_at(window, selection.output, index);
    }

    fn insert_window_at(&mut self, mut window: Window, output_id: ObjectId, index: usize) {
        window.output = Some(output_id.clone());
        self.leave_other_fullscreen(&window.proxy, &output_id, window.workspace);
        self.outputs
            .get_mut(&output_id)
            .unwrap()
            .workspaces
            .current_mut()
            .focused = Some(window.proxy.clone());
        self.windows.insert(index, window);
    }

    pub(super) fn select_window(&mut self, proxy: &RiverWindowV1) {
        if let Some(output) = self
            .windows
            .iter()
            .find(|window| &window.proxy == proxy)
            .and_then(|window| window.output.clone())
            .filter(|id| self.outputs.contains_key(id))
        {
            let selected = self
                .windows
                .iter()
                .find(|window| &window.proxy == proxy)
                .unwrap();
            let (column, workspace) = (selected.column, selected.workspace);
            let root = self.tiled_window(proxy).map(|window| window.proxy.clone());
            self.leave_other_fullscreen(proxy, &output, workspace);
            for window in &mut self.windows {
                if !window.dialog && window.column == column && Some(&window.proxy) != root.as_ref()
                {
                    window.tile_width.soft_fullscreen = false;
                }
            }
            let workspaces = &mut self.outputs.get_mut(&output).unwrap().workspaces;
            workspaces.activate(workspace);
            workspaces.current_mut().focused = Some(proxy.clone());
            self.active_output = Some(output);
        }
    }

    fn leave_other_fullscreen(
        &mut self,
        focused: &RiverWindowV1,
        output: &ObjectId,
        workspace: u64,
    ) {
        let mut family = vec![focused.clone()];
        while let Some(parent) = family
            .last()
            .and_then(|proxy| self.windows.iter().find(|window| &window.proxy == proxy))
            .and_then(|window| window.parent.clone())
        {
            if family.contains(&parent) {
                break;
            }
            family.push(parent);
        }
        for window in &mut self.windows {
            if &window.proxy != focused
                && !family.contains(&window.proxy)
                && window.output.as_ref() == Some(output)
                && window.workspace == workspace
            {
                window.fullscreen = false;
                window.fullscreen_requested = None;
            }
        }
    }

    pub(super) fn reconcile_soft_fullscreen_focus(&mut self) {
        // An application maximize request marks a row soft fullscreen, which
        // hides its siblings during layout. If the hidden row holds keyboard
        // focus, typing would continue into an invisible tile. Move focus to
        // the visible soft row instead, mirroring sibling-focus restoration.
        let output_ids: Vec<ObjectId> = self.outputs.keys().cloned().collect();
        let mut corrections = Vec::new();
        for output_id in &output_ids {
            let focused_workspaces: Vec<(u64, Option<RiverWindowV1>)> = self.outputs[output_id]
                .workspaces
                .entries
                .iter()
                .map(|workspace| (workspace.id, workspace.focused.clone()))
                .collect();
            for (workspace_id, focused) in focused_workspaces {
                let Some(focused_proxy) = focused else {
                    continue;
                };
                let focused_proxy = self
                    .tiled_window(&focused_proxy)
                    .map(|window| window.proxy.clone());
                let Some(focused_index) = self
                    .windows
                    .iter()
                    .position(|window| Some(&window.proxy) == focused_proxy.as_ref())
                else {
                    continue;
                };
                let columns = self.column_indices(output_id, workspace_id);
                if let Some(replacement) = soft_focus_replacement(
                    &columns,
                    |index| self.windows[index].tile_width.soft_fullscreen,
                    focused_index,
                ) {
                    corrections.push((
                        output_id.clone(),
                        workspace_id,
                        self.windows[replacement].proxy.clone(),
                    ));
                }
            }
        }
        for (output_id, workspace_id, proxy) in corrections {
            if let Some(output) = self.outputs.get_mut(&output_id)
                && let Some(workspace) = output
                    .workspaces
                    .entries
                    .iter_mut()
                    .find(|workspace| workspace.id == workspace_id)
            {
                workspace.focused = Some(proxy);
            }
        }
    }

    fn column_indices(&self, output: &ObjectId, workspace: u64) -> Vec<Vec<usize>> {
        let mut columns: Vec<Vec<usize>> = Vec::new();
        for (index, window) in self.windows.iter().enumerate() {
            if window.dialog
                || window.output.as_ref() != Some(output)
                || window.workspace != workspace
            {
                continue;
            }
            if let Some(column) = columns
                .iter_mut()
                .find(|column| self.windows[column[0]].column == window.column)
            {
                column.push(index);
            } else {
                columns.push(vec![index]);
            }
        }
        columns
    }

    fn focused_column(&self) -> Option<FocusedColumn> {
        let output_id = self.active_output.as_ref()?;
        let workspace = self.outputs[output_id].workspaces.current();
        let focused = &self.tiled_window(workspace.focused.as_ref()?)?.proxy;
        let columns = self.column_indices(output_id, workspace.id);
        let (column, row) = columns.iter().enumerate().find_map(|(column, rows)| {
            rows.iter()
                .position(|index| &self.windows[*index].proxy == focused)
                .map(|row| (column, row))
        })?;
        Some(FocusedColumn {
            columns,
            column,
            row,
        })
    }

    pub(super) fn focus_column(&mut self, previous: bool) {
        let Some(FocusedColumn {
            columns,
            column,
            row,
        }) = self.focused_column()
        else {
            return;
        };
        let Some(next) = adjacent_index(column, columns.len(), previous) else {
            return;
        };
        let index = columns[next][row.min(columns[next].len() - 1)];
        self.select_window(&self.windows[index].proxy.clone());
    }

    pub(super) fn focus_vertical(&mut self, up: bool) {
        let Some(FocusedColumn {
            columns,
            column,
            row,
        }) = self.focused_column()
        else {
            return;
        };
        let rows = &columns[column];
        let Some(next) = adjacent_index(row, rows.len(), up) else {
            return;
        };
        let index = rows[next];
        self.select_window(&self.windows[index].proxy.clone());
    }

    pub(super) fn focus_output(&mut self, previous: bool) {
        if let Some(output) = self.adjacent_output(previous) {
            self.active_output = Some(output);
        }
    }

    fn adjacent_output(&self, previous: bool) -> Option<ObjectId> {
        let outputs = self.ordered_outputs();
        let current = outputs
            .iter()
            .position(|id| Some(id) == self.active_output.as_ref())?;
        let next = adjacent_index(current, outputs.len(), previous)?;
        Some(outputs[next].clone())
    }

    pub(super) fn move_to_output(&mut self, previous: bool) {
        let Some(target) = self.adjacent_output(previous) else {
            return;
        };
        let Some(FocusedColumn {
            columns,
            column,
            row,
        }) = self.focused_column()
        else {
            return;
        };
        let mut window = self.windows.remove(columns[column][row]).unwrap();
        // Detach only the focused row, preserving the source stack and width.
        window.column = self.allocate_column();
        window.workspace = self.outputs[&target].workspaces.current().id;
        window.fullscreen = false;
        window.fullscreen_requested = None;
        window.tile_width.soft_fullscreen = false;
        window.soft_fullscreen_requested = None;
        self.active_output = Some(target.clone());
        self.insert_window(window, target);
        self.reconcile_workspaces();
    }

    pub(super) fn stack_window(&mut self, previous: bool) {
        let Some(FocusedColumn {
            columns,
            column,
            row,
        }) = self.focused_column()
        else {
            return;
        };
        let target = if previous {
            column.checked_sub(1)
        } else {
            (column + 1 < columns.len()).then_some(column + 1)
        };
        let Some(target) = target else {
            return;
        };
        let source = columns[column][row];
        let target_index = columns[target][0];
        let target_column = self.windows[target_index].column;
        let mut width = self.windows[target_index].tile_width;
        width.soft_fullscreen = false;
        let mut window = self.windows.remove(source).unwrap();
        window.column = target_column;
        window.tile_width = width;
        window.fullscreen = false;
        for sibling in &mut self.windows {
            if sibling.column == target_column {
                sibling.tile_width.soft_fullscreen = false;
            }
        }
        let index = self
            .windows
            .iter()
            .rposition(|sibling| sibling.column == target_column)
            .unwrap()
            + 1;
        let proxy = window.proxy.clone();
        self.windows.insert(index, window);
        self.select_window(&proxy);
    }

    pub(super) fn unstack_window(&mut self) {
        let Some(FocusedColumn {
            columns,
            column,
            row,
        }) = self.focused_column()
        else {
            return;
        };
        if columns[column].len() == 1 {
            return;
        }
        let mut window = self.windows.remove(columns[column][row]).unwrap();
        let original_column = window.column;
        window.column = self.allocate_column();
        window.tile_width.soft_fullscreen = false;
        window.fullscreen = false;
        let output = window.output.clone().unwrap();
        // Keep the original column as the insertion anchor after detaching its
        // focused window, so left/right growth applies to the whole stack.
        self.outputs
            .get_mut(&output)
            .unwrap()
            .workspaces
            .current_mut()
            .focused = self
            .windows
            .iter()
            .find(|sibling| sibling.column == original_column)
            .map(|sibling| sibling.proxy.clone());
        self.insert_window(window, output);
    }

    pub(super) fn change_width(&mut self, focused: &RiverWindowV1, delta: i16) {
        let Some(window) = self.tiled_window(focused) else {
            return;
        };
        let column = window.column;
        let mut width = window.tile_width;
        width.change(delta);
        for window in &mut self.windows {
            if !window.dialog && window.column == column {
                window.tile_width = width;
                window.fullscreen = false;
            }
        }
    }

    fn workspace_widths(&self, columns: &[Vec<usize>]) -> Vec<u8> {
        columns
            .iter()
            .map(|rows| {
                if rows
                    .iter()
                    .copied()
                    .any(|index| self.windows[index].tile_width.soft_fullscreen)
                {
                    98
                } else {
                    self.windows[rows[0]].tile_width.percent()
                }
            })
            .collect()
    }

    pub(super) fn layout_windows(&mut self) {
        for window in &mut self.windows {
            window.tile = None;
        }
        let output_ids: Vec<_> = self.outputs.keys().cloned().collect();
        for id in output_ids {
            let (geometry, border_width, inner_gap) = {
                let output = &self.outputs[&id];
                (
                    outer_area(output.work_area(), self.config.gaps.outer),
                    self.config.border.width,
                    i64::from(self.config.gaps.inner.max(0)),
                )
            };
            if geometry.width <= 0 || geometry.height <= 0 {
                continue;
            }
            let workspace_ids: Vec<u64> = self.outputs[&id]
                .workspaces
                .entries
                .iter()
                .map(|workspace| workspace.id)
                .collect();
            for workspace_id in workspace_ids {
                let columns = self.column_indices(&id, workspace_id);
                if columns.is_empty() {
                    continue;
                }
                let soft: Vec<_> = columns
                    .iter()
                    .map(|rows| {
                        rows.iter()
                            .copied()
                            .find(|index| self.windows[*index].tile_width.soft_fullscreen)
                    })
                    .collect();
                let widths = self.workspace_widths(&columns);
                let (focused_proxy, prev_scroll) = {
                    let workspace = self.outputs[&id]
                        .workspaces
                        .entries
                        .iter()
                        .find(|workspace| workspace.id == workspace_id)
                        .unwrap();
                    (
                        workspace
                            .focused
                            .as_ref()
                            .and_then(|proxy| self.tiled_window(proxy))
                            .map(|window| window.proxy.clone()),
                        workspace.scroll,
                    )
                };
                let focused = columns
                    .iter()
                    .position(|rows| {
                        rows.iter().any(|index| {
                            Some(&self.windows[*index].proxy) == focused_proxy.as_ref()
                        })
                    })
                    .unwrap_or(0);
                let prev = prev_scroll.unwrap_or_else(|| inset_for(geometry.width));
                let (tiles, scroll) =
                    scrolling_tiles(geometry, &widths, focused, border_width, prev, inner_gap);
                {
                    let workspace = self.outputs.get_mut(&id).unwrap();
                    let entry = workspace
                        .workspaces
                        .entries
                        .iter_mut()
                        .find(|workspace| workspace.id == workspace_id)
                        .unwrap();
                    entry.scroll = Some(scroll);
                }
                for ((rows, soft), tile) in columns.into_iter().zip(soft).zip(tiles) {
                    if let Some(selected) = soft {
                        for index in rows {
                            self.windows[index].tile = (index == selected).then_some(tile);
                        }
                    } else {
                        let tiles = tile.split_vertical(rows.len(), inner_gap);
                        for (index, tile) in rows.into_iter().zip(tiles) {
                            self.windows[index].tile = Some(tile);
                        }
                    }
                }
            }
        }
    }

    fn focused_scroll_target(
        &mut self,
        compute: impl FnOnce(i32, &[i64], usize, i64) -> Option<i64>,
    ) {
        let Some(output_id) = self.active_output.clone() else {
            return;
        };
        let (geometry, workspace_id, inner_gap) = {
            let Some(output) = self.outputs.get(&output_id) else {
                return;
            };
            (
                outer_area(output.work_area(), self.config.gaps.outer),
                output.workspaces.current().id,
                i64::from(self.config.gaps.inner.max(0)),
            )
        };
        if geometry.width <= 0 {
            return;
        }
        let columns = self.column_indices(&output_id, workspace_id);
        if columns.is_empty() {
            return;
        }
        let focused_proxy = self.outputs[&output_id]
            .workspaces
            .current()
            .focused
            .as_ref()
            .and_then(|proxy| self.tiled_window(proxy))
            .map(|window| window.proxy.clone());
        let focused = columns
            .iter()
            .position(|rows| {
                rows.iter()
                    .any(|index| Some(&self.windows[*index].proxy) == focused_proxy.as_ref())
            })
            .unwrap_or(0);
        let widths = self.workspace_widths(&columns);
        let widths_px = pixel_widths(geometry.width, &widths);
        let Some(scroll) = compute(geometry.width, &widths_px, focused, inner_gap) else {
            return;
        };
        let output = self.outputs.get_mut(&output_id).unwrap();
        output.workspaces.current_mut().scroll = Some(scroll);
    }

    pub(super) fn center_window(&mut self) {
        self.focused_scroll_target(|width, sizes, focused, gap| {
            centered_scroll(width, sizes, focused, gap)
        });
    }

    pub(super) fn align_window_right(&mut self) {
        self.focused_scroll_target(|width, sizes, focused, gap| {
            right_aligned_scroll(width, sizes, focused, gap)
        });
    }

    /// Move the focused column one step left/right, keeping its stacked rows
    /// together. Stops at either end of the strip.
    pub(super) fn move_column(&mut self, previous: bool) {
        let Some(FocusedColumn {
            columns,
            column,
            row: _,
        }) = self.focused_column()
        else {
            return;
        };
        let Some(target) = adjacent_index(column, columns.len(), previous) else {
            return;
        };
        // Reorder the two adjacent columns in the global deque by removing
        // both blocks and reinserting them swapped at the earliest position.
        // Unrelated workspaces keep their relative order; the moved columns
        // become contiguous, which also defragments previously interleaved
        // strips.
        let focused_id = self.windows[columns[column][0]].column;
        let target_id = self.windows[columns[target][0]].column;
        let mut indices: Vec<usize> = columns[column]
            .iter()
            .chain(columns[target].iter())
            .copied()
            .collect();
        indices.sort_unstable();
        let at = indices[0];
        let mut removed = Vec::with_capacity(indices.len());
        for index in indices.into_iter().rev() {
            removed.push(self.windows.remove(index).unwrap());
        }
        removed.reverse();
        let mut focused_block = Vec::new();
        let mut target_block = Vec::new();
        for window in removed {
            if window.column == focused_id {
                focused_block.push(window);
            } else if window.column == target_id {
                target_block.push(window);
            } else {
                // Only the two moved columns were removed.
                debug_assert!(false, "unexpected column in move");
                target_block.push(window);
            }
        }
        let ordered: Vec<_> = if previous {
            focused_block.into_iter().chain(target_block).collect()
        } else {
            target_block.into_iter().chain(focused_block).collect()
        };
        for (offset, window) in ordered.into_iter().enumerate() {
            self.windows.insert(at + offset, window);
        }
    }
}

fn insertion_index(focused: Option<usize>, len: usize, direction: GrowthDirection) -> usize {
    focused.map_or(len, |index| {
        index + usize::from(direction == GrowthDirection::Right)
    })
}

fn adjacent_index(current: usize, len: usize, previous: bool) -> Option<usize> {
    let next = if previous {
        current.checked_sub(1)
    } else {
        current.checked_add(1)
    };
    next.filter(|next| current < len && *next < len)
}

fn soft_focus_replacement(
    columns: &[Vec<usize>],
    is_soft: impl Fn(usize) -> bool,
    focused: usize,
) -> Option<usize> {
    let rows = columns.iter().find(|rows| rows.contains(&focused))?;
    let selected = rows.iter().copied().find(|index| is_soft(*index))?;
    (selected != focused).then_some(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn growth_inserts_beside_focus() {
        let mut strip = vec!["old-left", "focused", "old-right"];
        strip.insert(
            insertion_index(Some(1), strip.len(), GrowthDirection::Left),
            "new",
        );
        assert_eq!(strip, ["old-left", "new", "focused", "old-right"]);
        assert_eq!(insertion_index(Some(1), 3, GrowthDirection::Right), 2);
        assert_eq!(insertion_index(None, 0, GrowthDirection::Left), 0);
    }

    #[test]
    fn focus_navigation_stops_at_edges_and_visits_every_neighbor() {
        for len in 0_usize..6 {
            assert_eq!(adjacent_index(0, len, true), None);
            assert_eq!(adjacent_index(len.saturating_sub(1), len, false), None);
            for current in 1..len {
                assert_eq!(adjacent_index(current, len, true), Some(current - 1));
                assert_eq!(adjacent_index(current - 1, len, false), Some(current));
            }
        }
    }

    #[test]
    fn hidden_sibling_focus_moves_to_visible_soft_row() {
        let columns = vec![vec![0, 1], vec![2]];
        // Focused row hidden by its sibling's maximize request.
        assert_eq!(
            soft_focus_replacement(&columns, |index| index == 0, 1),
            Some(0)
        );
        // Focus already on the visible soft row stays put.
        assert_eq!(
            soft_focus_replacement(&columns, |index| index == 0, 0),
            None
        );
        // Columns without soft fullscreen keep their focus.
        assert_eq!(
            soft_focus_replacement(&columns, |index| index == 0, 2),
            None
        );
        assert_eq!(soft_focus_replacement(&columns, |_| false, 1), None);
        // Layout shows the first soft row, so focus follows it.
        assert_eq!(
            soft_focus_replacement(&columns, |index| index != 2, 1),
            Some(0)
        );
        // Stale focus outside every column is left alone.
        assert_eq!(
            soft_focus_replacement(&columns, |index| index == 0, 7),
            None
        );
    }
}
