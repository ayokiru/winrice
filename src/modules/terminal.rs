//! Windows Terminal look: theme, profile defaults, custom color schemes and themes.

use super::{Change, Ctx, Module, files};
use crate::sys::System;
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value as J};
use std::fs;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

const SETTINGS: &[&str] = &[
    r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json",
    r"Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json",
    r"Microsoft\Windows Terminal\settings.json",
];

const ROOT_KEYS: &[&str] = &["theme", "useAcrylicInTabRow"];

const PROFILE_KEYS: &[&str] = &[
    "colorScheme",
    "font",
    "opacity",
    "useAcrylic",
    "padding",
    "cursorShape",
    "cursorHeight",
    "backgroundImage",
    "backgroundImageOpacity",
    "backgroundImageStretchMode",
    "backgroundImageAlignment",
    "intenseTextStyle",
    "antialiasingMode",
];

pub struct Terminal;

fn settings_path(sys: &dyn System) -> Option<PathBuf> {
    let base = PathBuf::from(sys.env("LOCALAPPDATA")?);
    SETTINGS
        .iter()
        .map(|rel| base.join(rel.replace('\\', std::path::MAIN_SEPARATOR_STR)))
        .find(|p| p.is_file())
}

/// Removes comments and trailing commas so serde_json can read the file.
fn strip_jsonc(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                while let Some(c) = chars.next() {
                    out.push(c);
                    match c {
                        '\\' => out.extend(chars.next()),
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                while chars.peek().is_some_and(|&c| c != '\n') {
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = ' ';
                for c in chars.by_ref() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
            }
            '}' | ']' => {
                let trimmed = out.trim_end().len();
                if out[..trimmed].ends_with(',') {
                    out.truncate(trimmed - 1);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn load(path: &Path) -> Result<J> {
    let text = fs::read_to_string(path)?;
    let text = text.trim_start_matches('\u{feff}');
    serde_json::from_str(&strip_jsonc(text))
        .with_context(|| format!("cannot parse {}", path.display()))
}

fn json_to_toml(v: &J) -> Option<Value> {
    Some(match v {
        J::Null => return None,
        J::Bool(b) => Value::Boolean(*b),
        J::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Float(n.as_f64()?),
        },
        J::String(s) => Value::String(s.clone()),
        J::Array(a) => Value::Array(a.iter().filter_map(json_to_toml).collect()),
        J::Object(o) => Value::Table(
            o.iter()
                .filter_map(|(k, v)| json_to_toml(v).map(|t| (k.clone(), t)))
                .collect(),
        ),
    })
}

fn toml_to_json(v: &Value) -> J {
    match v {
        Value::Boolean(b) => J::Bool(*b),
        Value::Integer(i) => J::from(*i),
        Value::Float(f) => J::from(*f),
        Value::String(s) => J::String(s.clone()),
        Value::Datetime(d) => J::String(d.to_string()),
        Value::Array(a) => J::Array(a.iter().map(toml_to_json).collect()),
        Value::Table(t) => J::Object(
            t.iter()
                .map(|(k, v)| (k.clone(), toml_to_json(v)))
                .collect(),
        ),
    }
}

/// Profile defaults merged with the default profile: what the user actually sees.
fn effective_profile(doc: &J) -> Map<String, J> {
    let mut out = Map::new();
    let profiles = &doc["profiles"];
    if let Some(defaults) = profiles["defaults"].as_object() {
        out.extend(defaults.clone());
    }
    let default_guid = doc["defaultProfile"].as_str().unwrap_or_default();
    let list = profiles["list"].as_array().or(profiles.as_array());
    if let Some(p) = list
        .into_iter()
        .flatten()
        .find(|p| {
            p["guid"]
                .as_str()
                .is_some_and(|g| g.eq_ignore_ascii_case(default_guid))
        })
        .and_then(J::as_object)
    {
        out.extend(p.clone());
    }
    out.retain(|k, _| PROFILE_KEYS.contains(&k.as_str()));
    out
}

/// Scheme names referenced by colorScheme (a name or {light, dark}).
fn scheme_names(v: Option<&J>) -> Vec<String> {
    match v {
        Some(J::String(s)) => vec![s.clone()],
        Some(J::Object(o)) => o
            .values()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}

fn find_named<'a>(doc: &'a J, list: &str, names: &[String]) -> Vec<&'a J> {
    doc[list]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| {
            s["name"]
                .as_str()
                .is_some_and(|n| names.iter().any(|x| x == n))
        })
        .collect()
}

/// Font faces used by the current Terminal look.
pub fn font_faces(sys: &dyn System) -> Vec<String> {
    let Some(doc) = settings_path(sys).and_then(|p| load(&p).ok()) else {
        return Vec::new();
    };
    let profile = effective_profile(&doc);
    let face = profile
        .get("font")
        .and_then(|f| f["face"].as_str())
        .unwrap_or_default();
    face.split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(String::from)
        .collect()
}

const RESTORE: &str = "restore";
const RESTORE_FILE: &str = "terminal/settings.json";

/// Backups carry a full settings.json copy: (live file, copy to put back).
fn restore(sys: &dyn System, section: &Table, ctx: &Ctx) -> Result<Option<(PathBuf, PathBuf)>> {
    let Some(v) = section.get(RESTORE) else {
        return Ok(None);
    };
    let Some(rel) = v.as_str() else {
        bail!("terminal.{RESTORE}: expected a file name, got {v}");
    };
    let src = files::asset(ctx, rel)?;
    Ok(settings_path(sys).map(|path| (path, src)))
}

/// Setup file to use as the background; only setup files or "desktopWallpaper" are allowed.
fn background(key: &str, v: &Value, ctx: &Ctx) -> Result<Option<PathBuf>> {
    if key != "backgroundImage" {
        return Ok(None);
    }
    match v.as_str() {
        Some("desktopWallpaper") => Ok(None),
        Some(rel) => files::asset(ctx, rel)
            .context("terminal.defaults.backgroundImage")
            .map(Some),
        None => bail!("terminal.defaults.backgroundImage: expected a file name, got {v}"),
    }
}

fn set(changes: &mut Vec<Change>, slot: &mut J, key: String, new: J) {
    if *slot != new {
        changes.push(Change {
            key,
            from: (!slot.is_null()).then(|| slot.to_string()),
            to: new.to_string(),
        });
        *slot = new;
    }
}

/// Builds the updated document, the changes, and the setup files apply must copy.
fn compute(doc: &J, section: &Table, ctx: &Ctx) -> Result<(J, Vec<Change>, Vec<PathBuf>)> {
    let mut doc = doc.clone();
    let mut changes = Vec::new();
    let mut copies = Vec::new();

    for (key, value) in section {
        match key.as_str() {
            k if ROOT_KEYS.contains(&k) => {
                let slot = &mut doc[k];
                set(&mut changes, slot, key.clone(), toml_to_json(value));
            }
            "defaults" => {
                let Some(defaults) = value.as_table() else {
                    bail!("terminal.defaults must be a table");
                };
                if !doc["profiles"].is_object() {
                    // Legacy files keep profiles as a plain list.
                    let list = doc["profiles"].take();
                    doc["profiles"] = serde_json::json!({ "list": list });
                }
                for (k, v) in defaults {
                    if !PROFILE_KEYS.contains(&k.as_str()) {
                        crate::ui::warn(format_args!(
                            "terminal.defaults.{k} is not supported, skipped"
                        ));
                        continue;
                    }
                    let mut new = toml_to_json(v);
                    if let Some(src) = background(k, v, ctx)? {
                        let dest = files::stable_path(&src, &ctx.data_dir.join("terminal"))?;
                        new = J::String(dest.to_string_lossy().into_owned());
                        copies.push(src);
                    }
                    let slot = &mut doc["profiles"]["defaults"][k];
                    set(&mut changes, slot, format!("defaults.{k}"), new.clone());
                    // Profiles that override the key would hide the new default.
                    if let Some(list) = doc["profiles"]["list"].as_array_mut() {
                        for p in list.iter_mut().filter(|p| p.get(k).is_some()) {
                            let name = p["name"].as_str().unwrap_or("profile").to_string();
                            set(
                                &mut changes,
                                &mut p[k],
                                format!("profiles[{name}].{k}"),
                                new.clone(),
                            );
                        }
                    }
                }
            }
            "schemes" | "themes" => {
                let Some(items) = value.as_array() else {
                    bail!("terminal.{key} must be an array");
                };
                for item in items {
                    let new = toml_to_json(item);
                    let Some(name) = new["name"].as_str().map(String::from) else {
                        bail!("terminal.{key}: every entry needs a name");
                    };
                    if !doc[key.as_str()].is_array() {
                        doc[key.as_str()] = J::Array(Vec::new());
                    }
                    let list = doc[key.as_str()].as_array_mut().unwrap();
                    match list.iter_mut().find(|s| s["name"] == name.as_str()) {
                        Some(slot) => set(&mut changes, slot, format!("{key}[{name}]"), new),
                        None => {
                            changes.push(Change {
                                key: format!("{key}[{name}]"),
                                from: None,
                                to: "(added)".into(),
                            });
                            list.push(new);
                        }
                    }
                }
            }
            _ => crate::ui::warn(format_args!("terminal.{key} is not supported, skipped")),
        }
    }
    Ok((doc, changes, copies))
}

fn write_settings(path: &Path, doc: &J) -> Result<()> {
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    serde::Serialize::serialize(doc, &mut ser)?;
    buf.push(b'\n');
    fs::write(path, buf)?;
    Ok(())
}

impl Module for Terminal {
    fn name(&self) -> &'static str {
        "terminal"
    }

    fn description(&self) -> &'static str {
        "Windows Terminal theme, font, colors and background"
    }

    fn capture(&self, sys: &dyn System, ctx: &Ctx) -> Result<Table> {
        let mut t = Table::new();
        let Some(path) = settings_path(sys) else {
            return Ok(t);
        };
        let doc = load(&path)?;

        for k in ROOT_KEYS {
            if let Some(v) = doc.get(*k).and_then(json_to_toml) {
                t.insert(k.to_string(), v);
            }
        }

        let mut profile = effective_profile(&doc);
        if let Some(J::String(bg)) = profile.get("backgroundImage").cloned() {
            let src = PathBuf::from(files::expand(sys, &bg));
            if src.is_file() {
                let rel = format!("terminal/background.{}", files::ext(&src, "png"));
                fs::create_dir_all(ctx.assets.join("terminal"))?;
                fs::copy(&src, ctx.assets.join(&rel))?;
                profile.insert("backgroundImage".into(), J::String(rel));
            } else if bg != "desktopWallpaper" {
                crate::ui::warn(format_args!("terminal background {bg} not found, skipped"));
                profile.remove("backgroundImage");
            }
        }

        let schemes = scheme_names(profile.get("colorScheme"));
        // A stock Terminal has nothing to share: skip the empty table.
        if let Some(defaults) = json_to_toml(&J::Object(profile))
            .filter(|v| v.as_table().is_some_and(|t| !t.is_empty()))
        {
            t.insert("defaults".into(), defaults);
        }
        let custom: Vec<Value> = find_named(&doc, "schemes", &schemes)
            .into_iter()
            .filter_map(json_to_toml)
            .collect();
        if !custom.is_empty() {
            t.insert("schemes".into(), Value::Array(custom));
        }
        let themes: Vec<Value> = find_named(&doc, "themes", &scheme_names(doc.get("theme")))
            .into_iter()
            .filter_map(json_to_toml)
            .collect();
        if !themes.is_empty() {
            t.insert("themes".into(), Value::Array(themes));
        }
        Ok(t)
    }

    /// The whole file, so rollback restores every profile and the comments too.
    fn backup(&self, sys: &dyn System, _section: &Table, ctx: &Ctx) -> Result<Table> {
        let mut t = Table::new();
        if let Some(path) = settings_path(sys) {
            fs::create_dir_all(ctx.assets.join("terminal"))?;
            fs::copy(&path, ctx.assets.join(RESTORE_FILE))?;
            t.insert(RESTORE.into(), Value::String(RESTORE_FILE.into()));
        }
        Ok(t)
    }

    fn plan(&self, sys: &dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        if let Some((path, src)) = restore(sys, section, ctx)? {
            let same = files::same_content(&path, &src);
            return Ok(if same {
                Vec::new()
            } else {
                vec![Change {
                    key: "settings.json".into(),
                    from: None,
                    to: "restored".into(),
                }]
            });
        }
        let Some(path) = settings_path(sys) else {
            crate::ui::warn(format_args!(
                "Windows Terminal settings not found, terminal skipped"
            ));
            return Ok(Vec::new());
        };
        Ok(compute(&load(&path)?, section, ctx)?.1)
    }

    fn apply(&self, sys: &mut dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        if let Some((path, src)) = restore(sys, section, ctx)? {
            let changes = self.plan(sys, section, ctx)?;
            if !changes.is_empty() {
                fs::copy(&src, &path)?;
            }
            return Ok(changes);
        }
        let Some(path) = settings_path(sys) else {
            crate::ui::warn(format_args!(
                "Windows Terminal settings not found, terminal skipped"
            ));
            return Ok(Vec::new());
        };
        let (doc, changes, copies) = compute(&load(&path)?, section, ctx)?;
        if changes.is_empty() {
            return Ok(changes);
        }
        for src in copies {
            files::stable_copy(&src, &ctx.data_dir.join("terminal"))?;
        }
        write_settings(&path, &doc)?;
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::mock::MockSystem;

    const SAMPLE: &str = r##"{
        // user comment
        "$schema": "https://aka.ms/terminal-profiles-schema",
        "defaultProfile": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
        "theme": "dark",
        "profiles": {
            "defaults": { "font": { "face": "Cascadia Mono", "size": 12 }, },
            "list": [
                { "guid": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}", "name": "PowerShell",
                  "colorScheme": "Night /* not a comment */", "opacity": 80 },
                { "guid": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}", "name": "cmd" },
            ],
        },
        /* block
           comment */
        "schemes": [ { "name": "Night /* not a comment */", "background": "#101010" } ],
    }"##;

    fn setup_env(name: &str) -> (PathBuf, MockSystem, PathBuf) {
        let root = std::env::temp_dir().join(format!("winrice-term-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = root.join("Microsoft").join("Windows Terminal");
        fs::create_dir_all(&dir).unwrap();
        let settings = dir.join("settings.json");
        fs::write(&settings, SAMPLE).unwrap();
        let sys = MockSystem::default().env("LOCALAPPDATA", root.to_string_lossy());
        (root, sys, settings)
    }

    #[test]
    fn rejects_background_outside_the_setup() {
        let ctx = Ctx {
            assets: "assets".into(),
            data_dir: "data".into(),
        };
        for bad in [
            r"\\evil\share\bg.png",
            r"C:\pics\bg.png",
            "ms-appdata:///local/bg.png",
        ] {
            assert!(background("backgroundImage", &Value::String(bad.into()), &ctx).is_err());
        }
        let wallpaper = Value::String("desktopWallpaper".into());
        assert!(
            background("backgroundImage", &wallpaper, &ctx)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn strips_comments_and_trailing_commas() {
        let doc: J = serde_json::from_str(&strip_jsonc(SAMPLE)).unwrap();
        assert_eq!(doc["schemes"][0]["name"], "Night /* not a comment */");
        assert_eq!(doc["profiles"]["list"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn captures_effective_look() {
        let (root, sys, _) = setup_env("capture");
        let ctx = Ctx {
            assets: root.join("assets"),
            data_dir: root.join("data"),
        };
        let t = Terminal.capture(&sys, &ctx).unwrap();
        assert_eq!(t["theme"].as_str(), Some("dark"));
        let d = t["defaults"].as_table().unwrap();
        assert_eq!(d["colorScheme"].as_str(), Some("Night /* not a comment */"));
        assert_eq!(d["opacity"].as_integer(), Some(80));
        assert_eq!(d["font"]["face"].as_str(), Some("Cascadia Mono"));
        assert_eq!(t["schemes"].as_array().unwrap().len(), 1);
        let plan = Terminal.plan(&sys, &t, &ctx).unwrap();
        assert!(plan.iter().all(|c| c.key.starts_with("defaults.")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn applies_and_overrides_profiles() {
        let (root, mut sys, settings) = setup_env("apply");
        let ctx = Ctx {
            assets: root.join("assets"),
            data_dir: root.join("data"),
        };
        let section: Table = toml::from_str(
            r##"
            theme = "light"
            [defaults]
            colorScheme = "Tokyo"
            [[schemes]]
            name = "Tokyo"
            background = "#1a1b26"
            "##,
        )
        .unwrap();
        let changes = Terminal.apply(&mut sys, &section, &ctx).unwrap();
        let keys: Vec<_> = changes.iter().map(|c| c.key.as_str()).collect();
        assert!(keys.contains(&"theme"));
        assert!(keys.contains(&"profiles[PowerShell].colorScheme"));
        assert!(keys.contains(&"schemes[Tokyo]"));

        let doc = load(&settings).unwrap();
        assert_eq!(doc["profiles"]["defaults"]["colorScheme"], "Tokyo");
        assert_eq!(doc["profiles"]["list"][0]["colorScheme"], "Tokyo");
        assert!(doc["profiles"]["list"][1].get("colorScheme").is_none());
        assert_eq!(doc["schemes"].as_array().unwrap().len(), 2);
        assert!(Terminal.plan(&sys, &section, &ctx).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backup_restores_file_exactly() {
        let (root, mut sys, settings) = setup_env("restore");
        let ctx = Ctx {
            assets: root.join("backup"),
            data_dir: root.join("data"),
        };
        let backup = Terminal.backup(&sys, &Table::new(), &ctx).unwrap();
        let section: Table = toml::from_str("[defaults]\ncolorScheme = \"Tokyo\"").unwrap();
        let apply_ctx = Ctx {
            assets: root.join("assets"),
            data_dir: root.join("data"),
        };
        Terminal.apply(&mut sys, &section, &apply_ctx).unwrap();
        assert_ne!(fs::read_to_string(&settings).unwrap(), SAMPLE);
        assert_eq!(Terminal.apply(&mut sys, &backup, &ctx).unwrap().len(), 1);
        assert_eq!(fs::read_to_string(&settings).unwrap(), SAMPLE);
        fs::remove_dir_all(root).unwrap();
    }
}
