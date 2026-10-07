// SPDX-License-Identifier: 0BSD

//! Column ordering, vertical stacks, and keyboard layout policy.

use wayland_backend::client::ObjectId;

use crate::config::GrowthDirection;
use crate::protocol::river_window_v1::RiverWindowV1;

use super::{WindowManager, layout::scrolling_tiles, window::Window};

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

    pub(super) fn insert_window(&mut self, mut window: Window, output_id: ObjectId) {
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
            self.leave_other_fullscreen(proxy, &output, workspace);
            for window in &mut self.windows {
                if window.column == column && &window.proxy != proxy {
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
        for window in &mut self.windows {
            if &window.proxy != focused
                && window.output.as_ref() == Some(output)
                && window.workspace == workspace
            {
                window.fullscreen = false;
                window.fullscreen_requested = None;
            }
        }
    }

    fn column_indices(&self, output: &ObjectId, workspace: u64) -> Vec<Vec<usize>> {
        let mut columns: Vec<Vec<usize>> = Vec::new();
        for (index, window) in self.windows.iter().enumerate() {
            if window.output.as_ref() != Some(output) || window.workspace != workspace {
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
        let focused = workspace.focused.as_ref()?;
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

    pub(super) fn cycle_window(&mut self, previous: bool) {
        let Some(FocusedColumn {
            columns,
            column,
            row,
        }) = self.focused_column()
        else {
            return;
        };
        let next = cycle_index(column, columns.len(), previous);
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
        let index = rows[cycle_index(row, rows.len(), up)];
        self.select_window(&self.windows[index].proxy.clone());
    }

    pub(super) fn cycle_output(&mut self, previous: bool) {
        let outputs = self.ordered_outputs();
        if outputs.is_empty() {
            return;
        }
        let current = outputs
            .iter()
            .position(|id| Some(id) == self.active_output.as_ref())
            .unwrap_or(0);
        self.active_output = Some(outputs[cycle_index(current, outputs.len(), previous)].clone());
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
        let Some(window) = self.windows.iter().find(|window| &window.proxy == focused) else {
            return;
        };
        let column = window.column;
        let mut width = window.tile_width;
        width.change(delta);
        for window in &mut self.windows {
            if window.column == column {
                window.tile_width = width;
                window.fullscreen = false;
            }
        }
    }

    pub(super) fn layout_windows(&mut self) {
        let outputs: Vec<_> = self.outputs.keys().cloned().collect();
        for id in outputs {
            let output = &self.outputs[&id];
            for workspace in &output.workspaces.entries {
                let columns = self.column_indices(&id, workspace.id);
                let soft: Vec<_> = columns
                    .iter()
                    .map(|rows| {
                        rows.iter()
                            .copied()
                            .find(|index| self.windows[*index].tile_width.soft_fullscreen)
                    })
                    .collect();
                let widths: Vec<_> = columns
                    .iter()
                    .zip(&soft)
                    .map(|(rows, soft)| {
                        if soft.is_some() {
                            98
                        } else {
                            self.windows[rows[0]].tile_width.percent()
                        }
                    })
                    .collect();
                let focused = columns
                    .iter()
                    .position(|rows| {
                        rows.iter().any(|index| {
                            Some(&self.windows[*index].proxy) == workspace.focused.as_ref()
                        })
                    })
                    .unwrap_or(0);
                let tiles =
                    scrolling_tiles(output.geometry, &widths, focused, self.config.border.width);
                for ((rows, soft), tile) in columns.into_iter().zip(soft).zip(tiles) {
                    if let Some(selected) = soft {
                        for index in rows {
                            self.windows[index].tile = (index == selected).then_some(tile);
                        }
                    } else {
                        let tiles = tile.split_vertical(rows.len());
                        for (index, tile) in rows.into_iter().zip(tiles) {
                            self.windows[index].tile = Some(tile);
                        }
                    }
                }
            }
        }
    }
}

fn insertion_index(focused: Option<usize>, len: usize, direction: GrowthDirection) -> usize {
    focused.map_or(len, |index| {
        index + usize::from(direction == GrowthDirection::Right)
    })
}

fn cycle_index(current: usize, len: usize, previous: bool) -> usize {
    if previous {
        (current + len - 1) % len
    } else {
        (current + 1) % len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn growth_inserts_beside_focus_and_navigation_wraps_in_spatial_order() {
        let mut strip = vec!["old-left", "focused", "old-right"];
        strip.insert(
            insertion_index(Some(1), strip.len(), GrowthDirection::Left),
            "new",
        );
        assert_eq!(strip, ["old-left", "new", "focused", "old-right"]);
        assert_eq!(cycle_index(0, strip.len(), true), 3);
        assert_eq!(cycle_index(3, strip.len(), false), 0);
        assert_eq!(insertion_index(Some(1), 3, GrowthDirection::Right), 2);
        assert_eq!(insertion_index(None, 0, GrowthDirection::Left), 0);
    }
}
