//! Modules that are plain HKCU registry values.

use super::{Change, Ctx, Module};
use crate::sys::{RegValue, System};
use anyhow::{Result, bail};
use toml::{Table, Value};

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const DWM: &str = r"Software\Microsoft\Windows\DWM";
const ADVANCED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
const SEARCH: &str = r"Software\Microsoft\Windows\CurrentVersion\Search";

pub enum Kind {
    /// Boolean stored as two DWORD values.
    Flag { on: u32, off: u32 },
    /// Named DWORD values.
    Choice(&'static [(&'static str, u32)]),
}

const BOOL: Kind = Kind::Flag { on: 1, off: 0 };
const INV_BOOL: Kind = Kind::Flag { on: 0, off: 1 };

pub struct Setting {
    pub key: &'static str,
    pub path: &'static str,
    pub name: &'static str,
    pub kind: Kind,
}

pub struct RegModule {
    name: &'static str,
    description: &'static str,
    settings: &'static [Setting],
}

pub fn theme() -> RegModule {
    RegModule {
        name: "theme",
        description: "Dark/light mode, transparency, accent color usage",
        settings: &[
            Setting {
                key: "dark_apps",
                path: PERSONALIZE,
                name: "AppsUseLightTheme",
                kind: INV_BOOL,
            },
            Setting {
                key: "dark_system",
                path: PERSONALIZE,
                name: "SystemUsesLightTheme",
                kind: INV_BOOL,
            },
            Setting {
                key: "transparency",
                path: PERSONALIZE,
                name: "EnableTransparency",
                kind: BOOL,
            },
            Setting {
                key: "accent_on_taskbar",
                path: PERSONALIZE,
                name: "ColorPrevalence",
                kind: BOOL,
            },
            Setting {
                key: "accent_on_titlebars",
                path: DWM,
                name: "ColorPrevalence",
                kind: BOOL,
            },
        ],
    }
}

pub fn taskbar() -> RegModule {
    RegModule {
        name: "taskbar",
        description: "Taskbar alignment, search and task view buttons",
        settings: &[
            Setting {
                key: "alignment",
                path: ADVANCED,
                name: "TaskbarAl",
                kind: Kind::Choice(&[("left", 0), ("center", 1)]),
            },
            Setting {
                key: "task_view",
                path: ADVANCED,
                name: "ShowTaskViewButton",
                kind: BOOL,
            },
            Setting {
                key: "search",
                path: SEARCH,
                name: "SearchboxTaskbarMode",
                kind: Kind::Choice(&[("hidden", 0), ("icon", 1), ("box", 2), ("button", 3)]),
            },
        ],
    }
}

pub fn explorer() -> RegModule {
    RegModule {
        name: "explorer",
        description: "File Explorer view options",
        settings: &[
            Setting {
                key: "show_extensions",
                path: ADVANCED,
                name: "HideFileExt",
                kind: INV_BOOL,
            },
            Setting {
                key: "show_hidden",
                path: ADVANCED,
                name: "Hidden",
                kind: Kind::Flag { on: 1, off: 2 },
            },
            Setting {
                key: "compact",
                path: ADVANCED,
                name: "UseCompactMode",
                kind: BOOL,
            },
            Setting {
                key: "open_to",
                path: ADVANCED,
                name: "LaunchTo",
                kind: Kind::Choice(&[("this_pc", 1), ("home", 2)]),
            },
        ],
    }
}

impl Setting {
    fn format_value(&self, v: &RegValue) -> Option<Value> {
        let RegValue::Dword(n) = *v else { return None };
        Some(match self.kind {
            Kind::Flag { on, off } if n == on || n == off => Value::Boolean(n == on),
            Kind::Choice(opts) => match opts.iter().find(|(_, x)| *x == n) {
                Some((label, _)) => Value::String(label.to_string()),
                None => Value::Integer(n.into()),
            },
            _ => Value::Integer(n.into()),
        })
    }

