//! Fonts used by the setup (currently the Windows Terminal font).
//! Installed per user, no admin rights needed.

use super::{Change, Ctx, Module, files, terminal};
use crate::sys::{RegValue, System};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

const FONTS_KEY: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Fonts";

/// Fonts that ship with Windows or Windows Terminal.
const BUILTIN: &[&str] = &[
    "Cascadia Code",
    "Cascadia Mono",
    "Consolas",
    "Courier New",
    "Lucida Console",
    "Segoe UI",
    "Segoe UI Emoji",
    "Segoe UI Symbol",
    "Segoe Fluent Icons",
];

const STYLES: &[&str] = &[
    "regular",
    "bold",
    "italic",
    "oblique",
    "thin",
    "light",
    "extralight",
    "semilight",
    "medium",
    "semibold",
    "extrabold",
    "black",
    "heavy",
    "book",
    "normal",
];

pub struct Fonts;

/// Registry entry names look like "Fira Code Bold & Fira Code Light (TrueType)".
fn entry_matches(entry: &str, face: &str) -> bool {
    let entry = match entry.rfind(" (") {
        Some(i) => &entry[..i],
        None => entry,
    };
    entry.split(" & ").any(|part| {
        let part = part.trim();
        part.len() >= face.len()
            && part.is_char_boundary(face.len())
            && part[..face.len()].eq_ignore_ascii_case(face)
            && part[face.len()..]
                .split_whitespace()
                .all(|w| STYLES.contains(&w.to_ascii_lowercase().as_str()))
    })
}

fn system_fonts(sys: &dyn System) -> PathBuf {
    let root = sys
        .env("SystemRoot")
        .unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(root).join("Fonts")
}

fn user_fonts(sys: &dyn System) -> Option<PathBuf> {
    let base = sys.env("LOCALAPPDATA")?;
    Some(
        PathBuf::from(base)
            .join("Microsoft")
            .join("Windows")
            .join("Fonts"),
    )
}

