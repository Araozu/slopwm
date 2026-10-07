# slopwm

A Rust window manager for [River](https://isaacfreund.com/software/river/),
starting from Julian Andrews's [tinyrwm Rust example](https://codeberg.org/river/tinyrwm).

Windows tile in a horizontal scrolling strip on each monitor. New windows open
to the left of the focused tile by default; each monitor can override that
direction. Opening, closing, or focusing a window preserves the other windows'
widths and scrolls the strip instead of squeezing it into the screen. Windows
can also stack vertically inside a column using keyboard actions.

The focused tile's left edge always sits 1% of the monitor width from its left
edge, even for a lone window. Tiles fill the monitor height, including their
borders. Soft fullscreen uses 98% of the monitor width and restores the previous
width and vertical stack when toggled off. Adjacent scrolled-away windows peek
through the remaining margins. True fullscreen delegates geometry to River and
covers the whole monitor; leaving it restores the tile. Selecting another tile
on the same monitor also leaves true fullscreen.

The active monitor stays fixed until an explicit monitor shortcut, a window
click, or output removal changes it. Moving the pointer never changes keyboard
or monitor focus. Tiles are clipped to their own monitor, so scrolling cannot
spill onto a neighboring screen. Removing a monitor moves its windows to a
surviving monitor. slopwm uses River's output geometry without changing screen
resolution or output positions.

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

## Configuration

slopwm reads YAML from `$XDG_CONFIG_HOME/slopwm/config.yaml`, falling back to
`~/.config/slopwm/config.yaml` when `XDG_CONFIG_HOME` is unset or invalid.
Start with the [example config](config.example.yaml):

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/slopwm"
cp config.example.yaml "${XDG_CONFIG_HOME:-$HOME/.config}/slopwm/config.yaml"
```

```yaml
border: {width: 2, color: "#ffffff"}
scrolling: {growth_direction: left, default_width_percent: 50}
monitors:
  DP-1: {growth_direction: right}
keybindings:
  "Super+Return": {spawn: [foot]}
  "Super+d": {spawn: [fuzzel]}
  "Super+q": close
  "Super+n": focus-next
  "Super+p": focus-previous
  "Super+Up": focus-up
  "Super+Down": focus-down
  "Super+Shift+Right": stack-next
  "Super+Shift+Left": stack-previous
  "Super+u": unstack
  "Super+m": focus-output-next
  "Super+f": toggle-soft-fullscreen
  "Super+Shift+f": toggle-fullscreen
  "Super+equal": {change-width-percent: 10}
  "Super+minus": {change-width-percent: -10}
  "Super+Escape": exit
```

The `keybindings` map replaces all default keyboard bindings. Omit the map to
keep the defaults, or use `keybindings: {}` to disable keyboard bindings. When
the default file is absent, the built-in shortcuts below are used.

`border.width` is a nonnegative number of logical pixels; `0` disables borders.
Colors accept quoted `"#RRGGBB"` or `"#RRGGBBAA"` values. Border space is included
in tile dimensions. Borders shrink on tiny tiles so content dimensions remain
positive, and River suppresses them in true fullscreen.

`scrolling.default_width_percent` sets new tile widths, from `1` through `98`.
`scrolling.growth_direction` accepts `left` (the default) or `right`. The
`monitors` map overrides growth direction by Wayland output name, such as `DP-1`
or `HDMI-A-1`. Unlisted monitors use the global setting. Output names require
`wl_output` version 4; older outputs use the global setting.

Actions include `close`, `focus-next`, `focus-previous`, `focus-output-next`,
`focus-output-previous`, `toggle-soft-fullscreen`, `toggle-fullscreen`, and
`exit` (which exits the entire Wayland session). Column focus cycles in physical
left-to-right order within the active monitor, wrapping at either end. Monitor
focus cycles by River's output positions and remembers each monitor's selected
tile, including when an empty monitor is selected.

`stack-next` / `stack-previous` move the focused window into the column on its
right / left, adding it below that column's windows. The resulting stack shares
the target column's width and divides its height equally. At the strip's edge,
stacking toward a missing neighbor does nothing. `focus-up` / `focus-down` cycle
within the column; `focus-next` / `focus-previous` switch columns while preserving
the row where possible. `unstack` restores the focused window to its own column
on the configured growth side. New windows always get their own column.

Soft fullscreen temporarily hides sibling rows and gives the selected window
the whole height. Toggling it off restores the stack; focusing a sibling row
also restores it. Stacking, unstacking, and closing a row change only the
affected columns' vertical allocations. Other columns retain their widths.

`{change-width-percent: 10}` adds ten percentage points of monitor width;
`{change-width-percent: -10}` subtracts them. Each binding chooses its own integer
step between `-97` and `97`, excluding zero. Width stays within `1`–`98` percent.
Changing width leaves either fullscreen mode and changes the focused column,
including its stacked rows.
Applications may round or reject proposed dimensions; oversized content is
clipped to its tile. Application maximize requests use soft fullscreen, and
application fullscreen requests use River fullscreen on the window's monitor.

`spawn` takes a list containing the executable and its arguments; for example,
`{spawn: [foot, --title, "My terminal"]}` preserves the title as one argument.
Commands run directly, without shell expansion. For shell syntax, explicitly
use `{spawn: [sh, -c, "your shell command"]}`.

Shortcuts have zero or more modifiers separated by `+`, followed by a
case-sensitive [XKB keysym name](https://xkbcommon.org/doc/current/group__keysyms.html),
such as `q`, `space`, `Return`, `Escape`, `F1`, `plus`, or `XF86AudioMute`.
They follow the active keyboard layout. Modifier names are case-insensitive:
`Shift`, `Ctrl`/`Control`, `Alt`/`Mod1`, `Super`/`Logo`/`Mod4`, `Mod3`, and `Mod5`.
For example, `Ctrl+Alt+Return` combines two modifiers and `F1` binds an
unmodified key.

Validate a file without starting a Wayland connection:

```sh
cargo run -- --config config.example.yaml --check-config
```

Use `--check-config` on its own to validate the default config path. Invalid YAML,
unknown settings/actions/keys/modifiers, duplicate shortcuts, invalid geometry
settings, and empty spawn commands produce startup errors. A file explicitly
selected with `--config` must exist. Configuration is loaded at startup;
restart slopwm to apply changes.

For a custom file, pass the command and its arguments together to River:

```sh
river -c './target/release/slopwm --config /absolute/path/config.yaml'
```

## Default controls

| Input | Action |
| --- | --- |
| Super + Space | Spawn `foot` |
| Super + q | Ask the focused window to close |
| Super + n / Super + p | Focus the next / previous column |
| Super + Up / Super + Down | Focus the row above / below within a column |
| Super + Shift + Right / Left | Stack the focused window into the right / left column |
| Super + u | Unstack the focused window into its own column |
| Super + m / Super + Shift + m | Focus the next / previous monitor |
| Super + f | Toggle soft fullscreen (98% width) |
| Super + Shift + f | Toggle true fullscreen |
| Super + = / Super + - | Add / subtract 10 percentage points of width |
| Super + Escape | Exit the entire Wayland session |
| Click a window | Focus and raise it |

See [development notes](docs/README.md) for architecture, protocol rules, the
reference walkthrough, and planned improvements. [Protocol provenance](protocol/README.md)
records the released XML revision used by this baseline.

## License

Rust implementation: [0BSD](LICENSE), with original tinyrwm attribution retained.
The bundled protocol XML files carry their original MIT license notices.
