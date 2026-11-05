mod engine;
mod modules;
mod setup;
mod source;
mod sys;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use modules::Change;
use source::Source;
use std::io::Write;
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
        /// Author shown to people who apply the setup
        #[arg(long)]
        author: Option<String>,
        /// Short description of the setup
        #[arg(long)]
        description: Option<String>,
        /// Only these modules, comma separated
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
    },
    /// Show what applying a setup would change
    Diff {
        /// Setup folder or GitHub repo (owner/repo[/path][@ref])
        setup: String,
    },
    /// Apply a setup (the current state is backed up first)
    Apply {
        /// Setup folder or GitHub repo (owner/repo[/path][@ref])
        setup: String,
        /// Only show the changes
        #[arg(long)]
        dry_run: bool,
        /// Do not ask for confirmation
        #[arg(long, short)]
        yes: bool,
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

fn confirm(question: &str) -> Result<bool> {
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

fn describe(s: &setup::Setup, source: &Source) {
    let mut line = format!("\"{}\"", s.meta.name);
    if let Some(a) = &s.meta.author {
        line.push_str(&format!(" by {a}"));
    }
    if let Source::Remote(_, r) = source {
        line.push_str(&format!(" from {}", r.label()));
    }
    println!("{line}");
    if let Some(d) = &s.meta.description {
        println!("  {d}");
    }
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
        Cmd::Capture {
            out,
            name,
            author,
            description,
            only,
        } => {
            let sys = sys::open()?;
            let name = name.unwrap_or_else(|| {
                out.file_name()
                    .map_or("my-setup".into(), |n| n.to_string_lossy().into_owned())
            });
            let mut setup = engine::capture(sys.as_ref(), &out, &name, &only, &data_dir)?;
            if author.is_some() || description.is_some() {
                setup.meta.author = author;
                setup.meta.description = description;
                setup.save(&out)?;
            }
            let names: Vec<_> = setup.sections.keys().map(String::as_str).collect();
            println!("Captured {} into {}", names.join(", "), out.display());
        }
        Cmd::Diff { setup } => {
            let sys = sys::open()?;
            let source = source::resolve(&setup, &data_dir)?;
            let dir = source.dir();
            let s = setup::Setup::load(dir)?;
            describe(&s, &source);
            let results = engine::plan(sys.as_ref(), &s, dir, &data_dir)?;
            if print_changes(&results) == 0 {
                println!("Nothing to change.");
            }
        }
        Cmd::Apply {
            setup,
            dry_run,
            yes,
        } => {
            let mut sys = sys::open()?;
            let source = source::resolve(&setup, &data_dir)?;
            let dir = source.dir();
            let s = setup::Setup::load(dir)?;
            describe(&s, &source);
            let results = engine::plan(sys.as_ref(), &s, dir, &data_dir)?;
            let n = print_changes(&results);
            if dry_run {
                println!("Would change {n} setting(s).");
                return Ok(());
            }
            if n == 0 {
                println!("Nothing to change.");
                return Ok(());
            }
            if !yes && !confirm(&format!("Apply {n} change(s)?"))? {
                bail!("cancelled");
            }
            let results = engine::apply(sys.as_mut(), &s, dir, &data_dir, true)?;
            let n: usize = results.iter().map(|(_, c)| c.len()).sum();
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
