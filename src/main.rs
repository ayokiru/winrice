mod engine;
mod modules;
mod setup;
mod source;
mod sys;
mod ui;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use source::Source;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

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

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

fn confirm(question: &str) -> Result<bool> {
    anstream::print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

fn describe(s: &setup::Setup, source: &Source) {
    let from = match source {
        Source::Remote(_, r) => Some(r.label()),
        Source::Local(_) => None,
    };
    ui::title(
        &s.meta.name,
        s.meta.author.as_deref(),
        from.as_deref(),
        s.meta.description.as_deref(),
    );
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            ui::error(&e);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let data_dir = engine::data_dir();
    match cli.cmd {
        Cmd::Modules => {
            let all = modules::all();
            let items: Vec<_> = all.iter().map(|m| (m.name(), m.description())).collect();
            ui::module_list(&items);
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
            ui::success(format_args!(
                "Captured {} into {}",
                names.join(", "),
                out.display()
            ));
            ui::hint(format_args!(
                "Share it: push the folder to GitHub, then anyone can run `winrice apply <you>/<repo>`"
            ));
        }
        Cmd::Diff { setup } => {
            let sys = sys::open()?;
            let source = source::resolve(&setup, &data_dir)?;
            let dir = source.dir();
            let s = setup::Setup::load(dir)?;
            describe(&s, &source);
            let results = engine::plan(sys.as_ref(), &s, dir, &data_dir)?;
            match ui::changes(&results) {
                0 => ui::success("Already applied, nothing to change."),
                n => ui::info(format_args!("{} would be made.", plural(n, "change"))),
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
            let n = ui::changes(&results);
            if n == 0 {
                ui::success("Already applied, nothing to change.");
                return Ok(());
            }
            if dry_run {
                ui::info(format_args!("{} would be made.", plural(n, "change")));
                return Ok(());
            }
            if !yes && !confirm(&format!("Apply {}?", plural(n, "change")))? {
                bail!("cancelled");
            }
            let results = engine::apply(sys.as_mut(), &s, dir, &data_dir, true)?;
            let n: usize = results.iter().map(|(_, c)| c.len()).sum();
            ui::success(format_args!(
                "Applied {} ({})",
                s.meta.name,
                plural(n, "change")
            ));
            ui::hint("Undo anytime with `winrice rollback`");
        }
        Cmd::Rollback => {
            let mut sys = sys::open()?;
            let results = engine::rollback(sys.as_mut(), &data_dir)?;
            let n = ui::changes(&results);
            ui::success(format_args!("Rolled back {}", plural(n, "change")));
        }
    }
    Ok(())
}
