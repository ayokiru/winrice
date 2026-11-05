mod cursors;
mod files;
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
    fn plan(&self, sys: &dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>>;
    /// Applies the section and returns what actually changed.
    fn apply(&self, sys: &mut dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>>;
}

pub fn all() -> Vec<Box<dyn Module>> {
    vec![
        Box::new(registry::theme()),
        Box::new(registry::taskbar()),
        Box::new(registry::explorer()),
        Box::new(wallpaper::Wallpaper),
        Box::new(cursors::Cursors),
        Box::new(terminal::Terminal),
    ]
}

pub fn find(name: &str) -> Option<Box<dyn Module>> {
    all().into_iter().find(|m| m.name() == name)
}
