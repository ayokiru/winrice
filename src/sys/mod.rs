//! Thin layer over the OS so modules can be tested without Windows.

use anyhow::Result;
use std::path::{Path, PathBuf};

#[cfg(test)]
pub mod mock;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::WinSystem;

#[derive(Debug, Clone, PartialEq)]
pub enum RegValue {
    Dword(u32),
    Str(String),
    Binary(Vec<u8>),
}

/// Registry paths are relative to HKEY_CURRENT_USER unless stated otherwise.
pub trait System {
    fn reg_get(&self, path: &str, name: &str) -> Result<Option<RegValue>>;
    /// String values of a key; `machine` reads HKEY_LOCAL_MACHINE instead.
    fn reg_list(&self, machine: bool, path: &str) -> Result<Vec<(String, String)>>;
    fn reg_set(&mut self, path: &str, name: &str, value: &RegValue) -> Result<()>;
    /// Removes a value; a missing value is not an error.
    fn reg_delete(&mut self, path: &str, name: &str) -> Result<()>;
    fn wallpaper(&self) -> Result<Option<PathBuf>>;
    fn set_wallpaper(&mut self, path: &Path) -> Result<()>;
    /// Makes Windows reload cursors from the registry.
    fn reload_cursors(&mut self) -> Result<()>;
    /// Loads a font file into the current session.
    fn load_font(&mut self, path: &Path) -> Result<()>;
    /// Tells running apps (Explorer, taskbar) to reload settings.
    fn notify_changed(&mut self) -> Result<()>;
    fn env(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

#[cfg(windows)]
pub fn open() -> Result<Box<dyn System>> {
    Ok(Box::new(WinSystem::new()))
}

#[cfg(not(windows))]
pub fn open() -> Result<Box<dyn System>> {
    anyhow::bail!("winrice can only capture or apply setups on Windows")
}
