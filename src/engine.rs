use crate::modules::{self, Change, Ctx, Module};
use crate::setup::{self, Meta, Setup};
use crate::sys::System;
use anyhow::{Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const KEEP_BACKUPS: usize = 10;

pub fn data_dir() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(d) => PathBuf::from(d).join("winrice"),
        None => std::env::temp_dir().join("winrice"),
    }
}

fn ctx(setup_dir: &Path, data_dir: &Path) -> Ctx {
    Ctx {
        assets: setup_dir.join(setup::ASSETS),
        data_dir: data_dir.to_path_buf(),
    }
}

/// Modules in the order they should run, limited to `names` if given.
fn select(names: &[String]) -> Result<Vec<Box<dyn Module>>> {
    for n in names {
        if modules::find(n).is_none() {
            bail!("unknown module \"{n}\" (see `winrice modules`)");
        }
    }
    Ok(modules::all()
        .into_iter()
        .filter(|m| names.is_empty() || names.iter().any(|n| n == m.name()))
        .collect())
}

pub fn capture(
    sys: &dyn System,
    out: &Path,
    name: &str,
    only: &[String],
    data_dir: &Path,
) -> Result<Setup> {
    save(sys, out, name, &select(only)?, data_dir, None)
}

fn save(
    sys: &dyn System,
    out: &Path,
    name: &str,
    modules: &[Box<dyn Module>],
    data_dir: &Path,
    backup_of: Option<&Setup>,
) -> Result<Setup> {
    let ctx = ctx(out, data_dir);
    let mut setup = Setup {
        meta: Meta {
            name: name.into(),
            winrice: env!("CARGO_PKG_VERSION").into(),
            ..Meta::default()
        },
        ..Setup::default()
    };
    for m in modules {
        let section = match backup_of {
            Some(s) => m.backup(sys, &s.sections[m.name()], &ctx)?,
            None => m.capture(sys, &ctx)?,
        };
        if !section.is_empty() {
            setup.sections.insert(m.name().into(), section);
        }
    }
    setup.save(out)?;
    Ok(setup)
}

/// The setup's modules in run order; unknown sections are skipped with a warning.
fn setup_modules(setup: &Setup) -> Vec<Box<dyn Module>> {
    for n in setup.sections.keys().filter(|n| modules::find(n).is_none()) {
        crate::ui::warn(format_args!(
            "module \"{n}\" is not supported by this version, skipped"
        ));
    }
    modules::all()
        .into_iter()
        .filter(|m| setup.sections.contains_key(m.name()))
        .collect()
}

pub fn plan(
    sys: &dyn System,
    setup: &Setup,
    dir: &Path,
    data_dir: &Path,
) -> Result<Vec<(String, Vec<Change>)>> {
    let ctx = ctx(dir, data_dir);
    let mut out = Vec::new();
    for m in setup_modules(setup) {
        let changes = m.plan(sys, &setup.sections[m.name()], &ctx)?;
        out.push((m.name().to_string(), changes));
    }
    Ok(out)
}

pub fn apply(
    sys: &mut dyn System,
    setup: &Setup,
    dir: &Path,
    data_dir: &Path,
    backup: bool,
) -> Result<Vec<(String, Vec<Change>)>> {
    let modules = setup_modules(setup);
    if backup {
        let path = data_dir.join("backups").join(timestamp());
        save(sys, &path, "backup", &modules, data_dir, Some(setup))?;
        prune_backups(data_dir)?;
    }
    let ctx = ctx(dir, data_dir);
    let mut out = Vec::new();
    for m in &modules {
        let changes = m.apply(sys, &setup.sections[m.name()], &ctx)?;
        out.push((m.name().to_string(), changes));
    }
    sys.notify_changed()?;
    Ok(out)
}

fn timestamp() -> String {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}{:03}", t.as_secs(), t.subsec_millis())
}

fn backups(data_dir: &Path) -> Result<Vec<PathBuf>> {
    let dir = data_dir.join("backups");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut list: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join(setup::FILE).is_file())
        .collect();
    list.sort();
    Ok(list)
}

fn prune_backups(data_dir: &Path) -> Result<()> {
    let list = backups(data_dir)?;
    for old in list.iter().take(list.len().saturating_sub(KEEP_BACKUPS)) {
        fs::remove_dir_all(old)?;
    }
    Ok(())
}

/// Restores the newest backup and removes it.
pub fn rollback(sys: &mut dyn System, data_dir: &Path) -> Result<Vec<(String, Vec<Change>)>> {
    let Some(latest) = backups(data_dir)?.pop() else {
        bail!("no backups found in {}", data_dir.join("backups").display());
    };
    let setup = Setup::load(&latest)?;
    let changes = apply(sys, &setup, &latest, data_dir, false)?;
    fs::remove_dir_all(&latest)?;
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::RegValue;
    use crate::sys::mock::MockSystem;

    const ADVANCED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";

    #[test]
    fn apply_backs_up_and_rolls_back() {
        let root = std::env::temp_dir().join(format!("winrice-engine-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let data = root.join("data");

        let source = MockSystem::default().with(ADVANCED, "TaskbarAl", RegValue::Dword(0));
        let setup = capture(&source, &root.join("setup"), "left", &[], &data).unwrap();

        let mut target = MockSystem::default().with(ADVANCED, "TaskbarAl", RegValue::Dword(1));
        let changes = apply(&mut target, &setup, &root.join("setup"), &data, true).unwrap();
        assert_eq!(changes[0].1.len(), 1);
        assert_eq!(
            target.reg_get(ADVANCED, "TaskbarAl").unwrap(),
            Some(RegValue::Dword(0))
        );
        assert_eq!(target.notified, 1);

        rollback(&mut target, &data).unwrap();
        assert_eq!(
            target.reg_get(ADVANCED, "TaskbarAl").unwrap(),
            Some(RegValue::Dword(1))
        );
        assert!(backups(&data).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_removes_values_that_did_not_exist() {
        let root = std::env::temp_dir().join(format!("winrice-unset-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let data = root.join("data");
        let setup: Setup = {
            let mut s = Setup::default();
            s.sections.insert(
                "taskbar".into(),
                toml::from_str("alignment = \"left\"").unwrap(),
            );
            s
        };
        let mut sys = MockSystem::default();
        apply(&mut sys, &setup, &root, &data, true).unwrap();
        assert_eq!(
            sys.reg_get(ADVANCED, "TaskbarAl").unwrap(),
            Some(RegValue::Dword(0))
        );
        let undone = rollback(&mut sys, &data).unwrap();
        assert_eq!(undone[0].1.len(), 1);
        assert_eq!(sys.reg_get(ADVANCED, "TaskbarAl").unwrap(), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_unknown_module() {
        assert!(select(&["nope".into()]).is_err());
    }
}
