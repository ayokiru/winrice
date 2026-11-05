mod engine;
mod modules;
mod setup;
mod sys;

use anyhow::Result;
use clap::{Parser, Subcommand};
use modules::Change;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "winrice",
    version,
    about = "Capture, share and apply your whole Windows look"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Save the current look into a setup folder
    Capture {
        #[arg(default_value = "my-setup")]
        out: PathBuf,
        /// Setup name (defaults to the folder name)
        #[arg(long)]
        name: Option<String>,
        /// Only these modules, comma separated
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
    },
    /// Show what applying a setup would change
    Diff { setup: PathBuf },
    /// Apply a setup (the current state is backed up first)
    Apply {
        setup: PathBuf,
        /// Only show the changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Undo the last apply
    Rollback,
    /// List supported modules
    Modules,
}

fn print_changes(results: &[(String, Vec<Change>)]) -> usize {
    let mut total = 0;
    for (module, changes) in results {
        for c in changes {
            let from = c.from.as_deref().unwrap_or("(unset)");
            println!("  {module}.{}: {from} -> {}", c.key, c.to);
        }
        total += changes.len();
    }
    total
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let data_dir = engine::data_dir();
    match cli.cmd {
        Cmd::Modules => {
            for m in modules::all() {
                println!("{:<10} {}", m.name(), m.description());
            }
        }
        Cmd::Capture { out, name, only } => {
            let sys = sys::open()?;
            let name = name.unwrap_or_else(|| {
                out.file_name()
                    .map_or("my-setup".into(), |n| n.to_string_lossy().into_owned())
            });
            let setup = engine::capture(sys.as_ref(), &out, &name, &only, &data_dir)?;
            let names: Vec<_> = setup.sections.keys().map(String::as_str).collect();
            println!("Captured {} into {}", names.join(", "), out.display());
        }
        Cmd::Diff { setup } => {
            let sys = sys::open()?;
            let dir = setup::resolve(&setup);
            let s = setup::Setup::load(&dir)?;
            let results = engine::plan(sys.as_ref(), &s, &dir, &data_dir)?;
            if print_changes(&results) == 0 {
                println!("Nothing to change.");
            }
        }
        Cmd::Apply { setup, dry_run } => {
            let mut sys = sys::open()?;
            let dir = setup::resolve(&setup);
            let s = setup::Setup::load(&dir)?;
            if dry_run {
                let results = engine::plan(sys.as_ref(), &s, &dir, &data_dir)?;
                println!("Would change {} setting(s).", print_changes(&results));
                return Ok(());
            }
            let results = engine::apply(sys.as_mut(), &s, &dir, &data_dir, true)?;
            let n = print_changes(&results);
            println!(
                "Applied \"{}\": {n} setting(s) changed. Undo with `winrice rollback`.",
                s.meta.name
            );
        }
        Cmd::Rollback => {
            let mut sys = sys::open()?;
            let results = engine::rollback(sys.as_mut(), &data_dir)?;
            println!("Rolled back {} setting(s).", print_changes(&results));
        }
    }
    Ok(())
}
