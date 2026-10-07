# Rust implementation and next steps

The imported tinyrwm floating implementation is split into modules by
responsibility, with released River v0.4.8 protocols and a dependency lockfile.
Keyboard bindings and spawned commands are configured through YAML at startup.
The final layout style and workspace model are still decisions to make.

## Module boundaries

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | Application entry point |
| `src/config.rs` | YAML loading, XDG paths, key names, actions, and validation |
| `src/action.rs` | Actions shared by configuration and input dispatch |
| `src/app.rs` | Connection, registry negotiation, startup checks, and event loop |
| `src/protocol.rs` | Generated River bindings and interface imports |
| `src/wm/mod.rs` | Manager state, object creation/cleanup, and manage/render sequence orchestration |
| `src/wm/window.rs` | Window state, geometry, and window-event dispatch |
| `src/wm/output.rs` | Output state and output-event dispatch |
| `src/wm/seat.rs` | Seat state, focus policy, action execution, and seat-event dispatch |
| `src/wm/bindings.rs` | Configured keyboard/default pointer bindings, binding lifecycle, and event dispatch |
| `src/wm/operation.rs` | Pointer move/resize state, dimension proposals, and render positioning |
| `protocol/*.xml` | Reviewed protocol definitions with their original notices |

Each object's dispatch implementation lives beside its state and behavior.
Incoming object and binding events accumulate pending state; the manager invokes
policy and pointer-operation helpers at the manage/render boundaries and sends
the finish requests. Manager internals stay within the `wm` module. The event
loop still uses one queue and thread, as in the example.

As layout policy grows, extract geometry calculations that can be independent
of Wayland proxies. Introduce an event-loop framework when timers or IPC
actually require one.

Pointer operations retain their starting geometry separately from the
application's confirmed dimensions. Seat state tracks focus, hovered windows,
cumulative motion, and pending actions explicitly. Future layout state should
also store desired placement and dimensions separately from confirmed geometry.
Keep focus order separate from render order if the eventual policy requires
them to differ.

The current handlers propose dimensions during manage and reconcile geometry
against confirmed dimensions during render. Both handlers complete each
sequence even when no updates are needed. Future backend hardening should
represent the current phase explicitly and validate requests against the
manage/render rules.

## Milestones

1. **Imported baseline.** Floating windows, click-to-focus/raise, focus cycling,
   `foot` spawning, window closure, and pointer move/resize are present. Generated
   bindings negotiate management 4–5 and XKB 1–3 using released, documented XML.
2. **Harden the small floating manager.** Keyboard bindings and terminal/command
   spawning are now configurable with YAML; `--check-config` validates offline.
   Add complete object cleanup and graceful manager shutdown, and replace brittle
   state lookups with tolerant handling of obsolete objects. See the
   [reference gaps](rust-demo.md#gaps-to-address-in-slopwm).
3. **Track real output and application state.** Store output rectangles, move
   windows to a remaining output after removal, handle size changes and parent
   relationships, implement fullscreen, and publish accurate capabilities.
4. **Choose and implement slopwm's layout policy.** Add a tiling algorithm or
   richer floating behavior, then visibility/workspace actions and configuration
   for those policies.
   Reuse the protocol backend rather than embedding requests in the algorithm.
5. **Make daily operation predictable.** Add useful diagnostics, configuration
   reload through `manage_dirty()`, lock-aware bindings, and any needed timer/IPC
   integration. Gate optional protocol extensions by negotiated versions.

Bars, launchers, custom titlebars, and animation can follow a working manager.
Use compositor-drawn borders first if decoration is needed; custom decoration
surfaces introduce buffer creation and commit synchronization work.

## Validation

Run `cargo fmt --check`, `cargo check`, and `cargo test` for implementation changes.
Configuration tests cover replacement/default behavior, command arguments, XKB
names/modifiers, invalid or conflicting bindings, and config-path fallback. For pure
layout code, test invariants that can actually fail: positive content sizes,
correct distribution of leftover pixels, negative output origins, and correct
top/left resize anchoring. Avoid tests that just repeat request-building code.

Use a controlled River session to check behavior beyond compilation:

- Open several windows, change focus, close a focused window, and keep typing
  into another application.
- Resize a terminal whose actual size differs from the proposal; confirm the
  manager consumes returned dimensions and later render-only sequences.
- Release a pointer operation and close a window during one; verify operations
  and seat references are cleaned up.
- Reconfigure/remove an output and leave fullscreen; verify restored geometry
  and focus point to surviving objects.
- Stop or restart the manager while applications remain open; distinguish this
  from the explicit command that exits the entire session.
- Check startup against missing/older globals and an unavailable manager; report
  a useful error without issuing unsupported requests.

Protocol logging with `WAYLAND_DEBUG=1` should show ordered finish requests,
with no per-frame roundtrip introduced by our event loop. Interactive checks
must confirm responsiveness as well as correct request ordering.