    fn parse_value(&self, v: &Value) -> Result<RegValue> {
        let n = match (&self.kind, v) {
            (Kind::Flag { on, off }, Value::Boolean(b)) => {
                if *b {
                    *on
                } else {
                    *off
                }
            }
            (Kind::Choice(opts), Value::String(s)) => match opts.iter().find(|(l, _)| l == s) {
                Some((_, n)) => *n,
                None => {
                    let allowed: Vec<_> = opts.iter().map(|(l, _)| *l).collect();
                    bail!(
                        "{}: unknown value \"{s}\", expected one of {allowed:?}",
                        self.key
                    )
                }
            },
            (_, Value::Integer(i)) => u32::try_from(*i)?,
            _ => bail!("{}: unexpected value {v}", self.key),
        };
        Ok(RegValue::Dword(n))
    }
}

impl Module for RegModule {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        self.description
    }

    fn capture(&self, sys: &dyn System, _ctx: &Ctx) -> Result<Table> {
        let mut t = Table::new();
        for s in self.settings {
            if let Some(v) = sys
                .reg_get(s.path, s.name)?
                .and_then(|v| s.format_value(&v))
            {
                t.insert(s.key.into(), v);
            }
        }
        Ok(t)
    }

    fn plan(&self, sys: &dyn System, section: &Table, _ctx: &Ctx) -> Result<Vec<Change>> {
        let mut changes = Vec::new();
        for (key, value) in section {
            let Some(s) = self.settings.iter().find(|s| s.key == key) else {
                crate::ui::warn(format_args!(
                    "{}.{key} is not supported, skipped",
                    self.name
                ));
                continue;
            };
            let target = s.parse_value(value)?;
            let current = sys.reg_get(s.path, s.name)?;
            if current.as_ref() != Some(&target) {
                changes.push(Change {
                    key: key.clone(),
                    from: current
                        .and_then(|c| s.format_value(&c))
                        .map(|v| v.to_string()),
                    to: value.to_string(),
                });
            }
        }
        Ok(changes)
    }

    fn apply(&self, sys: &mut dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        let changes = self.plan(sys, section, ctx)?;
        for c in &changes {
            let s = self.settings.iter().find(|s| s.key == c.key).unwrap();
            sys.reg_set(s.path, s.name, &s.parse_value(&section[&c.key])?)?;
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::mock::MockSystem;

    fn ctx() -> Ctx {
        Ctx {
            assets: "assets".into(),
            data_dir: "data".into(),
        }
    }

    #[test]
    fn capture_maps_values() {
        let sys = MockSystem::default()
            .with(PERSONALIZE, "AppsUseLightTheme", RegValue::Dword(0))
            .with(ADVANCED, "TaskbarAl", RegValue::Dword(0))
            .with(ADVANCED, "Hidden", RegValue::Dword(2));
        let theme = theme().capture(&sys, &ctx()).unwrap();
        assert_eq!(theme["dark_apps"], Value::Boolean(true));
        let taskbar = taskbar().capture(&sys, &ctx()).unwrap();
        assert_eq!(taskbar["alignment"], Value::String("left".into()));
        let explorer = explorer().capture(&sys, &ctx()).unwrap();
        assert_eq!(explorer["show_hidden"], Value::Boolean(false));
    }

    #[test]
    fn apply_writes_only_changes() {
        let mut sys = MockSystem::default().with(ADVANCED, "TaskbarAl", RegValue::Dword(1));
        let section: Table = toml::from_str("alignment = \"left\"\ntask_view = false").unwrap();
        let changes = taskbar().apply(&mut sys, &section, &ctx()).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(
            sys.reg_get(ADVANCED, "TaskbarAl").unwrap(),
            Some(RegValue::Dword(0))
        );
        assert!(taskbar().plan(&sys, &section, &ctx()).unwrap().is_empty());
    }

    #[test]
    fn rejects_unknown_choice() {
        let sys = MockSystem::default();
        let section: Table = toml::from_str("alignment = \"right\"").unwrap();
        assert!(taskbar().plan(&sys, &section, &ctx()).is_err());
    }
}
