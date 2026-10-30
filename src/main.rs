#![cfg_attr(
    all(target_os = "windows", feature = "gui", not(test)),
    windows_subsystem = "windows"
)]

use anyhow::{Context, Result};
use pte_explorer::pte::Program;
use std::path::PathBuf;
#[cfg(feature = "gui")]
mod ui;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // CLI modes may reuse the calling terminal; normal GUI launches never
    // allocate or attach a console.
    #[cfg(all(target_os = "windows", feature = "gui"))]
    if args
        .first()
        .is_some_and(|a| a == "--inspect" || a == "--help")
    {
        unsafe {
            let _ = windows::Win32::System::Console::AttachConsole(
                windows::Win32::System::Console::ATTACH_PARENT_PROCESS,
            );
        }
    }
    if args.first().is_some_and(|a| a == "--inspect") {
        let path = PathBuf::from(
            args.get(1)
                .context("Usage: pte-explorer --inspect model.pte")?,
        );
        let program = Program::open(&path)?;
        println!("{}", serde_json::to_string_pretty(&program)?);
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--help") {
        println!(
            "pte-explorer [model.pte]\npte-explorer --inspect model.pte  (JSON; no display required)"
        );
        return Ok(());
    }
    let path = args.first().map(PathBuf::from);
    #[cfg(feature = "gui")]
    {
        ui::run(path);
        Ok(())
    }
    #[cfg(not(feature = "gui"))]
    {
        let _ = path;
        anyhow::bail!("GUI disabled; use --inspect or rebuild with PTE_GUI=ON")
    }
}
