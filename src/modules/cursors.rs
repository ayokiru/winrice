//! Mouse cursors: HKCU\Control Panel\Cursors.
//!
//! Role values in setup.toml:
//!   "default"          - Windows built-in default (empty registry value)
//!   "system:<file>"    - file from %SystemRoot%\Cursors, not copied
//!   "cursors/<file>"   - file shipped in the setup's assets

use super::{Change, Ctx, Module, files};
use crate::sys::{RegValue, System};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

const CURSORS: &str = r"Control Panel\Cursors";

// (setup key, registry value name)
const ROLES: &[(&str, &str)] = &[
    ("arrow", "Arrow"),
    ("help", "Help"),
    ("working", "AppStarting"),
    ("busy", "Wait"),
    ("precision", "Crosshair"),
    ("text", "IBeam"),
    ("handwriting", "NWPen"),
    ("unavailable", "No"),
    ("resize_ns", "SizeNS"),
    ("resize_we", "SizeWE"),
    ("resize_nwse", "SizeNWSE"),
    ("resize_nesw", "SizeNESW"),
    ("move", "SizeAll"),
    ("alternate", "UpArrow"),
    ("link", "Hand"),
    ("location", "Pin"),
    ("person", "Person"),
];

pub struct Cursors;

fn system_dir(sys: &dyn System) -> String {
    let root = sys
        .env("SystemRoot")
        .unwrap_or_else(|| r"C:\Windows".into());
    format!("{}\\Cursors", root.trim_end_matches('\\'))
}

/// Splits a Windows path into (parent, file name); works on any host OS.
fn split_win(path: &str) -> (&str, &str) {
    match path.rfind(['\\', '/']) {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => ("", path),
    }
}

fn reg_string(sys: &dyn System, name: &str) -> Result<Option<String>> {
    Ok(match sys.reg_get(CURSORS, name)? {
        Some(RegValue::Str(s)) => Some(s),
        _ => None,
    })
}

/// Current cursor file for a role, None means Windows default.
fn current(sys: &dyn System, name: &str) -> Result<Option<String>> {
    Ok(reg_string(sys, name)?
        .filter(|s| !s.is_empty())
        .map(|s| files::expand(sys, &s)))
}

enum Target {
    Default,
    System(String),
    Asset(PathBuf),
}

fn target(key: &str, v: &Value, ctx: &Ctx) -> Result<Target> {
    let Some(s) = v.as_str() else {
        bail!("cursors.{key}: expected a string, got {v}");
    };
    if s == "default" {
        return Ok(Target::Default);
    }
    if let Some(file) = s.strip_prefix("system:") {
        if file.is_empty() || file.contains(['\\', '/', ':']) {
            bail!("cursors.{key}: \"{s}\" must name a file in the system cursor folder");
        }
        return Ok(Target::System(file.into()));
    }
    Ok(Target::Asset(
        files::asset(ctx, s).with_context(|| format!("cursors.{key}"))?,
    ))
}

fn matches(sys: &dyn System, current: &Option<String>, target: &Target) -> bool {
    match (current, target) {
        (None, Target::Default) => true,
        (Some(c), Target::System(file)) => {
            let (dir, name) = split_win(c);
            name.eq_ignore_ascii_case(file) && dir.eq_ignore_ascii_case(&system_dir(sys))
        }
        (Some(c), Target::Asset(p)) => files::same_content(Path::new(c), p),
        _ => false,
    }
}

