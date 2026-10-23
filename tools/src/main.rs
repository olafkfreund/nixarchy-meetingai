//! Repository tools, deliberately independent of the GUI/native audio packages.
mod bump;
#[cfg(target_os = "linux")]
mod hyprland;
mod release;

use anyhow::{Result, bail};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}

fn main() -> Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, arguments)) = arguments.split_first() else {
        return help();
    };
    match command.as_str() {
        "release" => release::run(&root(), arguments),
        "bump" => bump::run(&root(), arguments),
        #[cfg(target_os = "linux")]
        "hyprland-smoke" => hyprland::smoke(&root(), arguments),
        #[cfg(target_os = "linux")]
        "hyprland-overview" => hyprland::overview(&root(), arguments),
        "--help" | "-h" | "help" => help(),
        _ => bail!("Unknown command: {command}. Run cargo xtask --help."),
    }
}

fn help() -> Result<()> {
    println!(
        "Lennette repository tools\n\n  cargo xtask release\n  cargo xtask bump [patch|minor|major|auto] [message or message-file]\n  cargo xtask hyprland-smoke --help\n  cargo xtask hyprland-overview --help"
    );
    Ok(())
}
