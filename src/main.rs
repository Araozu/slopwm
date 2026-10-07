// SPDX-FileCopyrightText: © 2026 Julian Andrews
// SPDX-License-Identifier: 0BSD

//! Application entry point.

mod action;
mod app;
mod config;
mod protocol;
mod wm;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "Usage: slopwm [--config PATH] [--check-config]

  -c, --config PATH  Load this YAML file instead of the default config
      --check-config Validate configuration and exit without connecting to Wayland
  -h, --help        Show this help

Default: $XDG_CONFIG_HOME/slopwm/config.yml or $HOME/.config/slopwm/config.yml.
If the default file is absent, built-in keyboard shortcuts are used.";

#[derive(Debug, Default)]
struct Options {
    config: Option<PathBuf>,
    check_config: bool,
    help: bool,
}

impl Options {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(argument) = args.next() {
            match argument.to_str() {
                Some("-c" | "--config") => {
                    if options.config.is_some() {
                        return Err("--config may only be specified once".into());
                    }
                    let path = args.next().ok_or("--config requires a file path")?;
                    if path.is_empty() || path.to_string_lossy().starts_with('-') {
                        return Err("--config requires a file path (use ./ for a filename starting with '-')".into());
                    }
                    options.config = Some(path.into());
                }
                Some("--check-config") => options.check_config = true,
                Some("-h" | "--help") => options.help = true,
                _ => {
                    return Err(format!(
                        "unknown argument {argument:?}; use --help for usage"
                    ));
                }
            }
        }
        Ok(options)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("slopwm: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::parse(std::env::args_os().skip(1))?;
    if options.help {
        println!("{HELP}");
        return Ok(());
    }
    let source = config::ConfigSource::new(options.config.as_deref());
    let config = source.load()?;
    if options.check_config {
        println!(
            "Configuration valid ({} keybindings)",
            config.keybindings.len()
        );
        return Ok(());
    }
    app::run(config, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn cli_parses_config_and_offline_validation_in_either_order() {
        for args in [
            ["--config", "my config.yaml", "--check-config"],
            ["--check-config", "-c", "my config.yaml"],
        ] {
            let options = parse(&args).unwrap();
            assert_eq!(options.config, Some("my config.yaml".into()));
            assert!(options.check_config);
        }
        assert!(parse(&["--help"]).unwrap().help);
        assert!(parse(&[]).unwrap().config.is_none());
    }

    #[test]
    fn cli_rejects_unknown_options_and_missing_or_duplicate_config_paths() {
        for args in [
            vec!["--unknown"],
            vec!["--config"],
            vec!["--config", "--check-config"],
            vec!["--config", ""],
            vec!["--config", "one.yaml", "-c", "two.yaml"],
        ] {
            assert!(parse(&args).is_err(), "accepted {args:?}");
        }
    }
}
