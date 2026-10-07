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
its bundled XML. Build/run requires `libxkbcommon` (including its development
files at build time). `foot` must be available for the default terminal binding.

For protocol logging after a debug build:

```sh
WAYLAND_DEBUG=1 river -c ./target/debug/slopwm
```

## Configure keybindings

slopwm reads YAML from `$XDG_CONFIG_HOME/slopwm/config.yaml`, falling back to
`~/.config/slopwm/config.yaml` when `XDG_CONFIG_HOME` is unset or invalid.
Start with the [example config](config.example.yaml):

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/slopwm"
cp config.example.yaml "${XDG_CONFIG_HOME:-$HOME/.config}/slopwm/config.yaml"
```

```yaml
keybindings:
  "Super+Return": {spawn: [foot]}
  "Super+d": {spawn: [fuzzel]}
  "Super+q": close
  "Super+n": focus-next
  "Super+Escape": exit
```

The `keybindings` map replaces all default keyboard bindings. Omit the map to
keep the defaults, or use `keybindings: {}` to disable keyboard bindings. When
the default file is absent, the built-in shortcuts below are used.

Built-in actions are `close`, `focus-next`, and `exit` (which exits the entire
Wayland session). `spawn` takes a list containing the executable and its
arguments; for example, `{spawn: [foot, --title, "My terminal"]}` preserves the
title as one argument. Commands run directly, without shell expansion. For shell
syntax, explicitly use `{spawn: [sh, -c, "your shell command"]}`.

Shortcuts have zero or more modifiers separated by `+`, followed by a
case-sensitive [XKB keysym name](https://xkbcommon.org/doc/current/group__keysyms.html),
such as `q`, `space`, `Return`, `Escape`, `F1`, `plus`, or `XF86AudioMute`.
They follow the active keyboard layout. Modifier names are case-insensitive:
`Shift`, `Ctrl`/`Control`, `Alt`/`Mod1`, `Super`/`Logo`/`Mod4`, `Mod3`, and `Mod5`.
For example, `Ctrl+Alt+Return` combines two modifiers and `F1` binds an
unmodified key. Pointer controls still use the defaults below.

Validate a file without starting a Wayland connection:

```sh
cargo run -- --config config.example.yaml --check-config
```

Use `--check-config` on its own to validate the default config path. Invalid YAML,
unknown actions/keys/modifiers, duplicate shortcuts, and empty spawn commands
produce startup errors. A file explicitly selected with `--config` must exist.
Configuration is loaded at startup; restart slopwm to apply changes.

For a custom file, pass the command and its arguments together to River:

```sh
river -c './target/release/slopwm --config /absolute/path/config.yaml'
```

## Default controls

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
