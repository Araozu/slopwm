// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Seat focus, queued keyboard actions, and seat events.

use std::collections::{HashMap, VecDeque};

use wayland_backend::client::ObjectId;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::action::Action;
use crate::app::AppData;
use crate::protocol::{
    river_layer_shell_seat_v1::RiverLayerShellSeatV1, river_seat_v1::RiverSeatV1,
    river_window_manager_v1::RiverWindowManagerV1, river_window_v1::RiverWindowV1,
};

use super::{WindowManager, bindings::XkbBinding};

#[derive(Debug)]
pub(super) struct Seat {
    pub(super) proxy: RiverSeatV1,
    pub(super) new: bool,
    pub(super) removed: bool,
    pub(super) focused: Option<RiverWindowV1>,
    pub(super) interacted: Option<RiverWindowV1>,
    pub(super) xkb_bindings: HashMap<ObjectId, XkbBinding>,
    pub(super) pending_actions: VecDeque<Action>,
    pub(super) layer_seat: Option<RiverLayerShellSeatV1>,
    /// A layer surface with exclusive keyboard focus (e.g. a launcher) owns
    /// focus until River sends focus_non_exclusive or focus_none. While set,
    /// window-manager focus requests are ignored by the compositor, so skip
    /// sending them.
    pub(super) layer_exclusive: bool,
    pub(super) layer_non_exclusive: bool,
    pub(super) focus_dirty: bool,
}

impl Seat {
    pub(super) fn new(proxy: RiverSeatV1) -> Self {
        Self {
            proxy,
            new: true,
            removed: false,
            focused: None,
            interacted: None,
            xkb_bindings: HashMap::new(),
            pending_actions: VecDeque::new(),
            layer_seat: None,
            layer_exclusive: false,
            layer_non_exclusive: false,
            focus_dirty: false,
        }
    }

