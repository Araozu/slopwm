// SPDX-License-Identifier: 0BSD

//! Startup configuration, parsed and validated before connecting to Wayland.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use xkbcommon::xkb;

use crate::action::Action;
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
}

impl Default for Config {
    fn default() -> Self {
        let keybindings = [
            (xkb::keysyms::KEY_space, Action::Spawn(vec!["foot".into()])),
            (xkb::keysyms::KEY_q, Action::Close),
            (xkb::keysyms::KEY_n, Action::FocusNext),
            (xkb::keysyms::KEY_Escape, Action::Exit),
        ]
        .into_iter()
        .map(|(keysym, action)| KeyBinding {
            keysym,
            modifiers: Modifiers::Mod4,
            action,
        })
        .collect();
        Self { keybindings }
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
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BindingAction {
    Named(String),
    Spawn(SpawnCommand),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpawnCommand {
    spawn: Vec<String>,
}

impl Config {
    pub(crate) fn load(explicit_path: Option<&Path>) -> Result<Self, ConfigError> {
        match explicit_path {
            Some(path) => Self::load_path(path, true),
            None => match default_path(
                std::env::var_os("XDG_CONFIG_HOME"),
                std::env::var_os("HOME"),
            ) {
                Some(path) => Self::load_path(&path, false),
                None => Ok(Self::default()),
            },
        }
    }

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
        let Some(bindings) = file.keybindings else {
            return Ok(Self::default());
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
        Ok(Self { keybindings })
    }
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
    Some(config_home.join("slopwm/config.yaml"))
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
            "exit" => Ok(Action::Exit),
            _ => Err(format!(
                "unknown action {name:?}; expected close, focus-next, exit, or a spawn argument list"
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
            Some("/tmp/config/slopwm/config.yaml".into())
        );
        for xdg in [None, Some("".into()), Some("relative".into())] {
            assert_eq!(
                default_path(xdg, home.clone()),
                Some("/home/test/.config/slopwm/config.yaml".into())
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
