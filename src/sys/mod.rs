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
}

/// All registry paths are relative to HKEY_CURRENT_USER.
pub trait System {
    fn reg_get(&self, path: &str, name: &str) -> Result<Option<RegValue>>;
    fn reg_set(&mut self, path: &str, name: &str, value: &RegValue) -> Result<()>;
    fn wallpaper(&self) -> Result<Option<PathBuf>>;
    fn set_wallpaper(&mut self, path: &Path) -> Result<()>;
    /// Tells running apps (Explorer, taskbar) to reload settings.
    fn notify_changed(&mut self) -> Result<()>;
}

#[cfg(windows)]
pub fn open() -> Result<Box<dyn System>> {
    Ok(Box::new(WinSystem::new()))
}

#[cfg(not(windows))]
pub fn open() -> Result<Box<dyn System>> {
    anyhow::bail!("winrice can only capture or apply setups on Windows")
}
