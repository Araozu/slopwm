# slopwm

A Rust window manager for [River](https://isaacfreund.com/software/river/),
starting from Julian Andrews's [tinyrwm Rust example](https://codeberg.org/river/tinyrwm).

Windows tile in a horizontal scrolling strip on each monitor. New windows open
to the left of the focused tile by default; each monitor can override that
direction. Opening, closing, or focusing a window preserves the other windows'
widths and scrolls the strip instead of squeezing it into the screen. Windows
can also stack vertically inside a column using keyboard actions.

Each monitor has its own dynamic list of workspaces, arranged from top to bottom.
Each workspace keeps its scrolling strip, widths, stacks, and selected window.
There is always one empty workspace at the bottom; using it creates a new empty
one below. Empty workspaces above it disappear automatically. Switching
workspaces affects only the active monitor and stops at the top or bottom.

Each workspace remembers its own scroll offset. The usable logical area is
98% of the monitor width, with a 1% peek margin on the left and right that
keeps scrolled-away neighbors visible. Within that area slopwm behaves like a
regular scrolling WM: if the focused tile already fits without moving, the
strip stays where it is; otherwise it moves only as far as needed to bring the
focused tile fully into view. A lone window starts at the 1% left inset.
Tiles fill the available monitor height, including their borders, avoiding
top/bottom panels that reserve space. Soft fullscreen uses
98% of the monitor width and the available height, and restores the previous
width and vertical stack when toggled off. `center-window` centers the focused
column, and `align-window-right` puts its right edge at the 99% mark. True
fullscreen delegates geometry to River and covers the whole monitor; leaving it restores
the tile. Selecting another tile in the same workspace also leaves true
fullscreen.

The active monitor stays fixed until an explicit monitor shortcut, a window
click, or output removal changes it. Moving the pointer never changes keyboard
or monitor focus. Tiles are clipped to their own monitor, so scrolling cannot
spill onto a neighboring screen. Removing a monitor moves its occupied
workspaces to a surviving monitor, above that monitor's empty workspace.
slopwm uses River's output geometry without changing screen resolution or output
positions.

## Build and run

```sh
cargo build --release
river -c ./target/release/slopwm
```

Requires River exposing window-management interface version 4 or newer and
XKB bindings and input management version 1 or newer. The client negotiates up
to the versions in its bundled XML. Build/run requires `libxkbcommon` (including
its development files at build time). `foot` must be available for the default
terminal binding.

For protocol logging after a debug build:

```sh
WAYLAND_DEBUG=1 river -c ./target/debug/slopwm
```

## Wallpaper and layer-shell clients

slopwm binds River's layer-shell interface, so wallpaper daemons (`awww`,
`swaybg`), bars, and launchers can map their surfaces. Without that binding
River closes layer surfaces immediately, which surfaces in `awww` as
`none of the requested outputs are valid` because the daemon ends up with no
outputs at all.

Start the daemon inside the River session so it connects to the same
`WAYLAND_DISPLAY`, then use the output names the compositor actually reports:

```sh
awww-daemon
awww query
awww img -o DP-1 ~/Pictures/wallpaper.jpeg
```

If `DP-1` is rejected, `awww query` (or `wlr-randr`) shows the valid names;
nested or headless sessions often use names like `WL-1` instead. The wallpaper
is visible in the tile margins and peeks,
or fully on an empty workspace. Top/bottom panels that reserve exclusive zones
reduce the vertical tiling area, including stacks, soft fullscreen, and spawn
previews. Removing a panel restores the height. Horizontal column widths and
the 1% peek margins always use the full monitor width; side-panel exclusive
zones are ignored. True fullscreen covers the entire monitor, including panels.

Layer-shell launchers can request exclusive or non-exclusive keyboard focus.
Exclusive focus lasts until the launcher releases it. Non-exclusive focus is
kept until explicit window/monitor/workspace navigation, a window click, or a
new selected window changes focus. Closing the launcher restores the selected
window. Pointer motion never changes focus.

