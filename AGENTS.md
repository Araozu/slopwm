# Agent guidance

slopwm is a small Rust 2024 scrolling tiling window manager running as a Wayland client
of River. Read `README.md` for usage and `docs/README.md` for development notes.

- Keep changes small and follow the existing module boundaries: `src/app.rs`
  handles the connection/event loop, `src/config.rs` and `src/action.rs` handle
  configuration/actions, and `src/wm/` owns window-management state and policy.
- Read `docs/protocol.md` before changing protocol behavior. Accumulate event
  state, issue requests at the appropriate manage/render boundaries, and finish
  every sequence, even when no changes are needed. Keep requested dimensions
  separate from confirmed dimensions.
- `src/protocol.rs` generates bindings from `protocol/*.xml`; do not check in
  generated Rust. Preserve upstream license notices and follow
  `protocol/README.md` when updating XML. Gate newer requests by negotiated versions.
- For Rust changes, run `cargo fmt --check`, `cargo check`, and `cargo test`.
  Test behavior and invariants rather than duplicating implementation details.
  Building requires the `libxkbcommon` development files.
- Validate configuration changes without a Wayland session using
  `cargo run -- --config config.example.yaml --check-config`. Keep the example
  and README consistent with configuration behavior.
- Check focus, move/resize, and lifecycle changes in a controlled River session;
  `WAYLAND_DEBUG=1` enables protocol logging. See `docs/implementation-plan.md`
  for scenarios. Report which checks ran and any that could not run.
- Preserve existing SPDX attribution and unrelated working-tree changes.
  Stage only files belonging to the requested task.

## Preferences

- Focus never follows mouse. Explicit keybinds will change focus on window or screen