impl Module for Cursors {
    fn name(&self) -> &'static str {
        "cursors"
    }

    fn description(&self) -> &'static str {
        "Mouse cursor scheme, files and size"
    }

    fn capture(&self, sys: &dyn System, ctx: &Ctx) -> Result<Table> {
        let mut t = Table::new();
        if let Some(scheme) = reg_string(sys, "")?.filter(|s| !s.is_empty()) {
            t.insert("scheme".into(), Value::String(scheme));
        }
        if let Some(RegValue::Dword(size)) = sys.reg_get(CURSORS, "CursorBaseSize")? {
            t.insert("size".into(), Value::Integer(size.into()));
        }
        let sys_dir = system_dir(sys);
        for (key, name) in ROLES {
            let value = match current(sys, name)? {
                None => "default".to_string(),
                Some(path) => {
                    let (dir, file) = split_win(&path);
                    if dir.eq_ignore_ascii_case(&sys_dir) {
                        format!("system:{file}")
                    } else if Path::new(&path).is_file() {
                        let rel = format!("cursors/{key}.{}", files::ext(Path::new(&path), "cur"));
                        fs::create_dir_all(ctx.assets.join("cursors"))?;
                        fs::copy(&path, ctx.assets.join(&rel))?;
                        rel
                    } else {
                        crate::ui::warn(format_args!("cursor {path} not found, {key} skipped"));
                        continue;
                    }
                }
            };
            t.insert(key.to_string(), Value::String(value));
        }
        Ok(t)
    }

    fn plan(&self, sys: &dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        let mut changes = Vec::new();
        for (key, value) in section {
            let from = match key.as_str() {
                "scheme" => {
                    let cur = reg_string(sys, "")?.unwrap_or_default();
                    (value.as_str() != Some(cur.as_str())).then_some(Some(format!("\"{cur}\"")))
                }
                "size" => {
                    let cur = match sys.reg_get(CURSORS, "CursorBaseSize")? {
                        Some(RegValue::Dword(n)) => Some(i64::from(n)),
                        _ => None,
                    };
                    if value.as_integer().is_none() {
                        bail!("cursors.size: expected a number, got {value}");
                    }
                    (value.as_integer() != cur).then_some(cur.map(|c| c.to_string()))
                }
                _ => {
                    let Some((_, name)) = ROLES.iter().find(|(k, _)| k == key) else {
                        crate::ui::warn(format_args!("cursors.{key} is not supported, skipped"));
                        continue;
                    };
                    let cur = current(sys, name)?;
                    let t = target(key, value, ctx)?;
                    (!matches(sys, &cur, &t)).then_some(cur)
                }
            };
            if let Some(from) = from {
                changes.push(Change {
                    key: key.clone(),
                    from,
                    to: value.to_string(),
                });
            }
        }
        Ok(changes)
    }

    fn apply(&self, sys: &mut dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        let changes = self.plan(sys, section, ctx)?;
        for c in &changes {
            let value = &section[&c.key];
            match c.key.as_str() {
                "scheme" => {
                    let s = value.as_str().unwrap_or_default().to_string();
                    sys.reg_set(CURSORS, "", &RegValue::Str(s))?;
                }
                "size" => {
                    let n = u32::try_from(value.as_integer().unwrap_or(32))?;
                    sys.reg_set(CURSORS, "CursorBaseSize", &RegValue::Dword(n))?;
                }
                key => {
                    let (_, name) = ROLES.iter().find(|(k, _)| *k == key).unwrap();
                    let path = match target(key, value, ctx)? {
                        Target::Default => String::new(),
                        Target::System(file) => {
                            format!("{}\\{file}", system_dir(sys))
                        }
                        Target::Asset(p) => {
                            let dest = files::stable_copy(&p, &ctx.data_dir.join("cursors"))?;
                            dest.to_string_lossy().into_owned()
                        }
                    };
                    sys.reg_set(CURSORS, name, &RegValue::Str(path))?;
                }
            }
        }
        if !changes.is_empty() {
            sys.reload_cursors()?;
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::mock::MockSystem;

    #[test]
    fn capture_and_apply_roundtrip() {
        let root = std::env::temp_dir().join(format!("winrice-cursors-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let custom = root.join("my_hand.ani");
        fs::write(&custom, b"ani-bytes").unwrap();

        let mut source = MockSystem::default()
            .env("SystemRoot", r"C:\Windows")
            .with(CURSORS, "", RegValue::Str("Neon".into()))
            .with(CURSORS, "CursorBaseSize", RegValue::Dword(48))
            .with(
                CURSORS,
                "Arrow",
                RegValue::Str(r"%SystemRoot%\cursors\aero_arrow.cur".into()),
            )
            .with(CURSORS, "IBeam", RegValue::Str(String::new()));
        source.reg.insert(
            (CURSORS.into(), "Hand".into()),
            RegValue::Str(custom.to_string_lossy().into_owned()),
        );
        let ctx = Ctx {
            assets: root.join("assets"),
            data_dir: root.join("data"),
        };

        let section = Cursors.capture(&source, &ctx).unwrap();
        assert_eq!(section["arrow"].as_str(), Some("system:aero_arrow.cur"));
        assert_eq!(section["text"].as_str(), Some("default"));
        assert_eq!(section["link"].as_str(), Some("cursors/link.ani"));
        assert_eq!(section["size"].as_integer(), Some(48));
        assert!(Cursors.plan(&source, &section, &ctx).unwrap().is_empty());

        let mut target = MockSystem::default().env("SystemRoot", r"C:\Windows");
        let changes = Cursors.apply(&mut target, &section, &ctx).unwrap();
        assert!(changes.iter().any(|c| c.key == "link"));
        assert_eq!(target.cursor_reloads, 1);
        assert_eq!(
            target.reg_get(CURSORS, "Arrow").unwrap(),
            Some(RegValue::Str(r"C:\Windows\Cursors\aero_arrow.cur".into()))
        );
        assert!(Cursors.plan(&target, &section, &ctx).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
