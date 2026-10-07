// SPDX-License-Identifier: 0BSD

//! Actions queued by input bindings and executed during a manage sequence.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpawnDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    Spawn(Vec<String>),
    Close,
    FocusNext,
    FocusPrevious,
    FocusUp,
    FocusDown,
    MoveNext,
    MovePrevious,
    StackNext,
    StackPrevious,
    Unstack,
    CenterWindow,
    AlignWindowRight,
    FocusOutputNext,
    FocusOutputPrevious,
    MoveToOutputNext,
    MoveToOutputPrevious,
    FocusWorkspaceUp,
    FocusWorkspaceDown,
    MoveToWorkspaceUp,
    MoveToWorkspaceDown,
    ToggleSoftFullscreen,
    ToggleFullscreen,
    ChangeWidthPercent(i16),
    Preselect(SpawnDirection),
    CancelPreselection,
    Exit,
}
