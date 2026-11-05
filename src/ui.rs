//! Terminal output. anstream strips colors when output is not a terminal or NO_COLOR is set.

use crate::modules::Change;
use anstream::{eprintln, println};
use anstyle::{AnsiColor, Style};
use std::fmt::Display;

const BOLD: Style = Style::new().bold();
const DIM: Style = Style::new().dimmed();
const GREEN: Style = AnsiColor::Green.on_default();
const GREEN_BOLD: Style = AnsiColor::Green.on_default().bold();
const RED: Style = AnsiColor::Red.on_default();
const RED_BOLD: Style = AnsiColor::Red.on_default().bold();
const YELLOW_BOLD: Style = AnsiColor::Yellow.on_default().bold();
const CYAN_BOLD: Style = AnsiColor::Cyan.on_default().bold();
const BLUE: Style = AnsiColor::Blue.on_default();

pub fn warn(msg: impl Display) {
    eprintln!("{YELLOW_BOLD}warning:{YELLOW_BOLD:#} {msg}");
}

pub fn note(msg: impl Display) {
    eprintln!("{BLUE}note:{BLUE:#} {msg}");
}

pub fn step(msg: impl Display) {
    eprintln!("{DIM}{msg}{DIM:#}");
}

pub fn error(err: &anyhow::Error) {
    eprintln!("{RED_BOLD}error:{RED_BOLD:#} {err:#}");
}

pub fn success(msg: impl Display) {
    println!("{GREEN_BOLD}✓{GREEN_BOLD:#} {msg}");
}

pub fn info(msg: impl Display) {
    println!("{msg}");
}

pub fn hint(msg: impl Display) {
    println!("{DIM}{msg}{DIM:#}");
}

/// Setup title line: name, author and where it came from.
pub fn title(name: &str, author: Option<&str>, from: Option<&str>, description: Option<&str>) {
    let mut line = format!("{CYAN_BOLD}{name}{CYAN_BOLD:#}");
    if let Some(a) = author {
        line.push_str(&format!(" {DIM}by{DIM:#} {BOLD}{a}{BOLD:#}"));
    }
    if let Some(f) = from {
        line.push_str(&format!(" {DIM}from {f}{DIM:#}"));
    }
    println!("{line}");
    if let Some(d) = description {
        println!("{DIM}{d}{DIM:#}");
    }
}

pub fn module_list(items: &[(&str, &str)]) {
    let width = items.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    for (name, desc) in items {
        println!("  {GREEN_BOLD}{name:<width$}{GREEN_BOLD:#}  {desc}");
    }
}

/// Prints changes grouped by module, returns how many there were.
pub fn changes(results: &[(String, Vec<Change>)]) -> usize {
    let mut total = 0;
    for (module, changes) in results.iter().filter(|(_, c)| !c.is_empty()) {
        println!();
        println!("{BOLD}{module}{BOLD:#}");
        let width = changes.iter().map(|c| c.key.len()).max().unwrap_or(0);
        for c in changes {
            let from = match &c.from {
                Some(f) => format!("{RED}{f}{RED:#}"),
                None => format!("{DIM}none{DIM:#}"),
            };
            println!(
                "  {:<width$}  {from} {DIM}→{DIM:#} {GREEN}{}{GREEN:#}",
                c.key, c.to
            );
        }
        total += changes.len();
    }
    if total > 0 {
        println!();
    }
    total
}