    pub(super) fn do_actions(&mut self, wm: &mut WindowManager, proxy: &RiverWindowManagerV1) {
        // Locked sessions must not run queued bindings. River may still
        // deliver presses while locked, so discard them instead.
        if wm.session_locked || wm.quitting {
            self.pending_actions.clear();
            return;
        }
        while let Some(action) = self.pending_actions.pop_front() {
            let focused = wm
                .active_output
                .as_ref()
                .and_then(|id| wm.outputs.get(id))
                .and_then(|output| output.workspaces.current().focused.clone());
            if matches!(
                action,
                Action::FocusNext
                    | Action::FocusPrevious
                    | Action::FocusUp
                    | Action::FocusDown
                    | Action::FocusOutputNext
                    | Action::FocusOutputPrevious
                    | Action::FocusWorkspaceUp
                    | Action::FocusWorkspaceDown
            ) {
                self.layer_non_exclusive = false;
                self.focus_dirty = true;
            }
            match action {
                Action::Spawn(argv) => {
                    if let Err(error) = spawn_reaped(&argv) {
                        eprintln!("Failed to spawn {:?}: {error}", argv[0]);
                    }
                }
                Action::Close => {
                    if let Some(window) = focused.as_ref() {
                        window.close();
                    }
                }
                Action::FocusNext | Action::FocusPrevious => {
                    wm.focus_column(matches!(action, Action::FocusPrevious));
                }
                Action::FocusUp | Action::FocusDown => {
                    wm.focus_vertical(matches!(action, Action::FocusUp));
                }
                Action::MoveNext | Action::MovePrevious => {
                    wm.move_column(matches!(action, Action::MovePrevious));
                }
                Action::StackNext | Action::StackPrevious => {
                    wm.stack_window(matches!(action, Action::StackPrevious));
                }
                Action::Unstack => wm.unstack_window(),
                Action::CenterWindow => wm.center_window(),
                Action::AlignWindowRight => wm.align_window_right(),
                Action::Preselect(direction) => wm.preselect(direction),
                Action::CancelPreselection => wm.preselection = None,
                Action::ReloadConfig => wm.reload_requested = true,
                Action::Quit => {
                    wm.quitting = true;
                    self.pending_actions.clear();
                }
                Action::ChangeWidthPercent(delta) => {
                    if let Some(window) = focused.as_ref() {
                        wm.change_width(window, delta);
                    }
                }
                Action::FocusOutputNext | Action::FocusOutputPrevious => {
                    wm.focus_output(matches!(action, Action::FocusOutputPrevious));
                }
                Action::MoveToOutputNext | Action::MoveToOutputPrevious => {
                    wm.move_to_output(matches!(action, Action::MoveToOutputPrevious));
                }
                Action::FocusWorkspaceUp | Action::FocusWorkspaceDown => {
                    wm.focus_workspace(matches!(action, Action::FocusWorkspaceUp));
                }
                Action::MoveToWorkspaceUp | Action::MoveToWorkspaceDown => {
                    wm.move_to_workspace(matches!(action, Action::MoveToWorkspaceUp));
                }
                Action::ToggleSoftFullscreen | Action::ToggleFullscreen => {
                    if let Some(window) = wm
                        .windows
                        .iter_mut()
                        .find(|window| Some(&window.proxy) == focused.as_ref())
                    {
                        match action {
                            Action::ToggleSoftFullscreen => {
                                window.fullscreen = false;
                                window.fullscreen_requested = None;
                                window.soft_fullscreen_requested = None;
                                window.tile_width.soft_fullscreen =
                                    !window.tile_width.soft_fullscreen;
                            }
                            Action::ToggleFullscreen => {
                                window.fullscreen =
                                    !window.fullscreen_requested.unwrap_or(window.fullscreen);
                                window.fullscreen_requested = None;
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                Action::Exit => proxy.exit_session(),
            }
            self.sync_focus(wm);
        }
    }

    pub(super) fn sync_focus(&mut self, wm: &WindowManager) {
        // A layer surface with exclusive focus owns keyboard focus; the
        // compositor ignores our focus requests until it releases exclusivity.
        if self.layer_exclusive || wm.session_locked {
            return;
        }
        // Shared focus is intentional: every seat follows the active output's
        // selected window.
        let focused = wm
            .active_output
            .as_ref()
            .and_then(|id| wm.outputs.get(id))
            .and_then(|output| output.workspaces.current().focused.as_ref());
        if self.focused.as_ref() == focused && (self.layer_non_exclusive || !self.focus_dirty) {
            return;
        }
        self.layer_non_exclusive = false;
        self.focus_dirty = false;
        match focused {
            Some(window) => {
                self.proxy.focus_window(window);
                if let Some(window) = wm
                    .windows
                    .iter()
                    .find(|candidate| &candidate.proxy == window)
                {
                    if window.dialog
                        && let Some(parent) = wm.tiled_window(&window.proxy)
                    {
                        parent.node.place_top();
                    }
                    window.node.place_top();
                }
                self.focused = Some(window.clone());
            }
            None => {
                self.proxy.clear_focus();
                self.focused = None;
            }
        }
    }
}

fn spawn_reaped(argv: &[String]) -> std::io::Result<u32> {
    // Dropping a `Child` without waiting leaves a zombie: the kernel keeps
    // the exit status until the parent reaps it. Detach a small waiter
    // thread per child so short-lived launcher commands cannot accumulate.
    let mut child = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        // Keep protocol logging out of spawned applications.
        .env_remove("WAYLAND_DEBUG")
        .spawn()?;
    let pid = child.id();
    if std::thread::Builder::new()
        .name("slopwm-spawn-reaper".into())
        .stack_size(64 * 1024)
        .spawn(move || {
            let _ = child.wait();
        })
        .is_err()
    {
        // Thread creation failed; blocking here would stall the manage
        // sequence, so the child is left for process-exit cleanup.
        eprintln!("Failed to detach reaper for {:?}", argv[0]);
    }
    Ok(pid)
}

impl Dispatch<RiverSeatV1, ()> for AppData {
    fn event(
        state: &mut Self,
        proxy: &RiverSeatV1,
        event: <RiverSeatV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_seat_v1::Event;
        let Some(seat) = state.wm.seats.get_mut(&proxy.id()) else {
            return;
        };
        match event {
            Event::Removed => seat.removed = true,
            Event::WindowInteraction { window } => seat.interacted = Some(window),
            // Pointer motion never changes keyboard or monitor focus.
            Event::PointerEnter { .. }
            | Event::PointerLeave
            | Event::PointerPosition { .. }
            | Event::WlSeat { .. }
            | Event::ShellSurfaceInteraction { .. }
            | Event::OpDelta { .. }
            | Event::OpRelease => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn spawned_children_are_reaped_without_lingering_zombies() {
        let pid = spawn_reaped(&["true".into()]).expect("spawn true");
        // A zombie still has a /proc entry; a reaped child disappears.
        let path = format!("/proc/{pid}");
        let start = Instant::now();
        while std::path::Path::new(&path).exists() {
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "child {pid} still present after 2s"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn spawn_reports_missing_executables_instead_of_panicking() {
        assert!(spawn_reaped(&["slopwm-definitely-missing-binary".into()]).is_err());
    }
}
