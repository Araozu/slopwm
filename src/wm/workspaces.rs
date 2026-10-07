// SPDX-License-Identifier: 0BSD

//! Per-output dynamic workspaces, remembered focus, and workspace navigation.

use std::collections::HashSet;

use wayland_backend::client::ObjectId;

use crate::protocol::river_window_v1::RiverWindowV1;

use super::WindowManager;

#[derive(Debug)]
pub(super) struct Workspace {
    pub(super) id: u64,
    pub(super) focused: Option<RiverWindowV1>,
    /// Strip origin relative to the output's left edge. `None` means the next
    /// layout starts at the 1% left inset; otherwise minimal-scroll keeps a
    /// fitting focused tile stationary.
    pub(super) scroll: Option<i64>,
}

#[derive(Debug)]
pub(super) struct DetachedWorkspace {
    pub(super) output: ObjectId,
    pub(super) workspace: Workspace,
}

#[derive(Debug)]
pub(super) struct Workspaces {
    pub(super) entries: Vec<Workspace>,
    active: usize,
    next_id: u64,
}

impl Default for Workspaces {
    fn default() -> Self {
        Self {
            entries: vec![Workspace {
                id: 0,
                focused: None,
                scroll: None,
            }],
            active: 0,
            next_id: 1,
        }
    }
}

impl Workspaces {
    pub(super) fn current(&self) -> &Workspace {
        &self.entries[self.active]
    }

    pub(super) fn current_mut(&mut self) -> &mut Workspace {
        &mut self.entries[self.active]
    }

    pub(super) fn adjacent(&self, up: bool) -> Option<u64> {
        let index = if up {
            self.active.checked_sub(1)?
        } else {
            self.active + 1
        };
        self.entries.get(index).map(|workspace| workspace.id)
    }

    pub(super) fn activate(&mut self, id: u64) {
        self.active = self
            .entries
            .iter()
            .position(|workspace| workspace.id == id)
            .expect("Workspace not found");
    }

    // Imported occupied workspaces go immediately above the trailing empty one.
    pub(super) fn import(&mut self, focused: Option<RiverWindowV1>, scroll: Option<i64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let index = self.entries.len() - 1;
        self.entries.insert(
            index,
            Workspace {
                id,
                focused,
                scroll,
            },
        );
        if self.active >= index {
            self.active += 1;
        }
        id
    }

    pub(super) fn reconcile(&mut self, occupied: &HashSet<u64>) {
        let active = self.current().id;
        let empty = self.entries.last().unwrap().id;
        // Retain the existing bottom workspace if it is empty, even when active.
        // Every other empty workspace disappears, keeping occupied IDs stable.
        let fallback = self.entries[self.active..]
            .iter()
            .find(|workspace| occupied.contains(&workspace.id) || workspace.id == empty)
            .unwrap()
            .id;
        self.entries
            .retain(|workspace| occupied.contains(&workspace.id) || workspace.id == empty);
        if occupied.contains(&empty) {
            self.entries.push(Workspace {
                id: self.next_id,
                focused: None,
                scroll: None,
            });
            self.next_id += 1;
        }
        self.active = self
            .entries
            .iter()
            .position(|workspace| workspace.id == active)
            .unwrap_or_else(|| {
                self.entries
                    .iter()
                    .position(|workspace| workspace.id == fallback)
                    .unwrap()
            });
    }
}

impl WindowManager {
    pub(super) fn reconcile_workspaces(&mut self) {
        for (id, output) in &mut self.outputs {
            let occupied = self
                .windows
                .iter()
                .filter(|window| window.output.as_ref() == Some(id))
                .map(|window| window.workspace)
                .collect();
            output.workspaces.reconcile(&occupied);
            for workspace in &mut output.workspaces.entries {
                let windows = || {
                    self.windows.iter().filter(|window| {
                        window.output.as_ref() == Some(id) && window.workspace == workspace.id
                    })
                };
                if workspace
                    .focused
                    .as_ref()
                    .is_none_or(|focused| !windows().any(|window| &window.proxy == focused))
                {
                    workspace.focused = windows().next().map(|window| window.proxy.clone());
                }
            }
        }
    }

    pub(super) fn focus_workspace(&mut self, up: bool) {
        let Some(output) = self
            .active_output
            .as_ref()
            .and_then(|id| self.outputs.get_mut(id))
        else {
            return;
        };
        if let Some(id) = output.workspaces.adjacent(up) {
            output.workspaces.activate(id);
        }
    }

