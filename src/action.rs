// SPDX-License-Identifier: 0BSD

//! Actions queued by input bindings and executed during a manage sequence.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    Spawn(Vec<String>),
    Close,
    FocusNext,
    FocusPrevious,
    FocusUp,
    FocusDown,
    StackNext,
    StackPrevious,
    Unstack,
    FocusOutputNext,
    FocusOutputPrevious,
    ToggleSoftFullscreen,
    ToggleFullscreen,
    ChangeWidthPercent(i16),
    Exit,
}
