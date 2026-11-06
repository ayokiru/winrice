mod accent;
mod apps;
mod configs;
mod cursors;
mod files;
mod fonts;
mod registry;
mod terminal;
mod wallpaper;

use crate::sys::System;
use anyhow::Result;
use std::path::PathBuf;
use toml::Table;

pub struct Ctx {
    /// Folder with the setup's files (wallpapers, cursors, ...).
    pub assets: PathBuf,
    /// Persistent winrice folder (applied assets, backups).
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub key: String,
    pub from: Option<String>,
    pub to: String,
}

pub trait Module {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn capture(&self, sys: &dyn System, ctx: &Ctx) -> Result<Table>;
    /// What to save before applying `section` so rollback can restore it.
    fn backup(&self, sys: &dyn System, _section: &Table, ctx: &Ctx) -> Result<Table> {
        self.capture(sys, ctx)
    }
    fn plan(&self, sys: &dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>>;
    /// Applies the section and returns what actually changed.
    fn apply(&self, sys: &mut dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>>;
}

/// Backup key listing values that did not exist before an apply; rollback deletes them.
pub const UNSET: &str = "unset";

pub fn unset_keys(section: &Table) -> Result<Vec<&str>> {
    let Some(list) = section.get(UNSET) else {
        return Ok(Vec::new());
    };
    list.as_array()
        .and_then(|l| l.iter().map(|v| v.as_str()).collect())
        .ok_or_else(|| anyhow::anyhow!("{UNSET} must be a list of key names"))
}

pub fn all() -> Vec<Box<dyn Module>> {
    vec![
        Box::new(registry::theme()),
        Box::new(accent::Accent),
        Box::new(registry::taskbar()),
        Box::new(registry::explorer()),
        Box::new(wallpaper::Wallpaper),
        Box::new(cursors::Cursors),
        Box::new(apps::Apps),
        Box::new(configs::Configs),
        Box::new(fonts::Fonts),
        Box::new(terminal::Terminal),
    ]
}

pub fn find(name: &str) -> Option<Box<dyn Module>> {
    all().into_iter().find(|m| m.name() == name)
}