    pub(super) fn move_to_workspace(&mut self, up: bool) {
        let Some(output_id) = self.active_output.clone() else {
            return;
        };
        let output = &self.outputs[&output_id];
        let Some(target) = output.workspaces.adjacent(up) else {
            return;
        };
        let focused = output
            .workspaces
            .current()
            .focused
            .as_ref()
            .and_then(|proxy| self.tiled_window(proxy))
            .map(|window| window.proxy.clone());
        let Some(index) = self
            .windows
            .iter()
            .position(|window| Some(&window.proxy) == focused.as_ref())
        else {
            return;
        };
        let mut window = self.windows.remove(index).unwrap();
        // Detaching a row creates its own column; leave the old stack intact.
        window.column = self.allocate_column();
        window.workspace = target;
        window.fullscreen = false;
        window.fullscreen_requested = None;
        window.tile_width.soft_fullscreen = false;
        window.soft_fullscreen_requested = None;
        self.outputs
            .get_mut(&output_id)
            .unwrap()
            .workspaces
            .activate(target);
        self.insert_window(window, output_id);
        self.reconcile_workspaces();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(workspaces: &Workspaces) -> Vec<u64> {
        workspaces
            .entries
            .iter()
            .map(|workspace| workspace.id)
            .collect()
    }

    #[test]
    fn filling_bottom_creates_exactly_one_empty_and_navigation_stops_at_edges() {
        let mut workspaces = Workspaces::default();
        assert_eq!(ids(&workspaces), [0]);
        assert_eq!(workspaces.adjacent(true), None);
        assert_eq!(workspaces.adjacent(false), None);
        workspaces.reconcile(&HashSet::from([0]));
        assert_eq!(ids(&workspaces), [0, 1]);
        assert_eq!(workspaces.current().id, 0);
        workspaces.activate(1);
        assert_eq!(workspaces.adjacent(true), Some(0));
        assert_eq!(workspaces.adjacent(false), None);
        workspaces.reconcile(&HashSet::from([0, 1]));
        assert_eq!(ids(&workspaces), [0, 1, 2]);
        assert_eq!(workspaces.current().id, 1);
        workspaces.reconcile(&HashSet::from([0, 1]));
        assert_eq!(ids(&workspaces), [0, 1, 2]);
    }

    #[test]
    fn empty_top_and_middle_are_removed_without_changing_selected_occupied_workspace() {
        let mut workspaces = Workspaces::default();
        for occupied in [vec![0], vec![0, 1], vec![0, 1, 2]] {
            workspaces.reconcile(&occupied.into_iter().collect());
        }
        workspaces.activate(2);
        workspaces.reconcile(&HashSet::from([2]));
        assert_eq!(ids(&workspaces), [2, 3]);
        assert_eq!(workspaces.current().id, 2);
        assert_eq!(workspaces.adjacent(true), None);
    }

    #[test]
    fn removing_active_workspace_selects_next_below_and_last_close_leaves_one_empty() {
        let mut workspaces = Workspaces::default();
        workspaces.reconcile(&HashSet::from([0]));
        workspaces.reconcile(&HashSet::from([0, 1]));
        workspaces.activate(0);
        workspaces.reconcile(&HashSet::from([1]));
        assert_eq!(ids(&workspaces), [1, 2]);
        assert_eq!(workspaces.current().id, 1);
        workspaces.reconcile(&HashSet::new());
        assert_eq!(ids(&workspaces), [2]);
        assert_eq!(workspaces.current().id, 2);
    }

    #[test]
    fn screens_keep_independent_workspace_lists_and_selections() {
        let mut first = Workspaces::default();
        let mut second = Workspaces::default();
        first.reconcile(&HashSet::from([0]));
        first.activate(1);
        second.reconcile(&HashSet::from([0]));
        second.reconcile(&HashSet::from([0, 1]));
        assert_eq!(ids(&first), [0, 1]);
        assert_eq!(first.current().id, 1);
        assert_eq!(ids(&second), [0, 1, 2]);
        assert_eq!(second.current().id, 0);
        first.reconcile(&HashSet::new());
        assert_eq!(ids(&second), [0, 1, 2]);
    }

    #[test]
    fn pruning_preserves_occupied_order_and_one_empty_for_every_selection() {
        for mask in 0..16 {
            for selected in 0..5 {
                let mut workspaces = Workspaces::default();
                for count in 1..=4 {
                    workspaces.reconcile(&(0..count).collect());
                }
                workspaces.activate(selected);
                let occupied: HashSet<_> = (0..4).filter(|id| mask & (1 << id) != 0).collect();
                workspaces.reconcile(&occupied);
                let remaining = ids(&workspaces);
                assert_eq!(remaining.len(), occupied.len() + 1);
                assert_eq!(remaining.last(), Some(&4));
                assert!(
                    remaining[..remaining.len() - 1]
                        .iter()
                        .all(|id| occupied.contains(id))
                );
                assert!(remaining.windows(2).all(|pair| pair[0] < pair[1]));
                let expected = (selected..=4)
                    .find(|id| occupied.contains(id) || *id == 4)
                    .unwrap();
                assert_eq!(workspaces.current().id, expected);
                workspaces.reconcile(&occupied);
                assert_eq!(ids(&workspaces), remaining);
            }
        }
    }

    #[test]
    fn imported_workspaces_preserve_selection_and_trailing_empty() {
        let mut workspaces = Workspaces::default();
        workspaces.reconcile(&HashSet::from([0]));
        workspaces.activate(1);
        let imported = workspaces.import(None, Some(42));
        workspaces.reconcile(&HashSet::from([0, imported]));
        assert_eq!(ids(&workspaces), [0, imported, 1]);
        assert_eq!(workspaces.current().id, 1);
        assert_eq!(workspaces.adjacent(true), Some(imported));
        assert_eq!(
            workspaces
                .entries
                .iter()
                .find(|workspace| workspace.id == imported)
                .unwrap()
                .scroll,
            Some(42)
        );
    }
}
