// SPDX-License-Identifier: 0BSD

//! Configuration sources, YAML parsing, and validation.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use xkbcommon::xkb;

use crate::action::{Action, SpawnDirection};
use crate::protocol::river_seat_v1::Modifiers;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyBinding {
    pub(crate) keysym: u32,
    pub(crate) modifiers: Modifiers,
    pub(crate) action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) keybindings: Vec<KeyBinding>,
    pub(crate) keyboard: KeyboardConfig,
    pub(crate) border: BorderConfig,
    pub(crate) scrolling: ScrollingConfig,
    pub(crate) monitors: BTreeMap<String, MonitorConfig>,
}

/// Resolve the source once so reloads use the same file as startup.
#[derive(Debug, Default)]
pub(crate) struct ConfigSource {
    path: Option<PathBuf>,
    required: bool,
}

impl ConfigSource {
    pub(crate) fn new(explicit_path: Option<&Path>) -> Self {
        Self {
            path: explicit_path.map(Path::to_path_buf).or_else(|| {
                default_path(
                    std::env::var_os("XDG_CONFIG_HOME"),
                    std::env::var_os("HOME"),
                )
            }),
            required: explicit_path.is_some(),
        }
    }

    pub(crate) fn load(&self) -> Result<Config, ConfigError> {
        match &self.path {
            Some(path) => Config::load_path(path, self.required),
            None => Ok(Config::default()),
        }
    }