/// Installed font files for a face, from both user and machine registrations.
fn font_files(sys: &dyn System, face: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for machine in [false, true] {
        for (name, file) in sys.reg_list(machine, FONTS_KEY)? {
            if !entry_matches(&name, face) {
                continue;
            }
            let path = if file.contains('\\') || file.contains(':') {
                PathBuf::from(files::expand(sys, &file))
            } else {
                system_fonts(sys).join(&file)
            };
            if path.is_file() && !out.contains(&path) {
                out.push(path);
            }
        }
    }
    Ok(out)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_installed(sys: &dyn System, asset: &Path) -> bool {
    let name = file_name(asset);
    let user = user_fonts(sys).map(|d| d.join(&name));
    user.is_some_and(|p| files::same_content(&p, asset)) || system_fonts(sys).join(&name).is_file()
}

fn assets(section: &Table, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let Some(list) = section.get("files") else {
        return Ok(Vec::new());
    };
    let Some(list) = list.as_array() else {
        bail!("fonts.files must be an array");
    };
    list.iter()
        .map(|v| {
            let Some(rel) = v.as_str() else {
                bail!("fonts.files: expected a file name, got {v}");
            };
            files::asset(ctx, rel).context("fonts.files")
        })
        .collect()
}

/// Setup fonts that are not installed yet.
fn missing(sys: &dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let mut list = assets(section, ctx)?;
    list.retain(|a| !is_installed(sys, a));
    Ok(list)
}

impl Module for Fonts {
    fn name(&self) -> &'static str {
        "fonts"
    }

    fn description(&self) -> &'static str {
        "Fonts used by the setup (Terminal font), installed per user"
    }

    fn capture(&self, sys: &dyn System, ctx: &Ctx) -> Result<Table> {
        let mut list = Vec::new();
        for face in terminal::font_faces(sys) {
            if BUILTIN.iter().any(|b| b.eq_ignore_ascii_case(&face)) {
                continue;
            }
            let found = font_files(sys, &face)?;
            if found.is_empty() {
                crate::ui::warn(format_args!(
                    "font \"{face}\" not found, it will not be bundled"
                ));
            }
            for src in found {
                let rel = format!("fonts/{}", file_name(&src));
                fs::create_dir_all(ctx.assets.join("fonts"))?;
                fs::copy(&src, ctx.assets.join(&rel))?;
                list.push(Value::String(rel));
            }
        }
        let mut t = Table::new();
        if !list.is_empty() {
            crate::ui::note(format_args!(
                "check font licenses before sharing a setup publicly"
            ));
            t.insert("files".into(), Value::Array(list));
        }
        Ok(t)
    }

    /// Rollback never uninstalls fonts, so copying them into every backup is wasted space.
    fn backup(&self, _sys: &dyn System, _ctx: &Ctx) -> Result<Table> {
        Ok(Table::new())
    }

    fn plan(&self, sys: &dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        Ok(missing(sys, section, ctx)?
            .iter()
            .map(|a| Change {
                key: file_name(a),
                from: None,
                to: "installed".into(),
            })
            .collect())
    }

    fn apply(&self, sys: &mut dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        let changes = self.plan(sys, section, ctx)?;
        if changes.is_empty() {
            return Ok(changes);
        }
        let Some(dir) = user_fonts(sys) else {
            bail!("LOCALAPPDATA is not set");
        };
        fs::create_dir_all(&dir)?;
        for src in missing(sys, section, ctx)? {
            let name = file_name(&src);
            let dest = dir.join(&name);
            fs::copy(&src, &dest)?;
            let stem = dest.file_stem().unwrap_or_default().to_string_lossy();
            let kind = if name.to_ascii_lowercase().ends_with(".otf") {
                "OpenType"
            } else {
                "TrueType"
            };
            let value = RegValue::Str(dest.to_string_lossy().into_owned());
            sys.reg_set(FONTS_KEY, &format!("{stem} ({kind})"), &value)?;
            sys.load_font(&dest)?;
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::mock::MockSystem;

    #[test]
    fn installs_fonts_listed_outside_fonts_folder() {
        let root = std::env::temp_dir().join(format!("winrice-fonts-root-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(root.join("assets/Hand.ttf"), b"font").unwrap();
        let ctx = Ctx {
            assets: root.join("assets"),
            data_dir: root.join("data"),
        };
        let section: Table = toml::from_str("files = [\"Hand.ttf\"]").unwrap();
        let mut sys =
            MockSystem::default().env("LOCALAPPDATA", root.join("local").to_string_lossy());
        assert_eq!(Fonts.apply(&mut sys, &section, &ctx).unwrap().len(), 1);
        assert!(
            root.join("local/Microsoft/Windows/Fonts/Hand.ttf")
                .is_file()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn matches_styles_only() {
        assert!(entry_matches("Fira Code Regular (TrueType)", "Fira Code"));
        assert!(entry_matches(
            "Fira Code Light & Fira Code Bold (TrueType)",
            "fira code"
        ));
        assert!(entry_matches("JetBrains Mono (TrueType)", "JetBrains Mono"));
        assert!(!entry_matches("Fira Code Retina (TrueType)", "Fira Code"));
        assert!(!entry_matches(
            "CaskaydiaCove Nerd Font Mono Regular",
            "CaskaydiaCove Nerd Font"
        ));
    }

    #[test]
    fn bundles_terminal_font_and_installs_it() {
        let root = std::env::temp_dir().join(format!("winrice-fonts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let term = root.join("Microsoft").join("Windows Terminal");
        fs::create_dir_all(&term).unwrap();
        fs::write(
            term.join("settings.json"),
            r#"{ "profiles": { "defaults": { "font": { "face": "Fira Code, Consolas" } }, "list": [] } }"#,
        )
        .unwrap();
        let src_dir = root.join("src-fonts");
        fs::create_dir_all(&src_dir).unwrap();
        let ttf = src_dir.join("FiraCode-Regular.ttf");
        fs::write(&ttf, b"font-bytes").unwrap();

        let local = root.to_string_lossy().into_owned();
        let source = MockSystem::default()
            .env("LOCALAPPDATA", &local)
            .env("SystemRoot", root.join("win").to_string_lossy())
            .with(
                FONTS_KEY,
                "Fira Code Regular (TrueType)",
                RegValue::Str(ttf.to_string_lossy().into_owned()),
            );
        let ctx = Ctx {
            assets: root.join("assets"),
            data_dir: root.join("data"),
        };
        let section = Fonts.capture(&source, &ctx).unwrap();
        let files: Vec<_> = section["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(files, ["fonts/FiraCode-Regular.ttf"]);

        let target_local = root.join("target");
        let mut target = MockSystem::default()
            .env("LOCALAPPDATA", target_local.to_string_lossy())
            .env("SystemRoot", root.join("win").to_string_lossy());
        let changes = Fonts.apply(&mut target, &section, &ctx).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(target.fonts.len(), 1);
        assert!(
            target_local
                .join("Microsoft/Windows/Fonts/FiraCode-Regular.ttf")
                .is_file()
        );
        assert!(
            target
                .reg_get(FONTS_KEY, "FiraCode-Regular (TrueType)")
                .unwrap()
                .is_some()
        );
        assert!(Fonts.plan(&target, &section, &ctx).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
