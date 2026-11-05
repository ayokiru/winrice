use super::{RegValue, System};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct MockSystem {
    pub reg: HashMap<(String, String), RegValue>,
    pub vars: HashMap<String, String>,
    pub wallpaper: Option<PathBuf>,
    pub notified: usize,
    pub cursor_reloads: usize,
    pub fonts: Vec<PathBuf>,
}

impl MockSystem {
    pub fn with(mut self, path: &str, name: &str, value: RegValue) -> Self {
        self.reg.insert((path.into(), name.into()), value);
        self
    }

    pub fn env(mut self, name: &str, value: impl Into<String>) -> Self {
        self.vars.insert(name.into(), value.into());
        self
    }
}

impl System for MockSystem {
    fn reg_get(&self, path: &str, name: &str) -> Result<Option<RegValue>> {
        Ok(self.reg.get(&(path.into(), name.into())).cloned())
    }

    /// HKLM entries are stored with an `HKLM\\` path prefix.
    fn reg_list(&self, machine: bool, path: &str) -> Result<Vec<(String, String)>> {
        let path = if machine {
            format!("HKLM\\{path}")
        } else {
            path.to_string()
        };
        let mut out: Vec<_> = self
            .reg
            .iter()
            .filter(|((p, _), _)| *p == path)
            .filter_map(|((_, n), v)| match v {
                RegValue::Str(s) => Some((n.clone(), s.clone())),
                RegValue::Dword(_) => None,
            })
            .collect();
        out.sort();
        Ok(out)
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

    fn reload_cursors(&mut self) -> Result<()> {
        self.cursor_reloads += 1;
        Ok(())
    }

    fn load_font(&mut self, path: &Path) -> Result<()> {
        self.fonts.push(path.to_path_buf());
        Ok(())
    }

    fn notify_changed(&mut self) -> Result<()> {
        self.notified += 1;
        Ok(())
    }

    fn env(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }
}