## Dialogs

Transient windows with a live parent open above its tile, centered and clipped
to the available area. The application chooses its initial content size;
oversized dialogs are constrained without resizing or reordering the parent's
column. Nested dialogs follow the same rule. A dialog receives focus on opening
when its parent's family is selected on the active workspace and monitor;
background dialogs do not switch workspaces or monitors or steal focus.

Dialogs follow their parent when its row/column moves, stacks, changes workspace
or monitor, or migrates after output removal. Column navigation, width changes,
and move/stack actions while a dialog is selected act on its parent tile. Close
and fullscreen actions act on the selected dialog. Dialogs do not consume spawn
preselection. Closing the focused dialog restores focus to its surviving parent;
a dialog whose parent disappears becomes an ordinary tile.

## Configuration

slopwm reads YAML from `$XDG_CONFIG_HOME/slopwm/config.yml`, falling back to
`~/.config/slopwm/config.yml` when `XDG_CONFIG_HOME` is unset or invalid.
The [example config](config.example.yaml) documents every supported setting and
keybinding action, with defaults and commented customization examples. Copy it
to get started:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/slopwm"
cp config.example.yaml "${XDG_CONFIG_HOME:-$HOME/.config}/slopwm/config.yml"
```

```yaml
keyboard: {repeat_rate: 40, repeat_delay: 400}
border: {width: 2, color: "#ffffff", unfocused_color: "#808080ff"}
scrolling: {growth_direction: left, default_width_percent: 50}
monitors:
  DP-1: {growth_direction: right}
keybindings:
  "Super+space": {spawn: [foot]}
  "Super+Return": {spawn: [foot]}
  "Super+d": {spawn: [fuzzel]}
  "Super+q": close
  "Super+Right": focus-next
  "Super+Left": focus-previous
  "Super+Up": focus-up
  "Super+Down": focus-down
  "Super+Ctrl+Shift+Right": move-next
  "Super+Ctrl+Shift+Left": move-previous
  "Super+Shift+Right": stack-next
  "Super+Shift+Left": stack-previous
  "Super+u": unstack
  "Super+c": center-window
  "Super+Shift+c": align-window-right
  "Super+Alt+Right": focus-output-next
  "Super+Alt+Left": focus-output-previous
  "Super+Alt+Shift+Right": move-to-output-next
  "Super+Alt+Shift+Left": move-to-output-previous
  "Super+Alt+Up": focus-workspace-up
  "Super+Alt+Down": focus-workspace-down
  "Super+Alt+Shift+Up": move-to-workspace-up
  "Super+Alt+Shift+Down": move-to-workspace-down
  "Super+f": toggle-soft-fullscreen
  "Super+Shift+f": toggle-fullscreen
  "Super+equal": {change-width-percent: 10}
  "Super+minus": {change-width-percent: -10}
  "Super+Ctrl+Left": preselect-left
  "Super+Ctrl+Right": preselect-right
  "Super+Ctrl+Up": preselect-up
  "Super+Ctrl+Down": preselect-down
  "Super+Ctrl+Escape": preselect-cancel
  "Super+Shift+r": reload-config
  "Super+Shift+Escape": exit
