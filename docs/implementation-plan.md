# Proposed Rust implementation

The tinyrwm floating implementation is now imported in `src/main.rs`, with
released River v0.4.8 protocols and a dependency lockfile. The module boundaries
below describe future refactoring. The final layout style, workspace model, and
configuration format are still decisions to make.

## Module boundaries

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | Parse startup options, connect, report errors, run the event loop |
| `src/protocol.rs` | Generated River bindings and interface imports |
| `src/backend.rs` | Registry negotiation, `Dispatch` implementations, translating events and outgoing requests |
| `src/state.rs` | Window, output, seat, and pending-intent state |
| `src/policy.rs` | Focus, visibility, application requests, and binding actions |
| `src/layout.rs` | Geometry calculations using policy state and logical output rectangles |
| `protocol/*.xml` | Reviewed protocol definitions with their original notices |

Keep geometry calculations independent of Wayland proxies where practical.
Centralize requests in the backend so sequence rules can be checked in one
place. Begin with a single event queue and thread, as in the example; introduce
an event-loop framework when timers or IPC actually require one.

Store a desired placement and dimensions separately from the application's
confirmed content dimensions. Track seat focus, hovered window, operation
origin, cumulative motion, and pending actions explicitly. Keep focus order
separate from render order if the eventual policy requires them to differ.

Represent the current sequence phase explicitly. Validate policy requests
against the manage phase, and rendering requests against manage/render phases.
Generate dimension proposals during manage; reconcile geometry against the
latest confirmed dimensions during render. Complete each sequence even when
the computed diff is empty.

## Milestones

1. **Imported baseline.** Floating windows, click-to-focus/raise, focus cycling,
   `foot` spawning, window closure, and pointer move/resize are present. Generated
   bindings negotiate management 4–5 and XKB 1–3 using released, documented XML.
2. **Harden the small floating manager.** Make the terminal configurable, add
   complete object cleanup and graceful manager shutdown, and replace brittle
   state lookups with tolerant handling of obsolete objects. See the
   [reference gaps](rust-demo.md#gaps-to-address-in-slopwm).
3. **Track real output and application state.** Store output rectangles, move
   windows to a remaining output after removal, handle size changes and parent
   relationships, implement fullscreen, and publish accurate capabilities.
4. **Choose and implement slopwm's layout policy.** Add a tiling algorithm or
   richer floating behavior, then visibility/workspace actions and configuration.
   Reuse the protocol backend rather than embedding requests in the algorithm.
5. **Make daily operation predictable.** Add useful diagnostics, configuration
   reload through `manage_dirty()`, lock-aware bindings, and any needed timer/IPC
   integration. Gate optional protocol extensions by negotiated versions.

Bars, launchers, custom titlebars, and animation can follow a working manager.
Use compositor-drawn borders first if decoration is needed; custom decoration
surfaces introduce buffer creation and commit synchronization work.

## Validation

Run `cargo fmt --check` and `cargo check` for implementation changes. For pure
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
