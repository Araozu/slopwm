# Rust demo walkthrough

Reference: `/home/fernando/projects/river/tinyrwm/rust`. This is a small floating
window manager, used as slopwm's imported baseline. This walkthrough describes
the original local reference; slopwm's adaptations are recorded below.

## Imported baseline

[slopwm's manager](../src/wm/mod.rs) retains the reference's floating behavior and
attribution, with startup in [app.rs](../src/app.rs) and state/event handlers
split by responsibility under `src/wm/`. See the
[module map](implementation-plan.md#module-boundaries) for the current structure.
Its [generated bindings](../src/protocol.rs) use the released River v0.4.8 XML
(management version 5 and XKB version 3), and registry binding negotiates the
server's version up to those maxima while retaining minimum requirements 4/1.
The new window/output capture-session events are accepted and ignored.
The package is named `slopwm`, and [Cargo.lock](../Cargo.lock) records dependency
resolutions. See the [project README](../README.md) for current build/run commands.

## Build inputs

The [demo manifest](/home/fernando/projects/river/tinyrwm/rust/Cargo.toml) declares
Rust edition 2024 and these dependency requirements:

| Crate | Manifest requirement | Role |
| --- | --- | --- |
| `wayland-client` | `0.31.13` | Connection, proxies, event queue, dispatch |
| `wayland-scanner` | `0.31.9` | Generate bindings from protocol XML |
| `wayland-backend` | `0.3.14` | Object identifiers and support for generated bindings |
| `bitflags` | `2.11.0` | Generated protocol bitfields |

These are the demo's semver requirements, not exact locked resolutions or a
claim about the latest releases. slopwm uses these same requirements and commits
a lockfile. No compositor library is involved in this example.

The [protocol module](/home/fernando/projects/river/tinyrwm/rust/src/main.rs:21)
uses `generate_interfaces!` and `generate_client_code!` for both XML files.
The XKB interface generation imports the management interfaces because its
bindings refer to management seats and modifiers. Preserve that dependency
ordering when moving generated bindings into their own module.

The Rust demo is attributed to Julian Andrews and uses 0BSD. The protocol XML
files are attributed to Isaac Freund and use MIT. Preserve their notices if
copying code or vendoring the XML.

## Connection and dispatch

[main](/home/fernando/projects/river/tinyrwm/rust/src/main.rs:773) connects with
`Connection::connect_to_env()`, creates an event queue, and requests the registry.
One startup roundtrip discovers globals and binds them; the steady-state loop
uses `blocking_dispatch()`.

The [registry handler](/home/fernando/projects/river/tinyrwm/rust/src/main.rs:528)
requires management version 4 and XKB version 1. Missing globals produce startup
errors. Connecting successfully to an arbitrary Wayland compositor is insufficient:
the connection must expose River's management globals to this client.

Implement `Dispatch` for each object type that sends events. The manager's
`event_created_child!` declaration associates compositor-created window, output,
and seat objects with their dispatch implementations. Binding objects carry
their seat's `ObjectId` as user data, so their callbacks can find the owning seat.
No-event objects use `delegate_noop!`.

## State and sequence handlers

| Code entry point | What to study |
| --- | --- |
| [State types](/home/fernando/projects/river/tinyrwm/rust/src/main.rs:73) | `AppData` holds globals and policy state; windows use a `VecDeque`, outputs/seats use maps keyed by `ObjectId` |
| [Manage handler](/home/fernando/projects/river/tinyrwm/rust/src/main.rs:137) | Cleanup, initialize windows and bindings, process requests and seat actions, then `manage_finish()` |
| [Render handler](/home/fernando/projects/river/tinyrwm/rust/src/main.rs:153) | Position moving windows; anchor top/left resizes using confirmed dimensions; then `render_finish()` |
| [Action processing](/home/fernando/projects/river/tinyrwm/rust/src/main.rs:403) | Spawn a terminal, close, cycle focus, begin pointer operations, or exit the session |
| [Window events](/home/fernando/projects/river/tinyrwm/rust/src/main.rs:625) | Record dimensions, closure, and application move/resize requests |
| [Seat events](/home/fernando/projects/river/tinyrwm/rust/src/main.rs:698) | Record hover, interaction, operation deltas, and release |

New windows start at `(0, 0)` with `propose_dimensions(0, 0)`. The back of the
window deque is the top window. Interaction moves a window to the back;
`focus_top()` sets keyboard focus and raises its node. Focus cycling rotates
the deque. Pointer operations retain the starting geometry separately from
the current dimensions.

The binding callbacks store `pending_action`; they do not directly issue focus
or resize requests. The next manage handler performs the action. Spawning `foot`
removes `WAYLAND_DEBUG` from the child's environment to keep protocol logging
focused on the manager.

## Demo controls

| Input | Behavior |
| --- | --- |
| Super + Space | Spawn `foot` |
| Super + q | Ask the focused window to close |
| Super + n | Cycle focus and stacking |
| Super + Escape | Exit the entire Wayland session |
| Super + left mouse button | Move the hovered window |
| Super + right mouse button | Resize from bottom/right |
| Click a window | Focus and raise it |

The demo [README](/home/fernando/projects/river/tinyrwm/rust/README.md) documents
these commands, run from its Rust directory:

```sh
cargo build --release
river -c ./target/release/tinyrwm
```

For slopwm debugging, run `cargo build` in this repository and then
`WAYLAND_DEBUG=1 river -c ./target/debug/slopwm`. The `-c` launch syntax is also
documented in River v0.4.8's manual.

## Gaps to address in slopwm

- The demo records output objects but ignores their positions and dimensions.
  Add output geometry before tiling or monitor-aware placement.
- Title, app ID, parent relationships, dimension hints, decoration hints, and
  fullscreen/maximize/minimize requests are ignored. Choose behavior for each
  and advertise only implemented capabilities.
- All seats share one global stacking deque; independent seat focus policy needs
  an explicit design.
- `remove_windows()` drops closed windows without explicitly destroying their
  window/node objects. Add full cleanup and clear all seat references to removed
  windows, beyond canceling active operations.
- `finished` immediately exits, session lock events are ignored, and many lookups
  use `expect()`. Add orderly shutdown and tolerant handling of obsolete state.
- Keep policy callbacks quick. Future configuration loading or expensive work
  should not block completion of a manage sequence.
