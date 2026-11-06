//! Config files of ricing tools (YASB, GlazeWM, Fastfetch, ...) copied to where the tools read them.

use super::{Change, Ctx, Module, UNSET, files, unset_keys};
use crate::setup::LIST;
use crate::sys::System;
use anyhow::{Context, Result, anyhow, bail};
use std::fs;
use std::path::PathBuf;
use toml::{Table, Value};

/// Where setups may write: (variable, folder ending in `\` or exact file), lowercase.
const PLACES: &[(&str, &str)] = &[
    ("USERPROFILE", r".config\yasb\"),
    ("USERPROFILE", r".glzr\glazewm\"),
    ("USERPROFILE", r".glzr\zebar\"),
    ("USERPROFILE", r".config\fastfetch\"),
    ("USERPROFILE", r".config\ohmyposh\"),
    ("USERPROFILE", r".config\starship.toml"),
    ("APPDATA", r"fastfetch\"),
    ("LOCALAPPDATA", r"programs\oh-my-posh\themes\"),
];

/// File types these tools read; anything else (.scf, .html, scripts) is refused.
const FORMATS: &[&str] = &["yaml", "yml", "json", "jsonc", "toml", "css"];

/// Files capture picks up: (setup file, destination).
const CAPTURE: &[(&str, &str)] = &[
    (
        "configs/yasb/config.yaml",
        r"%USERPROFILE%\.config\yasb\config.yaml",
    ),
    (
        "configs/yasb/styles.css",
        r"%USERPROFILE%\.config\yasb\styles.css",
    ),
    (
        "configs/glazewm/config.yaml",
        r"%USERPROFILE%\.glzr\glazewm\config.yaml",
    ),
    (
        "configs/fastfetch/config.jsonc",
        r"%USERPROFILE%\.config\fastfetch\config.jsonc",
    ),
    (
        "configs/starship.toml",
        r"%USERPROFILE%\.config\starship.toml",
    ),
];

pub struct Configs;

/// Resolves `to`. Setups are untrusted, so only the known config places are allowed.
fn destination(sys: &dyn System, to: &str) -> Result<PathBuf> {
    let bad = || anyhow!("configs: \"{to}\" is not an allowed config location");
    let (var, rest) = to
        .strip_prefix('%')
        .and_then(|s| s.split_once('%'))
        .ok_or_else(bad)?;
    let var = ["USERPROFILE", "APPDATA", "LOCALAPPDATA"]
        .into_iter()
        .find(|v| v.eq_ignore_ascii_case(var))
        .ok_or_else(bad)?;
    if !rest.starts_with(['\\', '/']) {
        return Err(bad());
    }
    let parts: Vec<&str> = rest.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
    let unsafe_part = |p: &&str| {
        matches!(*p, "." | "..")
            || p.ends_with(['.', ' '])
            || p.chars()
                .any(|c| c.is_control() || r#":*?"<>|%"#.contains(c))
    };
    if parts.iter().any(unsafe_part) {
        return Err(bad());
    }
    let rel = parts.join("\\").to_lowercase();
    let allowed = PLACES.iter().any(|(v, place)| {
        *v == var
            && if place.ends_with('\\') {
                rel.len() > place.len() && rel.starts_with(place)
            } else {
                rel == *place
            }
    });
    let name = parts.last().copied().unwrap_or_default().to_lowercase();
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    if !allowed || !FORMATS.contains(&ext) {
        return Err(bad());
    }
    let base = sys
        .env(var)
        .map(PathBuf::from)
        .filter(|b| b.is_absolute())
        .ok_or_else(|| anyhow!("{var} is not set"))?;
    Ok(parts.iter().fold(base, |p, part| p.join(part)))
}

/// (setup file, destination) pairs of the section.
fn entries(section: &Table) -> Result<Vec<(&str, &str)>> {
    let Some(list) = section.get(LIST) else {
        return Ok(Vec::new());
    };
    let Some(list) = list.as_array() else {
        bail!("configs must be a list of [[configs]] entries");
    };
    list.iter()
        .map(|e| {
            match (
                e.get("file").and_then(Value::as_str),
                e.get("to").and_then(Value::as_str),
            ) {
                (Some(file), Some(to)) => Ok((file, to)),
                _ => bail!("configs: every entry needs `file` and `to`"),
            }
        })
        .collect()
}

/// Changes to make: each with its destination and the setup file to copy (None = delete).
fn compute(
    sys: &dyn System,
    section: &Table,
    ctx: &Ctx,
) -> Result<Vec<(Change, PathBuf, Option<PathBuf>)>> {
    let mut out = Vec::new();
    for (file, to) in entries(section)? {
        let src = files::asset(ctx, file).context("configs")?;
        let dest = destination(sys, to)?;
        if !files::same_content(&src, &dest) {
            let change = Change {
                key: to.into(),
                from: dest.exists().then(|| "current file".into()),
                to: file.into(),
            };
            out.push((change, dest, Some(src)));
        }
    }
    for to in unset_keys(section)? {
        let dest = destination(sys, to)?;
        if dest.is_file() {
            let change = Change {
                key: to.into(),
                from: Some("current file".into()),
                to: "removed".into(),
            };
            out.push((change, dest, None));
        }
    }
    Ok(out)
}

fn entry(file: &str, to: &str) -> Value {
    let mut t = Table::new();
    t.insert("file".into(), Value::String(file.into()));
    t.insert("to".into(), Value::String(to.into()));
    Value::Table(t)
}

impl Module for Configs {
    fn name(&self) -> &'static str {
        "configs"
    }

    fn description(&self) -> &'static str {
        "Config files of ricing tools (YASB, GlazeWM, Fastfetch, Starship, Oh My Posh)"
    }

    fn capture(&self, sys: &dyn System, ctx: &Ctx) -> Result<Table> {
        let mut list = Vec::new();
        for (rel, to) in CAPTURE {
            let Ok(src) = destination(sys, to) else {
                continue;
            };
            if src.is_file() {
                let dest = ctx.assets.join(rel);
                fs::create_dir_all(dest.parent().unwrap_or(&ctx.assets))?;
                fs::copy(&src, &dest)?;
                list.push(entry(rel, to));
            }
        }
        let mut t = Table::new();
        if !list.is_empty() {
            crate::ui::note(format_args!(
                "check the captured config files for API keys and personal paths before sharing"
            ));
            t.insert(LIST.into(), Value::Array(list));
        }
        Ok(t)
    }

    /// Copies of the files the section replaces or deletes; files it creates are listed for deletion.
    fn backup(&self, sys: &dyn System, section: &Table, ctx: &Ctx) -> Result<Table> {
        let mut list = Vec::new();
        let mut unset = Vec::new();
        let written = entries(section)?.into_iter().map(|(_, to)| to);
        for (i, to) in written.chain(unset_keys(section)?).enumerate() {
            let dest = destination(sys, to)?;
            if dest.is_file() {
                let name = dest.file_name().unwrap_or_default().to_string_lossy();
                let rel = format!("configs/{i}-{name}");
                fs::create_dir_all(ctx.assets.join("configs"))?;
                fs::copy(&dest, ctx.assets.join(&rel))?;
                list.push(entry(&rel, to));
            } else {
                unset.push(Value::String(to.into()));
            }
        }
        let mut t = Table::new();
        if !list.is_empty() {
            t.insert(LIST.into(), Value::Array(list));
        }
        if !unset.is_empty() {
            t.insert(UNSET.into(), Value::Array(unset));
        }
        Ok(t)
    }

    fn plan(&self, sys: &dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        let ops = compute(sys, section, ctx)?;
        if ops.iter().any(|(_, _, src)| src.is_some()) {
            crate::ui::note(format_args!(
                "config files can make tools like YASB or GlazeWM run commands, read them before applying a setup from someone you do not know"
            ));
        }
        Ok(ops.into_iter().map(|(c, _, _)| c).collect())
    }

    fn apply(&self, sys: &mut dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        let ops = compute(sys, section, ctx)?;
        let mut changes = Vec::new();
        for (change, dest, src) in ops {
            match src {
                Some(src) => {
                    if let Some(dir) = dest.parent() {
                        fs::create_dir_all(dir)?;
                    }
                    fs::copy(&src, &dest)
                        .with_context(|| format!("cannot write {}", change.key))?;
                }
                None => fs::remove_file(&dest)
                    .with_context(|| format!("cannot remove {}", change.key))?,
            }
            changes.push(change);
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::mock::MockSystem;
    use std::path::Path;

    fn env(root: &Path) -> MockSystem {
        MockSystem::default()
            .env("USERPROFILE", root.join("home").to_string_lossy())
            .env("APPDATA", root.join("roaming").to_string_lossy())
            .env("LOCALAPPDATA", root.join("local").to_string_lossy())
    }

    #[test]
    fn allows_known_config_places_only() {
        let sys = env(&std::env::temp_dir());
        for ok in [
            r"%USERPROFILE%\.config\yasb\config.yaml",
            r"%userprofile%/.config/yasb/themes/dark.css",
            r"%USERPROFILE%\.glzr\glazewm\config.yaml",
            r"%USERPROFILE%\.config\starship.toml",
            r"%APPDATA%\fastfetch\config.jsonc",
            r"%LOCALAPPDATA%\Programs\oh-my-posh\themes\night.omp.json",
        ] {
            assert!(destination(&sys, ok).is_ok(), "{ok}");
        }
        for bad in [
            r"%USERPROFILE%\.config\yasb",
            r"%USERPROFILE%\.config\yasb\..\..\evil.yaml",
            r"%USERPROFILE%\.config\yasb\run.ps1",
            r"%USERPROFILE%\.config\yasb\desktop.scf",
            r"%USERPROFILE%\.config\yasb\x.library-ms",
            r"%USERPROFILE%\.glzr\zebar\widget.html",
            r"%USERPROFILE%\.config\yasb\noext",
            r"%USERPROFILE%\.config\yasb\run.BAT",
            r"%USERPROFILE%\.config\yasb\run.bat.",
            r"%USERPROFILE%\.config\yasb\a.yaml:stream",
            r"%USERPROFILE%\.config\yasb\%TEMP%\a.yaml",
            r"%USERPROFILE%\.config\starship.toml\x",
            r"%USERPROFILE%\.bashrc",
            r"%USERPROFILE%x\.config\yasb\config.yaml",
            r"%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\a.yaml",
            r"%TEMP%\.config\yasb\config.yaml",
            r"D:\rice\.config\yasb\config.yaml",
            r"\\evil\share\.config\yasb\config.yaml",
            "",
        ] {
            assert!(destination(&sys, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn captures_applies_and_rolls_back() {
        let root = std::env::temp_dir().join(format!("winrice-configs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let yasb = root.join("src/home/.config/yasb");
        fs::create_dir_all(&yasb).unwrap();
        fs::write(yasb.join("config.yaml"), "bar: new").unwrap();
        fs::write(yasb.join("styles.css"), "a {}").unwrap();
        let ctx = Ctx {
            assets: root.join("setup"),
            data_dir: root.join("data"),
        };
        let section = Configs.capture(&env(&root.join("src")), &ctx).unwrap();
        assert_eq!(section[LIST].as_array().unwrap().len(), 2);

        // The target has its own config.yaml but no styles.css.
        let mut target = env(&root.join("dst"));
        let dst = root.join("dst/home/.config/yasb");
        fs::create_dir_all(&dst).unwrap();
        fs::write(dst.join("config.yaml"), "bar: old").unwrap();
        let backup_ctx = Ctx {
            assets: root.join("backup"),
            data_dir: root.join("data"),
        };
        let backup = Configs.backup(&target, &section, &backup_ctx).unwrap();
        assert_eq!(Configs.apply(&mut target, &section, &ctx).unwrap().len(), 2);
        assert_eq!(
            fs::read_to_string(dst.join("config.yaml")).unwrap(),
            "bar: new"
        );
        assert!(Configs.plan(&target, &section, &ctx).unwrap().is_empty());

        assert_eq!(
            Configs
                .apply(&mut target, &backup, &backup_ctx)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            fs::read_to_string(dst.join("config.yaml")).unwrap(),
            "bar: old"
        );
        assert!(!dst.join("styles.css").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_restores_files_a_setup_deletes() {
        let root =
            std::env::temp_dir().join(format!("winrice-configs-unset-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let glaze = root.join("home/.glzr/glazewm");
        fs::create_dir_all(&glaze).unwrap();
        fs::write(glaze.join("config.yaml"), "mine").unwrap();
        let mut sys = env(&root);
        let ctx = Ctx {
            assets: root.join("setup"),
            data_dir: root.join("data"),
        };
        let backup_ctx = Ctx {
            assets: root.join("backup"),
            data_dir: root.join("data"),
        };
        let section: Table =
            toml::from_str(r#"unset = ['%USERPROFILE%\.glzr\glazewm\config.yaml']"#).unwrap();
        let backup = Configs.backup(&sys, &section, &backup_ctx).unwrap();
        Configs.apply(&mut sys, &section, &ctx).unwrap();
        assert!(!glaze.join("config.yaml").exists());
        Configs.apply(&mut sys, &backup, &backup_ctx).unwrap();
        assert_eq!(
            fs::read_to_string(glaze.join("config.yaml")).unwrap(),
            "mine"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_setup_files_outside_the_setup() {
        let sys = env(&std::env::temp_dir());
        let ctx = Ctx {
            assets: "assets".into(),
            data_dir: "data".into(),
        };
        let section: Table = toml::from_str(
            r#"list = [{ file = '..\..\secret.yaml', to = '%USERPROFILE%\.config\yasb\config.yaml' }]"#,
        )
        .unwrap();
        assert!(Configs.plan(&sys, &section, &ctx).is_err());
    }
}
