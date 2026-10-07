# Rust implementation and next steps

The tinyrwm-derived implementation uses scrolling columns with optional vertical
stacks, released River v0.4.8 protocols, and a dependency lockfile. YAML configures
keyboard actions, global keyboard repeat rate/delay, borders, gaps, initial
widths, and per-monitor growth directions.

## Module boundaries

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | Application entry point |
| `src/config.rs` | YAML loading/reloading from a stable source, XDG paths, key names, actions, and validation |
| `src/action.rs` | Actions shared by configuration and input dispatch |
| `src/app.rs` | Connection, registry negotiation, startup checks, poll loop, reload staging, and graceful shutdown |
| `src/app/signals.rs` | Signal flags and a nonblocking self-pipe for idle reload/shutdown |
| `src/protocol.rs` | Generated River bindings and interface imports |
| `src/wm/mod.rs` | Manager state, object creation/cleanup, and manage/render sequence orchestration |
| `src/wm/window.rs` | Window state, geometry, and window-event dispatch |
| `src/wm/dialogs.rs` | Parent-aware floating placement, family membership, stacking, and parent-close recovery |
| `src/wm/output.rs` | Output state and output-event dispatch |
| `src/wm/seat.rs` | Seat state, focus policy, action execution, and seat-event dispatch |
| `src/wm/bindings.rs` | Configured keyboard bindings, binding lifecycle, and event dispatch |
| `src/wm/input.rs` | Global keyboard repeat configuration and input-device lifecycle |
| `src/wm/layer.rs` | Layer-shell enablement, default output, focus exclusivity, and event dispatch |
| `src/wm/columns.rs` | Column ordering, stack/unstack, focus navigation, column moves, scroll/center/right policy, and layout policy |
| `src/wm/workspaces.rs` | Per-output dynamic workspace lifecycle, remembered focus/scroll, navigation, and window moves |
| `src/wm/layout.rs` | Pure scrolling geometry, minimal-scroll/center/right math, borders, gaps, width state, and vertical splitting |
| `src/wm/animation.rs` | Pure tile interpolation, interrupted transitions, and displayed geometry |
| `src/wm/preselection.rs` | One-shot spawn targets, cancellation, and clipped directional preview geometry |
| `src/wm/overlay.rs` | Input-transparent River shell surface, shared-memory drawing, and synchronized commits |
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
Workspace IDs remain stable when empty workspaces are removed. Every output
keeps one trailing empty workspace, and each workspace remembers its own focus.
Layouts include inactive workspaces, while render visibility includes only the
selected workspace on each output, including for true fullscreen windows.
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
   Closed window/node and removed output/seat objects are destroyed. Graceful
   shutdown stops both managers, finishes pending sequences, and destroys objects
   after their acknowledgements. Obsolete binding/output events are tolerated.
   See the [reference gaps](rust-demo.md#gaps-to-address-in-slopwm).
3. **Track real output and application state.** Output rectangles, stable active
   monitor placement, monitor-switching bindings, and recovery after output
   removal are present. Fullscreen/maximize capabilities match implemented
   behavior. Application dimensions are confirmed separately and clipped to
   allocations. Parent-aware dialogs float above their parent, follow its monitor
   and workspace, preserve pending spawn selections, and restore focus when
   closed. Nested dialogs and orphaned children are reconciled explicitly.
4. **Scrolling layout.** Stable-width columns grow left by default, with
   per-monitor overrides. Stack/unstack actions support vertical rows. Each
   workspace keeps its own scroll offset within a 98% logical area (1% peek
   margins left/right): focus/open/close/resize/move keeps the scroll when the
   focused tile fits and otherwise moves only as far as needed. `center-window`
   centers the focused column, `align-window-right` puts its right edge at 99%,
   and `move-next`/`move-previous` reorder whole columns. Soft fullscreen
   occupies 98% width and available height, while true fullscreen delegates to
   River. Borders use `border.color` for the focused tile and
   `border.unfocused_color` otherwise (both with alpha). Keyboard
   `repeat_rate`/`repeat_delay` apply globally, including hotplug. Each monitor
   has dynamic workspaces with up/down navigation and window moves
   (`move-to-output-*`, `move-to-workspace-*`). Empty workspaces are pruned
   above a single trailing empty workspace; output removal preserves occupied
   workspace groups (including scroll) on a surviving output.
   Direction preselection inserts the next regular window
   beside a column or row, with a manager-owned shell surface marking the side.
5. **Make daily operation predictable.** Configuration reload validates first,
   retains the last valid settings on failure, and applies replacements through
   `manage_dirty()`. Existing/hotplug keyboards receive current repeat settings.
   Bindings are disabled while locked, and layer-shell focus returns to the
   selected window. Animation deadlines participate in the existing poll loop;
   IPC integration and richer diagnostics remain future
   work. Gate optional protocol extensions by negotiated versions.

Layer-shell wallpapers, bars, and launchers can map their surfaces. Top/bottom
reservations reduce available height while preserving physical horizontal peek
margins; true fullscreen still covers the complete output. Layout movement and
tile clipping animate with configurable timing; custom titlebars remain future work.
Use compositor-drawn borders first if decoration is needed; custom decoration
surfaces introduce buffer creation and commit synchronization work.

## Validation

Run `cargo fmt --check`, `cargo check`, and `cargo test` for implementation changes.
See [testing](testing.md) for offline commands, protocol coverage, and the
controlled River checklist.
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
- Preselect all four spawn directions, spawn multiple windows, and verify only
  the first regular window consumes the choice. Check repeat-to-cancel, explicit
  cancellation, dialogs, target closure, output removal, and fullscreen previews.
- Click and type through the preview; verify it never receives pointer or
  keyboard focus and disappears with the new tile in a completed render sequence.
- Move the pointer between windows/outputs; verify keyboard and monitor focus
  stay fixed. Click a visible neighboring tile and check explicit focus.
- Reconfigure/remove an output and leave fullscreen; verify restored geometry
  and focus point to surviving objects.
- Fill the bottom workspace and confirm a new empty one appears below. Empty
  top/middle workspaces by closing or moving windows and verify navigation skips
  them. Check remembered focus, stacks, widths, and both fullscreen modes when
  switching workspaces, including an empty workspace's cleared focus.
- Switch workspaces with a pending spawn direction; verify the overlay hides
  on inactive workspaces and the next regular window returns to its target
  workspace. Moving the anchor to another workspace must cancel the selection.
- Switch workspaces independently on two monitors, move a stacked row between
  workspaces, and unplug a monitor with multiple occupied workspaces. Confirm
  the surviving monitor retains distinct workspace groups and a single empty
  workspace at the bottom.
- Stop or restart the manager while applications remain open; distinguish this
  from the explicit command that exits the entire session.
- Check startup against missing/older globals and an unavailable manager; report
  a useful error without issuing unsupported requests.

Protocol logging with `WAYLAND_DEBUG=1` should show ordered finish requests,
with no per-frame roundtrip introduced by our event loop. Interactive checks
must confirm responsiveness as well as correct request ordering.

Keyboard-repeat validation on 2026-10-06 passed formatting, compilation,
30 unit tests, and offline example configuration validation. An isolated nested
River 0.4.8 session confirmed that an application received the configured
73 repeats/second and 210 ms delay, keyboard removal destroyed its proxy, and
idle keyboard reconnection applied the same settings. All manage/render
sequences finished in order. A simulated protocol server additionally checked
input-management versions 1 and 2, negotiation down from a newer advertised
version, multiple keyboards, non-keyboard and unknown device types, zero rate
and delay, removal before configuration, one-time configuration, manager
cleanup, and an actionable error for a missing input-management global.
Physical keyboard hotplug was not exercised.

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

Dynamic-workspace validation on 2026-10-06 passed formatting, compilation,
24 unit tests, and offline example configuration validation. An isolated River
0.4.8 session with two headless monitors passed 15 checks covering workspace
growth/pruning, independent monitor selections, remembered focus and fullscreen,
stacked-row moves, keyboard/click focus, pointer focus invariance, and output
removal/reconnection. Screenshot comparisons confirmed that inactive true
fullscreen windows stay hidden. All 191 manage and 191 render sequences finished
in order without protocol errors. Physical monitors and older negotiated
protocol versions were not exercised.

Workspace/preselection integration on 2026-10-06 passed formatting, compilation,
27 unit tests, and offline validation of all 25 example bindings. Three isolated
two-monitor River sessions passed 45 checks: 15 workspace regressions, 21 spawn
preselection regressions, and 9 checks for their interaction. Pending selections
retain their target workspace, hide previews while it is inactive, return there
on spawning, and cancel when the anchor moves or the workspace disappears.
All 452 manage and 452 render sequences finished in order; preview commits were
synchronized without protocol errors. Physical monitors and older negotiated
protocol versions were not exercised.

Daily-operation validation on 2026-10-07 passed formatting, compilation,
38 unit tests, nine protocol scenarios through the Cargo integration test, and
offline validation of all 33 example bindings. The isolated protocol peer runs
the actual slopwm binary and checks sequence completion, reload failure recovery,
current/hotplug keyboard settings, layer focus and locking, panel reservations,
parent/nested dialogs and render-only resizing, workspace/output moves, object
cleanup, both shutdown acknowledgement orders, missing globals, and negotiated
versions. This environment is headless; no controlled River session or physical
input/output checks were run for these changes. Wallpaper rendering, application
responses, and real launcher/lock behavior remain pending in the live checklist.