    pub(crate) fn reload(&self) -> Result<Config, ConfigError> {
        match &self.path {
            Some(path) => Config::load_path(path, true),
            None => Ok(Config::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct KeyboardConfig {
    pub(crate) repeat_rate: i32,
    pub(crate) repeat_delay: i32,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            repeat_rate: 40,
            repeat_delay: 400,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum GrowthDirection {
    #[default]
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct MonitorConfig {
    pub(crate) growth_direction: GrowthDirection,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ScrollingConfig {
    pub(crate) growth_direction: GrowthDirection,
    pub(crate) default_width_percent: u8,
}

impl Default for ScrollingConfig {
    fn default() -> Self {
        Self {
            growth_direction: GrowthDirection::Left,
            default_width_percent: 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BorderConfig {
    pub(crate) width: i32,
    pub(crate) color: [u32; 4],
    pub(crate) unfocused_color: [u32; 4],
}

fn default_border_width() -> i32 {
    2
}

fn default_border_color() -> String {
    "#ffffff".into()
}

fn default_unfocused_color() -> String {
    "#808080ff".into()
}

fn default_unfocused_pixels() -> [u32; 4] {
    // Opaque medium gray, premultiplied to match `parse_color("#808080ff")`.
    [0x80808080, 0x80808080, 0x80808080, u32::MAX]
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            width: 2,
            color: [u32::MAX; 4],
            unfocused_color: default_unfocused_pixels(),
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileBorder {
    #[serde(default = "default_border_width")]
    width: i32,
    #[serde(default = "default_border_color")]
    color: String,
    #[serde(default = "default_unfocused_color")]
    unfocused_color: String,
}

impl Default for FileBorder {
    fn default() -> Self {
        Self {
            width: default_border_width(),
            color: default_border_color(),
            unfocused_color: default_unfocused_color(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let keybindings = [
            ("Super+space", Action::Spawn(vec!["foot".into()])),
            ("Super+Return", Action::Spawn(vec!["foot".into()])),
            ("Super+q", Action::Close),
            ("Super+Right", Action::FocusNext),
            ("Super+Left", Action::FocusPrevious),
            ("Super+Up", Action::FocusUp),
            ("Super+Down", Action::FocusDown),
            ("Super+Ctrl+Shift+Right", Action::MoveNext),
            ("Super+Ctrl+Shift+Left", Action::MovePrevious),
            ("Super+Shift+Right", Action::StackNext),
            ("Super+Shift+Left", Action::StackPrevious),
            ("Super+u", Action::Unstack),
            ("Super+c", Action::CenterWindow),
            ("Super+Shift+c", Action::AlignWindowRight),
            ("Super+Alt+Right", Action::FocusOutputNext),
            ("Super+Alt+Left", Action::FocusOutputPrevious),
            ("Super+Alt+Shift+Right", Action::MoveToOutputNext),
            ("Super+Alt+Shift+Left", Action::MoveToOutputPrevious),
            ("Super+Alt+Up", Action::FocusWorkspaceUp),
            ("Super+Alt+Down", Action::FocusWorkspaceDown),
            ("Super+Alt+Shift+Up", Action::MoveToWorkspaceUp),
            ("Super+Alt+Shift+Down", Action::MoveToWorkspaceDown),
            ("Super+f", Action::ToggleSoftFullscreen),
            ("Super+Shift+f", Action::ToggleFullscreen),
            ("Super+equal", Action::ChangeWidthPercent(10)),
            ("Super+minus", Action::ChangeWidthPercent(-10)),
            ("Super+Ctrl+Left", Action::Preselect(SpawnDirection::Left)),
            ("Super+Ctrl+Right", Action::Preselect(SpawnDirection::Right)),
            ("Super+Ctrl+Up", Action::Preselect(SpawnDirection::Up)),
            ("Super+Ctrl+Down", Action::Preselect(SpawnDirection::Down)),
            ("Super+Ctrl+Escape", Action::CancelPreselection),
            ("Super+Shift+r", Action::ReloadConfig),
            ("Super+Shift+Escape", Action::Exit),
        ]
        .into_iter()
        .map(|(chord, action)| {
            let (keysym, modifiers) = parse_chord(chord).expect("valid default shortcut");
            KeyBinding {
                keysym,
                modifiers,
                action,
            }
        })
        .collect();
        Self {
            keybindings,
            keyboard: KeyboardConfig::default(),
            border: BorderConfig::default(),
            scrolling: ScrollingConfig::default(),
            monitors: BTreeMap::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    keybindings: Option<BTreeMap<String, BindingAction>>,
    #[serde(default)]
    keyboard: KeyboardConfig,
    #[serde(default)]
    border: FileBorder,
    #[serde(default)]
    scrolling: ScrollingConfig,
    #[serde(default)]
    monitors: BTreeMap<String, MonitorConfig>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BindingAction {
    Named(String),
    Spawn(SpawnCommand),
    ChangeWidth(WidthChange),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpawnCommand {
    spawn: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WidthChange {
    #[serde(rename = "change-width-percent")]
    percent: i16,
}

impl Config {
    fn load_path(path: &Path, required: bool) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(source) => Self::parse(&source)
                .map_err(|error| ConfigError(format!("{}: {error}", path.display()))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => {
                Ok(Self::default())
            }
            Err(error) => Err(ConfigError(format!(
                "cannot read {}: {error}",
                path.display()
            ))),
        }
    }

    fn parse(source: &str) -> Result<Self, ConfigError> {
        let file: FileConfig = serde_saphyr::from_str(source)
            .map_err(|error| ConfigError(format!("invalid YAML configuration: {error}")))?;
        if file.keyboard.repeat_rate < 0 {
            return Err(ConfigError(
                "keyboard.repeat_rate must be nonnegative".into(),
            ));
        }
        if file.keyboard.repeat_delay < 0 {
            return Err(ConfigError(
                "keyboard.repeat_delay must be nonnegative".into(),
            ));
        }
        if !(1..=98).contains(&file.scrolling.default_width_percent) {
            return Err(ConfigError(
                "scrolling.default_width_percent must be between 1 and 98".into(),
            ));
        }
        if file.border.width < 0 {
            return Err(ConfigError("border.width must be nonnegative".into()));
        }
        if file.monitors.keys().any(|name| name.trim().is_empty()) {
            return Err(ConfigError("monitor names must not be empty".into()));
        }
        let mut config = Self {
            keyboard: file.keyboard,
            border: BorderConfig {
                width: file.border.width,
                color: parse_color(&file.border.color)
                    .map_err(|error| ConfigError(format!("border.color: {error}")))?,
                unfocused_color: parse_color(&file.border.unfocused_color)
                    .map_err(|error| ConfigError(format!("border.unfocused_color: {error}")))?,
            },
            scrolling: file.scrolling,
            monitors: file.monitors,
            ..Self::default()
        };
        let Some(bindings) = file.keybindings else {
            return Ok(config);
        };

        let mut seen = HashMap::new();
        let mut keybindings = Vec::with_capacity(bindings.len());
        for (chord, configured_action) in bindings {
            let (keysym, modifiers) = parse_chord(&chord)
                .map_err(|error| ConfigError(format!("keybinding {chord:?}: {error}")))?;
            if let Some(previous) = seen.insert((keysym, modifiers.bits()), chord.clone()) {
                return Err(ConfigError(format!(
                    "keybindings {previous:?} and {chord:?} refer to the same shortcut"
                )));
            }
            let action = parse_action(configured_action)
                .map_err(|error| ConfigError(format!("keybinding {chord:?}: {error}")))?;
            keybindings.push(KeyBinding {
                keysym,
                modifiers,
                action,
            });
        }
        config.keybindings = keybindings;
        Ok(config)
    }

    pub(crate) fn growth_direction(&self, output_name: Option<&str>) -> GrowthDirection {
        output_name
            .and_then(|name| self.monitors.get(name))
            .map_or(self.scrolling.growth_direction, |monitor| {
                monitor.growth_direction
            })
    }
}

fn parse_color(color: &str) -> Result<[u32; 4], ConfigError> {
    let hex = color.strip_prefix('#').unwrap_or_default();
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ConfigError("must be '#RRGGBB' or '#RRGGBBAA'".into()));
    }
    let mut channels = [255_u32; 4];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        channels[index] = u32::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
    }
    let alpha = channels[3];
    for channel in &mut channels[..3] {
        *channel =
            ((u64::from(*channel) * u64::from(alpha) * u64::from(u32::MAX)) / (255 * 255)) as u32;
    }
    channels[3] = alpha * 0x01010101;
    Ok(channels)
}

fn default_path(xdg_config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    // XDG paths must be absolute; ignore empty or relative environment values.
    let config_home = xdg_config_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|path| path.join(".config"))
        })?;
    Some(config_home.join("slopwm/config.yml"))
}

fn parse_chord(chord: &str) -> Result<(u32, Modifiers), String> {
    let mut parts: Vec<_> = chord.split('+').map(str::trim).collect();
    let key = parts.pop().unwrap_or_default();
    if key.is_empty() || parts.iter().any(|part| part.is_empty()) {
        return Err("expected modifiers followed by a key, e.g. Super+Return".into());
    }
    let mut modifiers = Modifiers::empty();
    for name in parts {
        let modifier = match name.to_ascii_lowercase().as_str() {
            "shift" => Modifiers::Shift,
            "ctrl" | "control" => Modifiers::Ctrl,
            "alt" | "mod1" => Modifiers::Mod1,
            "mod3" => Modifiers::Mod3,
            "super" | "logo" | "mod4" => Modifiers::Mod4,
            "mod5" => Modifiers::Mod5,
            _ => return Err(format!("unknown modifier {name:?}")),
        };
        if modifiers.contains(modifier) {
            return Err(format!("modifier {name:?} is specified more than once"));
        }
        modifiers |= modifier;
    }

    // Preserve XKB's distinction between symbols such as 'a' and 'A'.
    // Reject NUL first: xkbcommon's string wrapper requires a valid CString.
    if key.contains('\0') {
        return Err("key names must not contain NUL characters".into());
    }
    let keysym = xkb::keysym_from_name(key, xkb::KEYSYM_NO_FLAGS).raw();
    if keysym == xkb::keysyms::KEY_NoSymbol {
        return Err(format!(
            "unknown XKB key {key:?}; use names such as space, Return, Escape, F1, or XF86AudioMute"
        ));
    }
    Ok((keysym, modifiers))
}

fn parse_action(action: BindingAction) -> Result<Action, String> {
    match action {
        BindingAction::Named(name) => match name.as_str() {
            "close" => Ok(Action::Close),
            "focus-next" => Ok(Action::FocusNext),
            "focus-previous" => Ok(Action::FocusPrevious),
            "focus-up" => Ok(Action::FocusUp),
            "focus-down" => Ok(Action::FocusDown),
            "move-next" => Ok(Action::MoveNext),
            "move-previous" => Ok(Action::MovePrevious),
            "stack-next" => Ok(Action::StackNext),
            "stack-previous" => Ok(Action::StackPrevious),
            "unstack" => Ok(Action::Unstack),
            "center-window" => Ok(Action::CenterWindow),
            "align-window-right" => Ok(Action::AlignWindowRight),
            "focus-output-next" => Ok(Action::FocusOutputNext),
            "focus-output-previous" => Ok(Action::FocusOutputPrevious),
            "move-to-output-next" => Ok(Action::MoveToOutputNext),
            "move-to-output-previous" => Ok(Action::MoveToOutputPrevious),
            "focus-workspace-up" => Ok(Action::FocusWorkspaceUp),
            "focus-workspace-down" => Ok(Action::FocusWorkspaceDown),
            "move-to-workspace-up" => Ok(Action::MoveToWorkspaceUp),
            "move-to-workspace-down" => Ok(Action::MoveToWorkspaceDown),
            "toggle-soft-fullscreen" => Ok(Action::ToggleSoftFullscreen),
            "toggle-fullscreen" => Ok(Action::ToggleFullscreen),
            "preselect-left" => Ok(Action::Preselect(SpawnDirection::Left)),
            "preselect-right" => Ok(Action::Preselect(SpawnDirection::Right)),
            "preselect-up" => Ok(Action::Preselect(SpawnDirection::Up)),
            "preselect-down" => Ok(Action::Preselect(SpawnDirection::Down)),
            "preselect-cancel" => Ok(Action::CancelPreselection),
            "reload-config" => Ok(Action::ReloadConfig),
            "quit" => Ok(Action::Quit),
            "exit" => Ok(Action::Exit),
            _ => Err(format!(
                "unknown action {name:?}; see README.md for supported actions"
            )),
        },
        BindingAction::Spawn(command) => {
            if command
                .spawn
                .first()
                .is_none_or(|program| program.trim().is_empty())
            {
                return Err("spawn requires a nonempty executable as its first argument".into());
            }
            if command.spawn.iter().any(|argument| argument.contains('\0')) {
                return Err("spawn arguments must not contain NUL characters".into());
            }
            Ok(Action::Spawn(command.spawn))
        }
        BindingAction::ChangeWidth(change) => {
            if change.percent == 0 || !(-97..=97).contains(&change.percent) {
                return Err(
                    "change-width-percent must be between -97 and 97, excluding zero".into(),
                );
            }
            Ok(Action::ChangeWidthPercent(change.percent))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_preserves_default_shortcuts() {
        let example = Config::parse(include_str!("../config.example.yaml")).unwrap();
        let defaults = Config::default();
        assert_eq!(example.keybindings.len(), defaults.keybindings.len());
        for binding in defaults.keybindings {
            assert!(example.keybindings.contains(&binding));
        }
    }

    #[test]
    fn configured_bindings_replace_defaults_and_preserve_arguments() {
        let config = Config::parse(
            "keybindings:\n  Ctrl+Alt+Return:\n    spawn: [foot, --title, 'a title with spaces', '']\n  F1: close\n",
        )
        .unwrap();
        assert_eq!(config.keybindings.len(), 2);
        assert!(config.keybindings.contains(&KeyBinding {
            keysym: xkb::keysyms::KEY_Return,
            modifiers: Modifiers::Ctrl | Modifiers::Mod1,
            action: Action::Spawn(vec![
                "foot".into(),
                "--title".into(),
                "a title with spaces".into(),
                "".into(),
            ]),
        }));
        assert!(config.keybindings.contains(&KeyBinding {
            keysym: xkb::keysyms::KEY_F1,
            modifiers: Modifiers::empty(),
            action: Action::Close,
        }));
    }

    #[test]
    fn absent_bindings_use_defaults_but_empty_map_disables_them() {
        assert_eq!(Config::parse("{}").unwrap(), Config::default());
        assert!(
            Config::parse("keybindings: {}")
                .unwrap()
                .keybindings
                .is_empty()
        );
    }

    #[test]
    fn keyboard_repeat_settings_use_defaults_and_allow_independent_overrides() {
        let defaults = Config::default();
        assert_eq!(
            Config::parse("keyboard: {}").unwrap().keyboard,
            defaults.keyboard
        );
        let config = Config::parse("keyboard: {repeat_rate: 60}").unwrap();
        assert_eq!(config.keyboard.repeat_rate, 60);
        assert_eq!(config.keyboard.repeat_delay, defaults.keyboard.repeat_delay);
        assert_eq!(config.keybindings, defaults.keybindings);

        let config = Config::parse("keyboard: {repeat_delay: 250}").unwrap();
        assert_eq!(config.keyboard.repeat_rate, defaults.keyboard.repeat_rate);
        assert_eq!(config.keyboard.repeat_delay, 250);
        assert_eq!(
            Config::parse(include_str!("../config.example.yaml"))
                .unwrap()
                .keyboard,
            defaults.keyboard
        );
    }

    #[test]
    fn keyboard_repeat_accepts_zero_and_protocol_integer_limits() {
        for (rate, delay) in [(0, 400), (40, 0), (0, 0), (i32::MAX, i32::MAX)] {
            let config = Config::parse(&format!(
                "keyboard: {{repeat_rate: {rate}, repeat_delay: {delay}}}"
            ))
            .unwrap();
            assert_eq!(config.keyboard.repeat_rate, rate);
            assert_eq!(config.keyboard.repeat_delay, delay);
        }
    }

    #[test]
    fn keyboard_repeat_rejects_invalid_values_and_unknown_settings() {
        for source in [
            "keyboard: {repeat_rate: -1}",
            "keyboard: {repeat_delay: -1}",
            "keyboard: {repeat_rate: 2147483648}",
            "keyboard: {repeat_delay: 2147483648}",
            "keyboard: {repeat_rate: 1.5}",
            "keyboard: {repeat_delay: 1.5}",
            "keyboard: {repeat_rate: fast}",
            "keyboard: {repeat_delay: null}",
            "keyboard: {typo: 1}",
        ] {
            assert!(Config::parse(source).is_err(), "accepted {source:?}");
        }
    }

    #[test]
    fn layout_settings_preserve_default_bindings_and_monitor_overrides() {
        let config = Config::parse("border: {width: 4, color: '#ff000080'}\nscrolling: {default_width_percent: 40}\nmonitors:\n  DP-1: {growth_direction: right}\n").unwrap();
        assert_eq!(config.keybindings, Config::default().keybindings);
        assert_eq!(config.border.width, 4);
        assert_eq!(config.border.color, [0x80808080, 0, 0, 0x80808080]);
        // Unspecified unfocused color falls back to its default, and partial
        // border maps keep working.
        assert_eq!(
            config.border.unfocused_color,
            Config::default().border.unfocused_color
        );
        assert_eq!(
            Config::parse("border: {width: 4}").unwrap().border.color,
            Config::default().border.color
        );
        assert_eq!(
            Config::parse("border: {width: 4}")
                .unwrap()
                .border
                .unfocused_color,
            Config::default().border.unfocused_color
        );
        let both =
            Config::parse("border: {width: 2, color: '#ff000080', unfocused_color: '#00ff0080'}")
                .unwrap();
        assert_eq!(both.border.color, [0x80808080, 0, 0, 0x80808080]);
        assert_eq!(both.border.unfocused_color, [0, 0x80808080, 0, 0x80808080]);
        assert_eq!(config.scrolling.default_width_percent, 40);
        assert_eq!(
            config.growth_direction(Some("DP-1")),
            GrowthDirection::Right
        );
        assert_eq!(
            config.growth_direction(Some("HDMI-A-1")),
            GrowthDirection::Left
        );
        assert_eq!(config.growth_direction(None), GrowthDirection::Left);
        assert_eq!(
            parse_color("#123456").unwrap(),
            [0x12121212, 0x34343434, 0x56565656, u32::MAX]
        );
    }

    #[test]
    fn layout_and_width_actions_reject_invalid_configurations() {
        for source in [
            "border: {width: -1}",
            "border: {color: '#12345'}",
            "border: {color: '#gggggg'}",
            "border: {unfocused_color: '#12345'}",
            "border: {unfocused_color: '#gggggg'}",
            "border: {unfocused_color: '#ffffff', typo: 1}",
            "border: {color: '#ffffff', typo: 1}",
            "scrolling: {default_width_percent: 0}",
            "scrolling: {default_width_percent: 99}",
            "scrolling: {growth_direction: up}",
            "monitors: {'': {growth_direction: left}}",
            "monitors: {DP-1: {growth_direction: left, typo: 1}}",
            "keybindings: {F1: {change-width-percent: 0}}",
            "keybindings: {F1: {change-width-percent: 98}}",
            "keybindings: {F1: {change-width-percent: -98}}",
            "keybindings: {F1: {change-width-percent: 1.5}}",
            "keybindings: {F1: {change-width-percent: 10, typo: 1}}",
        ] {
            assert!(Config::parse(source).is_err(), "accepted {source:?}");
        }
        let config = Config::parse("keybindings: {F1: {change-width-percent: -7}}").unwrap();
        assert_eq!(config.keybindings[0].action, Action::ChangeWidthPercent(-7));
        for (name, expected) in [
            ("move-next", Action::MoveNext),
            ("move-previous", Action::MovePrevious),
            ("center-window", Action::CenterWindow),
            ("align-window-right", Action::AlignWindowRight),
        ] {
            let config = Config::parse(&format!("keybindings: {{F1: {name}}}")).unwrap();
            assert_eq!(config.keybindings[0].action, expected, "{name}");
        }
    }

    #[test]
    fn chords_accept_modifier_aliases_and_full_xkb_names() {
        assert_eq!(
            parse_chord(" control + ALT + Return ").unwrap(),
            (xkb::keysyms::KEY_Return, Modifiers::Ctrl | Modifiers::Mod1)
        );
        assert_eq!(
            parse_chord("Logo+XF86AudioMute").unwrap(),
            (xkb::keysyms::KEY_XF86AudioMute, Modifiers::Mod4)
        );
        assert_eq!(
            parse_chord("Super+plus").unwrap(),
            (xkb::keysyms::KEY_plus, Modifiers::Mod4)
        );
        assert_ne!(
            parse_chord("Super+a").unwrap(),
            parse_chord("Super+A").unwrap()
        );
    }

    #[test]
    fn invalid_chords_and_conflicting_aliases_are_rejected() {
        for chord in [
            "",
            "+q",
            "Super+",
            "Super++q",
            "Hyper+q",
            "Super+Mod4+q",
            "Super+NotAKey",
            "Super+NoSymbol",
            "Super+q\0",
        ] {
            assert!(parse_chord(chord).is_err(), "accepted {chord:?}");
        }
        let error = Config::parse("keybindings:\n  Super+Ctrl+q: close\n  Control+Mod4+q: exit\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("same shortcut"), "{error}");
    }

    #[test]
    fn invalid_yaml_actions_and_unknown_fields_are_rejected() {
        for source in [
            "keybindings: [",
            "keybindngs: {}",
            "keybindings:\n  Super+q: clsoe",
            "keybindings:\n  Super+q: {spawn: []}",
            "keybindings:\n  Super+q: {spawn: ['   ']}",
            "keybindings:\n  Super+q: {spawn: [foot], typo: true}",
            "keybindings:\n  Super+q: {spawn: foot}",
            "keybindings:\n  Super+q: {spawn: [foot, \"a\\0b\"]}",
            "keybindings:\n  Super+q: close\n  Super+q: exit\n",
        ] {
            assert!(Config::parse(source).is_err(), "accepted {source:?}");
        }
    }

    #[test]
    fn config_path_honors_xdg_and_home_fallback() {
        let home = Some(OsString::from("/home/test"));
        assert_eq!(
            default_path(Some("/tmp/config".into()), home.clone()),
            Some("/tmp/config/slopwm/config.yml".into())
        );
        for xdg in [None, Some("".into()), Some("relative".into())] {
            assert_eq!(
                default_path(xdg, home.clone()),
                Some("/home/test/.config/slopwm/config.yml".into())
            );
        }
        assert_eq!(default_path(None, None), None);
    }

    #[test]
    fn only_missing_implicit_configs_fall_back_to_defaults() {
        let missing = std::env::temp_dir()
            .join(format!("slopwm-missing-config-{}", std::process::id()))
            .join("config.yaml");
        assert_eq!(
            Config::load_path(&missing, false).unwrap(),
            Config::default()
        );
        let error = Config::load_path(&missing, true).unwrap_err().to_string();
        assert!(error.contains(&missing.display().to_string()), "{error}");
        // A directory is an I/O error, not a missing configuration.
        assert!(Config::load_path(&std::env::temp_dir(), false).is_err());
    }
}
