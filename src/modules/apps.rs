//! Apps installed with winget. Rollback never uninstalls them.

use super::{Change, Ctx, Module};
use crate::sys::System;
use anyhow::{Context, Result, bail};
use toml::{Table, Value};

/// Look and ricing tools that capture picks up when installed. Ids checked with `winget show`.
const KNOWN: &[&str] = &[
    "AmN.yasb",
    "RamenSoftware.Windhawk",
    "glzr-io.glazewm",
    "glzr-io.zebar",
    "Flow-Launcher.Flow-Launcher",
    "Rainmeter.Rainmeter",
    "Nilesoft.Shell",
    "Microsoft.PowerToys",
    "Fastfetch-cli.Fastfetch",
    "JanDeDobbeleer.OhMyPosh",
    "Starship.Starship",
];

pub struct Apps;

/// Setups are untrusted: an id must not look like a winget option.
fn valid_id(id: &str) -> bool {
    id.len() <= 128
        && id.starts_with(|c: char| c.is_ascii_alphanumeric())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

fn ids(section: &Table) -> Result<Vec<&str>> {
    let Some(list) = section.get("winget") else {
        return Ok(Vec::new());
    };
    let Some(list) = list.as_array() else {
        bail!("apps.winget must be a list of package ids");
    };
    list.iter()
        .map(|v| match v.as_str() {
            Some(id) if valid_id(id) => Ok(id),
            _ => bail!("apps.winget: {v} is not a winget package id"),
        })
        .collect()
}

fn missing<'a>(sys: &dyn System, section: &'a Table) -> Result<Vec<&'a str>> {
    let mut out = Vec::new();
    for id in ids(section)? {
        if !sys.winget_installed(id)? {
            out.push(id);
        }
    }
    Ok(out)
}

fn changes(ids: &[&str]) -> Vec<Change> {
    ids.iter()
        .map(|id| Change {
            key: id.to_string(),
            from: None,
            to: "install".into(),
        })
        .collect()
}

impl Module for Apps {
    fn name(&self) -> &'static str {
        "apps"
    }

    fn description(&self) -> &'static str {
        "Apps installed with winget (rollback does not uninstall them)"
    }

    fn capture(&self, sys: &dyn System, _ctx: &Ctx) -> Result<Table> {
        let mut found = Vec::new();
        for id in KNOWN {
            match sys.winget_installed(id) {
                Ok(true) => found.push(Value::String(id.to_string())),
                Ok(false) => {}
                Err(e) => {
                    crate::ui::warn(format_args!("{e:#}, apps skipped"));
                    return Ok(Table::new());
                }
            }
        }
        let mut t = Table::new();
        if !found.is_empty() {
            t.insert("winget".into(), Value::Array(found));
        }
        Ok(t)
    }

    /// Rollback does not uninstall apps, so there is nothing to save.
    fn backup(&self, _sys: &dyn System, _ctx: &Ctx) -> Result<Table> {
        Ok(Table::new())
    }

    fn plan(&self, sys: &dyn System, section: &Table, _ctx: &Ctx) -> Result<Vec<Change>> {
        let list = missing(sys, section).context("cannot check installed apps")?;
        if !list.is_empty() {
            crate::ui::note(format_args!(
                "winrice will install: {} (their license terms apply). Rollback does not uninstall them.",
                list.join(", ")
            ));
        }
        Ok(changes(&list))
    }

    fn apply(&self, sys: &mut dyn System, section: &Table, _ctx: &Ctx) -> Result<Vec<Change>> {
        let list = missing(sys, section).context("cannot check installed apps")?;
        for id in &list {
            crate::ui::step(format_args!("installing {id} with winget"));
            sys.winget_install(id)?;
        }
        Ok(changes(&list))
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
    fn captures_only_known_tools() {
        let sys = MockSystem {
            apps: vec!["AmN.yasb".into(), "Mozilla.Firefox".into()],
            ..MockSystem::default()
        };
        let t = Apps.capture(&sys, &ctx()).unwrap();
        assert_eq!(t["winget"].as_array().unwrap(), &[Value::from("AmN.yasb")]);
    }

    #[test]
    fn installs_missing_apps_only() {
        let mut sys = MockSystem {
            apps: vec!["AmN.yasb".into()],
            ..MockSystem::default()
        };
        let section: Table =
            toml::from_str(r#"winget = ["AmN.yasb", "Microsoft.PowerToys"]"#).unwrap();
        let plan = Apps.plan(&sys, &section, &ctx()).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].key, "Microsoft.PowerToys");
        Apps.apply(&mut sys, &section, &ctx()).unwrap();
        assert_eq!(sys.installs, ["Microsoft.PowerToys"]);
        assert!(Apps.plan(&sys, &section, &ctx()).unwrap().is_empty());
        assert!(Apps.backup(&sys, &ctx()).unwrap().is_empty());
    }

    #[test]
    fn rejects_ids_that_look_like_options() {
        for bad in ["--override", "-h", "a b", "x&calc", ""] {
            let section: Table = toml::from_str(&format!("winget = [{bad:?}]")).unwrap();
            assert!(ids(&section).is_err(), "{bad}");
        }
        let section: Table = toml::from_str(r#"winget = ["Fastfetch-cli.Fastfetch"]"#).unwrap();
        assert!(ids(&section).is_ok());
    }
}
