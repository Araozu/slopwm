# Rust implementation and next steps

The tinyrwm-derived implementation uses scrolling columns with optional vertical
stacks, released River v0.4.8 protocols, and a dependency lockfile. YAML configures
keyboard actions, borders, initial widths, and per-monitor growth directions.

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
| `src/wm/bindings.rs` | Configured keyboard bindings, binding lifecycle, and event dispatch |
| `src/wm/columns.rs` | Column ordering, stack/unstack, focus navigation, and layout policy |
| `src/wm/layout.rs` | Pure scrolling geometry, borders, width state, and vertical splitting |
| `protocol/*.xml` | Reviewed protocol definitions with their original notices |

Each object's dispatch implementation lives beside its state and behavior.
Incoming object and binding events accumulate pending state; the manager invokes
policy helpers at the manage/render boundaries and sends the finish requests.
Manager internals stay within the `wm` module. The event
loop still uses one queue and thread, as in the example.

Geometry calculations in `layout.rs` are independent of Wayland proxies.
Windows retain width percentages and column membership separately from desired
tile rectangles, cached dimension proposals, and confirmed content dimensions.
Focus and render order are independent of stable column/row order. Scrolled-away
windows keep their monitor membership; visibility is never used to infer it.
Floating pointer operations are removed. Pointer motion does not change focus.

The current handlers propose dimensions during manage and reconcile geometry
against confirmed dimensions during render. Both handlers complete each
sequence even when no updates are needed. Future backend hardening should
represent the current phase explicitly and validate requests against the
manage/render rules.

## Milestones

1. **Imported baseline.** The original floating manager supplied click-to-focus,
   focus cycling, `foot` spawning, and pointer operations. Scrolling tiling now
   replaces its floating geometry and pointer move/resize policy. Generated
   bindings negotiate management 4–5 and XKB 1–3 using released, documented XML.
2. **Harden the manager.** Keyboard bindings and terminal/command
   spawning are now configurable with YAML; `--check-config` validates offline.
   Closed window/node and removed output/seat objects are destroyed. Add graceful
   manager shutdown and tolerant handling of remaining obsolete-object lookups.
   See the [reference gaps](rust-demo.md#gaps-to-address-in-slopwm).
3. **Track real output and application state.** Output rectangles, stable active
   monitor placement, monitor-switching bindings, and recovery after output
   removal are present. Fullscreen/maximize capabilities match implemented
   behavior. Application dimensions are confirmed separately and clipped to
   allocations. Parent relationships remain future work.
4. **Scrolling layout.** Stable-width columns grow left by default, with
   per-monitor overrides. Stack/unstack actions support vertical rows. The
   focused column keeps a 1% left inset; soft fullscreen occupies 98% width and
   full height, while true fullscreen delegates to River. Workspace policy
   remains future work.
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
vertical distribution of leftover pixels, negative output origins, stable widths
while scrolling, fullscreen restoration, and neighbor peeks. Avoid tests that
just repeat request-building code.

Use a controlled River session to check behavior beyond compilation:

- Open several windows, change focus, close a focused window, and keep typing
  into another application.
- Resize a terminal whose actual size differs from the proposal; confirm the
  manager consumes returned dimensions and later render-only sequences.
- Stack/unstack windows, focus rows, toggle soft fullscreen, and close a row;
  verify height restoration, stable widths, and cleaned-up focus references.
- Move the pointer between windows/outputs; verify keyboard and monitor focus
  stay fixed. Click a visible neighboring tile and check explicit focus.
- Reconfigure/remove an output and leave fullscreen; verify restored geometry
  and focus point to surviving objects.
- Stop or restart the manager while applications remain open; distinguish this
  from the explicit command that exits the entire session.
- Check startup against missing/older globals and an unavailable manager; report
  a useful error without issuing unsupported requests.

Protocol logging with `WAYLAND_DEBUG=1` should show ordered finish requests,
with no per-frame roundtrip introduced by our event loop. Interactive checks
must confirm responsiveness as well as correct request ordering.

Scrolling-layout validation on 2026-10-06 passed formatting, compilation,
18 unit tests, and offline example configuration validation. An isolated River
0.4.8 session with two headless monitors and virtual input passed 20 checks for
growth direction, scrolling, keyboard/click focus, pointer focus invariance,
stack/unstack, fullscreen restoration, width steps, output removal, object
cleanup, and ordered manage/render completion. A separate XDG client deliberately
returned content 17 pixels wider and 11 pixels taller than requested; screenshot
pixels confirmed clipping, configured borders, the inset, and full output
coverage in true fullscreen. Its application maximize/fullscreen requests also
restored the expected tile. Physical-monitor interaction and older negotiated
protocol versions were not exercised in that session.
