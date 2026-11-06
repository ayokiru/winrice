//! A setup is a folder with `setup.toml` and an `assets/` directory.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

pub const FILE: &str = "setup.toml";
pub const ASSETS: &str = "assets";
/// A top-level `[[name]]` array is kept as section `name` with this key.
pub const LIST: &str = "list";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Meta {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// winrice version that wrote the setup.
    pub winrice: String,
}

#[derive(Debug, Clone, Default)]
pub struct Setup {
    pub meta: Meta,
    pub sections: BTreeMap<String, Table>,
}

/// Accepts either the setup folder or the `setup.toml` inside it.
pub fn resolve(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().map(Path::to_path_buf).unwrap_or_default()
    }
}

impl Setup {
    pub fn load(dir: &Path) -> Result<Self> {
        let file = dir.join(FILE);
        let text =
            fs::read_to_string(&file).with_context(|| format!("cannot read {}", file.display()))?;
        let mut table: Table =
            toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?;
        let meta = match table.remove("meta") {
            Some(v) => v.try_into().context("invalid [meta] section")?,
            None => Meta::default(),
        };
        let mut sections = BTreeMap::new();
        for (name, value) in table {
            match value {
                Value::Table(t) => {
                    sections.insert(name, t);
                }
                Value::Array(a) if a.iter().all(Value::is_table) => {
                    sections.insert(name, Table::from_iter([(LIST.into(), Value::Array(a))]));
                }
                _ => crate::ui::warn(format_args!(
                    "top-level key \"{name}\" is not a section, skipped"
                )),
            }
        }
        Ok(Self { meta, sections })
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        let mut table = Table::new();
        table.insert("meta".into(), Value::try_from(&self.meta)?);
        for (name, section) in &self.sections {
            let value = match section.get(LIST) {
                Some(list) if section.len() == 1 => list.clone(),
                _ => Value::Table(section.clone()),
            };
            table.insert(name.clone(), value);
        }
        fs::create_dir_all(dir)?;
        fs::write(dir.join(FILE), toml::to_string_pretty(&table)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("winrice-setup-{}", std::process::id()));
        let mut s = Setup::default();
        s.meta.name = "test".into();
        s.meta.winrice = "0.1.0".into();
        s.sections.insert(
            "taskbar".into(),
            toml::from_str("alignment = \"left\"").unwrap(),
        );
        s.save(&dir).unwrap();
        let loaded = Setup::load(&dir).unwrap();
        assert_eq!(loaded.meta.name, "test");
        assert_eq!(
            loaded.sections["taskbar"]["alignment"].as_str(),
            Some("left")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_array_of_tables_sections() {
        let dir = std::env::temp_dir().join(format!("winrice-setup-list-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(FILE),
            "[[configs]]
file = \"a.yaml\"
to = \"b\"
",
        )
        .unwrap();
        let s = Setup::load(&dir).unwrap();
        assert_eq!(
            s.sections["configs"][LIST][0]["file"].as_str(),
            Some("a.yaml")
        );
        s.save(&dir).unwrap();
        assert!(
            fs::read_to_string(dir.join(FILE))
                .unwrap()
                .contains("[[configs]]")
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
