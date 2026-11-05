use super::{RegValue, System};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct MockSystem {
    pub reg: HashMap<(String, String), RegValue>,
    pub wallpaper: Option<PathBuf>,
    pub notified: usize,
}

impl MockSystem {
    pub fn with(mut self, path: &str, name: &str, value: RegValue) -> Self {
        self.reg.insert((path.into(), name.into()), value);
        self
    }
}

impl System for MockSystem {
    fn reg_get(&self, path: &str, name: &str) -> Result<Option<RegValue>> {
        Ok(self.reg.get(&(path.into(), name.into())).cloned())
    }

    fn reg_set(&mut self, path: &str, name: &str, value: &RegValue) -> Result<()> {
        self.reg.insert((path.into(), name.into()), value.clone());
        Ok(())
    }

    fn wallpaper(&self) -> Result<Option<PathBuf>> {
        Ok(self.wallpaper.clone())
    }

    fn set_wallpaper(&mut self, path: &Path) -> Result<()> {
        self.wallpaper = Some(path.to_path_buf());
        Ok(())
    }

    fn notify_changed(&mut self) -> Result<()> {
        self.notified += 1;
        Ok(())
    }
}
