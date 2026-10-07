# Testing slopwm

Build and test prerequisites are Rust, the `libxkbcommon` development files,
and Python 3.10 or later for the protocol integration test. Offline checks need no Wayland
session:

```sh
cargo fmt --check
cargo check
cargo test
cargo run -- --config config.example.yaml --check-config
```

`cargo test` includes the pure geometry/configuration tests and a protocol
integration test that runs the actual binary against an isolated Wayland peer.
The peer uses bundled XML to encode requests/events, verifies request boundaries
and object lifetimes, and always completes manage/render sequences. It checks
reload success/failure, current and hotplugged keyboard repeat, locked bindings,
layer focus restoration, panel reservations, parent/nested dialogs, render-only
resizing, output/workspace moves, negotiated versions, and graceful shutdown with
either manager finishing first. Animation scenarios check intermediate motion,
retargeting, stacking, nested dialogs at viewport edges, preview placement,
confirmed-content clipping, reload/disable behavior, idle wakeups, locking,
fullscreen, output removal, and shutdown during a transition. Existing scenarios
also run with animations explicitly disabled. The peer uses a private socket
and temporary config;
it does not connect to the user's session.

Run only those scenarios with:

```sh
cargo test --test protocol
```

For individual Python scenario output:

```sh
cargo build
python3 tests/protocol.py target/debug/slopwm
```

## Controlled River checklist

Use a disposable River session and a copy of `config.example.yaml` with test
bindings for `reload-config` and `quit`. Keep the normal session's config and
processes separate. Build first, then start River with absolute paths:

```sh
WAYLAND_DEBUG=1 river -c '/absolute/path/slopwm/target/debug/slopwm --config /absolute/path/test-config.yaml'
```

Use the session's Wayland socket for its test applications. Record the River
version, output geometry, configured repeat values, and the protocol log. A
successful run should remain responsive, report no protocol errors, and finish
every manage/render sequence. The peer cannot verify pixels, real application
responses, or compositor input delivery; check those here.

| Scenario | Expected result |
| --- | --- |
| Start `awww` or `swaybg`; change wallpaper; remove/reconnect an output | Wallpaper surfaces map on every live output; application output queries and visible pixels agree |
| Add, resize, and remove top/bottom bar reservations | Tiles, stacks, dialogs, previews, and soft fullscreen avoid reserved height; horizontal 1% peeks and column widths stay tied to the full output; true fullscreen covers the output |
| Open an exclusive launcher, then dismiss it or launch an application | Launcher receives keys while open; the selected application receives keys afterward, including when selection did not change |
| Open an on-demand/nonexclusive launcher, then navigate or click a window | Launcher keeps focus until explicit selection; navigation/click restores application focus; pointer motion alone never changes focus |
| Lock, press shortcuts, and unlock | Applications receive no WM shortcuts while locked; queued actions do not replay; selected-window focus and bindings return after unlocking |
| Open dialogs above a focused and a background parent, then nested/sibling dialogs | Focused-family dialogs receive focus; background families stay in their workspace; dialogs are centered, constrained, and above their parent; clicking a sibling raises its branch |
| Move/stack/resize a parent with its dialog selected; switch workspaces and outputs | Column actions affect the parent's tile; all dialogs follow its workspace/output and retain their geometry; hidden families stay hidden |
| Open a dialog above a fullscreen parent; close nested dialogs, then the parent | Parent stays fullscreen when the dialog is focused; closing a dialog restores a live ancestor; surviving parentless windows become tiles |
| Preselect an insertion direction, then open a dialog and a regular window | Dialog leaves the choice and preview intact; the first regular window consumes it |
| Resize an application that rounds or rejects proposed sizes | Confirmed content is clipped to its allocation; later render-only sequences recenter dialogs without stale geometry |
| Reload valid config by binding and SIGHUP, then try invalid/deleted config | Bindings, borders, growth direction, and repeat update at a completed sequence; widths/workspaces/scroll persist; failed reload retains current settings and reports the error |
| Scroll, center, reorder, stack/unstack, and resize with animations enabled; interrupt transitions rapidly | Motion finishes at the normal layout; focus responds immediately; dialogs and previews follow displayed tiles; confirmed application content remains clipped |
| Reload duration/frame interval during motion, disable animations, then leave the session idle | Timing changes apply without jumps; disabling reaches the final layout; completed transitions cause no frame wakeups |
| Connect another keyboard after reload | Existing and newly connected keyboards receive the new repeat rate/delay; WM shortcuts do not repeat |
| Remove an output carrying stacks, workspaces, and dialogs, then reconnect it | Families recover together on a surviving output; focus has no stale references; workspace groups and scroll remain coherent |
| Invoke `quit`, SIGTERM, and SIGINT in separate runs; attach a replacement WM | Both managers finish and clean up; River and applications remain alive; a replacement WM can attach |
| Invoke `exit` in a separate disposable session | The entire Wayland session ends, distinct from WM-only shutdown |

Also run the scrolling/workspace/preselection regression scenarios in the
[implementation plan](implementation-plan.md#validation).

## Validation record

On 2026-10-07, formatting, compilation, 38 unit tests, nine protocol scenarios,
and offline validation of 33 example bindings passed. The controlled River
checklist above was **not run** in this headless environment. Real wallpaper,
launcher, lock, physical input/output, and visual behavior remain pending.

Animation validation on 2026-10-07 passed formatting, compilation, 46 unit tests,
15 protocol scenarios, and offline example configuration validation. Linking used
a temporary `LIBRARY_PATH` directory pointing to the installed `libxkbcommon.so.0`,
because this environment lacks the development linker file. No controlled River
session or visual animation checks ran in this headless environment.
