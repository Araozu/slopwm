# slopwm

A Rust window manager for [River](https://isaacfreund.com/software/river/),
starting from Julian Andrews's [tinyrwm Rust example](https://codeberg.org/river/tinyrwm).
The baseline uses a floating layout with keyboard focus and pointer move/resize.

## Build and run

```sh
cargo build --release
river -c ./target/release/slopwm
```

Requires River exposing window-management interface version 4 or newer and
XKB bindings version 1 or newer. The client negotiates up to the versions in
its bundled XML. `foot` must be available for the terminal binding.

For protocol logging after a debug build:

```sh
WAYLAND_DEBUG=1 river -c ./target/debug/slopwm
```

## Controls

| Input | Action |
| --- | --- |
| Super + Space | Spawn `foot` |
| Super + q | Ask the focused window to close |
| Super + n | Cycle keyboard focus and raise the selected window |
| Super + Escape | Exit the entire Wayland session |
| Super + left mouse button | Move the hovered window |
| Super + right mouse button | Resize the hovered window |
| Click a window | Focus and raise it |

See [development notes](docs/README.md) for architecture, protocol rules, the
reference walkthrough, and planned improvements. [Protocol provenance](protocol/README.md)
records the released XML revision used by this baseline.

## License

Rust implementation: [0BSD](LICENSE), with original tinyrwm attribution retained.
The bundled protocol XML files carry their original MIT license notices.