```

The `keybindings` map replaces all default keyboard bindings. Omit the map to
keep the defaults, or use `keybindings: {}` to disable keyboard bindings. When
the default file is absent, the built-in shortcuts below are used.

`keyboard.repeat_rate` sets key repeats per second (default `40`), and
`keyboard.repeat_delay` sets the delay before repeating in milliseconds (default
`400`). Both accept nonnegative integers; a rate of `0` disables repeat. One
setting applies to all keyboards on every seat, including keyboards connected
after startup. Omitted fields use their defaults. These settings control repeat
in applications; window-manager shortcuts trigger once per press.

`border.width` is a nonnegative number of logical pixels; `0` disables borders.
`border.color` (focused) and `border.unfocused_color` both accept quoted
`"#RRGGBB"` or `"#RRGGBBAA"` values, where `AA` is alpha (`00` transparent,
`ff` opaque). Only the active monitor's selected window uses the focused
color; every other visible tile uses the unfocused color. Border space is
included in tile dimensions. Borders shrink on tiny tiles so content dimensions
remain positive, and River suppresses them in true fullscreen.

`scrolling.default_width_percent` sets new tile widths, from `1` through `98`.
`scrolling.growth_direction` accepts `left` (the default) or `right`. The
`monitors` map overrides growth direction by Wayland output name, such as `DP-1`
or `HDMI-A-1`. Unlisted monitors use the global setting. Output names require
`wl_output` version 4; older outputs use the global setting.

Actions include `close`, `focus-next`, `focus-previous`, `move-next`,
`move-previous`, `center-window`, `align-window-right`, `focus-output-next`,
`focus-output-previous`, `toggle-soft-fullscreen`, `toggle-fullscreen`, and
`exit` (which exits the entire Wayland session). Column focus moves in physical
left-to-right order within the active workspace. Monitor focus follows River's
output positions and remembers each monitor's selected tile, including when an
empty monitor is selected. All seats share one keyboard focus and active
monitor by design. All focus navigation stops at the ends; windows,
stacked rows, monitors, and workspaces never wrap around.

`move-next` / `move-previous` move the focused column (with all its stacked
rows) one step right / left within the active workspace, stopping at either
end. `center-window` scrolls the strip so the focused column is centered, and
`align-window-right` scrolls it so the column's right edge sits at the 99%
mark; both preserve the 1% peek margins.

`move-to-output-next` / `move-to-output-previous` move the focused window to the
next / previous monitor and follow it, stopping at either end. The window gets
its own column in that monitor's selected workspace, on its configured growth
side. Remaining stacked rows stay together on the source monitor. The move
leaves fullscreen and keeps the regular width percentage, scaled to the target
monitor's width. Empty workspaces are pruned and a new trailing empty workspace
is created as needed on both monitors.

`focus-workspace-up` / `focus-workspace-down` select the workspace above / below
on the active monitor. `move-to-workspace-up` / `move-to-workspace-down` move the
focused window there and follow it. Moving a stacked row gives it its own column
and leaves the remaining rows together. Moving a window leaves fullscreen and
keeps its regular width. If the current workspace becomes empty, it is removed
and the next workspace below is selected. The bottom empty workspace remains
available and clears keyboard focus. Switching back restores the selected
window and fullscreen state; other monitors keep their selected workspaces.

`stack-next` / `stack-previous` move the focused window into the column on its
right / left, adding it below that column's windows. The resulting stack shares
the target column's width and divides its height equally. At the strip's edge,
stacking toward a missing neighbor does nothing. `focus-up` / `focus-down` move
within the column; `focus-next` / `focus-previous` switch columns while preserving
the row where possible. `unstack` restores the focused window to its own column
on the configured growth side. New windows get their own column unless a
vertical spawn direction is preselected.

`preselect-left` / `preselect-right` choose which side of the selected column
will receive the next window, overriding the monitor's growth direction once.
`preselect-up` / `preselect-down` insert the next window immediately above /
below the selected row in the same column. Vertical insertion keeps the
column's regular width and divides its height equally between the rows, leaving
soft or true fullscreen on the target column as needed.

A translucent blue overlay with an arrow marks the selected side of the tile.
It indicates insertion direction; the existing scrolling and stack rules still
determine the final window dimensions. The overlay accepts no input and never
changes focus. On an empty workspace it marks the initial tile, and any direction
opens the first column. It is clipped to its monitor, including when the chosen
tile scrolls partly or entirely out of view.

The selection stays attached to the chosen tile, workspace, and monitor when
focus moves. Its overlay is hidden while that workspace is inactive. Spawning
with a pending selection returns to its workspace and monitor.
The next new window without a parent consumes it and receives focus, whether
launched by a spawn binding or another program; dialogs do not consume it.
Press the same direction on the same tile again, or use `preselect-cancel`, to
clear it. Closing the target tile, moving it to another workspace or monitor,
removing its workspace or monitor, or locking the session also clears it. Without a
selection, normal monitor growth direction applies.

Soft fullscreen is per column: it temporarily hides sibling rows and gives
the selected window the available height. Toggling it off or focusing a sibling
row in the same column restores that column's stack. Other columns keep their
own soft-fullscreen state, so several columns can be soft at once.
Stacking, unstacking, and closing a row change only the
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
unknown settings/actions/keys/modifiers, duplicate shortcuts, invalid repeat or geometry
settings, and empty spawn commands produce startup errors. A file explicitly
selected with `--config` must exist.

Configuration is loaded at startup. Press **Super + Shift + r**, use a
`reload-config` binding, or send **SIGHUP** to reload the same configuration file.
The entire file is validated before any changes are applied. Invalid or missing
files leave the last working configuration and bindings in place and print an
error to stderr. Successful reloads update bindings on every seat, borders,
keyboard repeat on current and future keyboards, and settings for future window
insertion. Existing widths, stacks, workspace selections, and scroll offsets
are preserved; normal minimal scrolling still applies if geometry requires it.
If your replacement binding map omits `reload-config`, SIGHUP remains available.

The `quit` action, **SIGTERM**, and **SIGINT** stop only slopwm, completing
in-flight protocol sequences and waiting for River's shutdown acknowledgements
before destroying objects. River and applications remain alive so another
window manager can attach. `quit` has no default shortcut; for example bind
`"Super+Ctrl+Shift+Escape": quit`. The existing `exit` action still exits the
entire Wayland session.

For a custom file, pass the command and its arguments together to River:

```sh
river -c './target/release/slopwm --config /absolute/path/config.yml'
```

## Default controls

| Input | Action |
| --- | --- |
| Super + Space | Spawn `foot` |
| Super + Return (Enter) | Spawn `foot` |
| Super + q | Ask the focused window to close |
| Super + Right / Left | Focus the column to the right / left |
| Super + Up / Super + Down | Focus the row above / below within a column |
| Super + Ctrl + Shift + Right / Left | Move the focused column (with its stack) right / left |
| Super + Shift + Right / Left | Stack the focused window into the right / left column |
| Super + u | Unstack the focused window into its own column |
| Super + c | Center the focused column |
| Super + Shift + c | Align the focused column to the right edge (99% mark) |
| Super + Alt + Right / Left | Focus the next / previous monitor |
| Super + Alt + Shift + Right / Left | Move the focused window to the next / previous monitor and follow it |
| Super + Alt + Up / Down | Focus the workspace above / below on this monitor |
| Super + Alt + Shift + Up / Down | Move the focused window to the workspace above / below and follow it |
| Super + f | Toggle soft fullscreen (98% width) |
| Super + Shift + f | Toggle true fullscreen |
| Super + = / Super + - | Add / subtract 10 percentage points of width |
| Super + Ctrl + Left / Right / Up / Down | Preselect the next window's insertion direction |
| Super + Ctrl + Escape | Cancel spawn preselection |
| Super + Shift + r | Reload configuration; keep current settings if validation fails |
| Super + Shift + Escape | Exit the entire Wayland session |
| Click a window | Focus and raise it |

All focus shortcuts stop at the edges. Super + arrows selects windows;
Super + Alt + arrows selects monitors horizontally and workspaces vertically.
Adding Shift to Super + Alt + arrows moves the focused window and follows it.

See [development notes](docs/README.md) for architecture, protocol rules, the
reference walkthrough, and planned improvements. [Protocol provenance](protocol/README.md)
records the released XML revision used by this baseline.

## License

Rust implementation: [0BSD](LICENSE), with original tinyrwm attribution retained.
The bundled protocol XML files carry their original MIT license notices.
