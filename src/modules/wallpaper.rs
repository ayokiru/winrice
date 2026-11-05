use super::{Change, Ctx, Module, files};
use crate::sys::{RegValue, System};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

const DESKTOP: &str = r"Control Panel\Desktop";

// (style, WallpaperStyle, TileWallpaper)
const STYLES: &[(&str, &str, &str)] = &[
    ("fill", "10", "0"),
    ("fit", "6", "0"),
    ("stretch", "2", "0"),
    ("tile", "0", "1"),
    ("center", "0", "0"),
    ("span", "22", "0"),
];

pub struct Wallpaper;

fn reg_str(sys: &dyn System, name: &str) -> Result<Option<String>> {
    Ok(match sys.reg_get(DESKTOP, name)? {
        Some(RegValue::Str(s)) => Some(s),
        Some(RegValue::Dword(n)) => Some(n.to_string()),
        None => None,
    })
}

fn current_style(sys: &dyn System) -> Result<Option<&'static str>> {
    let style = reg_str(sys, "WallpaperStyle")?.unwrap_or_default();
    let tile = reg_str(sys, "TileWallpaper")?.unwrap_or_else(|| "0".into());
    Ok(STYLES
        .iter()
        .find(|(_, s, t)| *s == style && *t == tile)
        .map(|(n, ..)| *n))
}

fn ext(path: &Path) -> String {
    // TranscodedWallpaper has no extension and is a JPEG.
    files::ext(path, "jpg")
}

fn asset_path(section: &Table, ctx: &Ctx) -> Result<Option<PathBuf>> {
    match section.get("file") {
        Some(Value::String(f)) => {
            let p = ctx.assets.join(f);
            if !p.is_file() {
                bail!("wallpaper file {} not found", p.display());
            }
            Ok(Some(p))
        }
        Some(v) => bail!("wallpaper.file: expected a file name, got {v}"),
        None => Ok(None),
    }
}

impl Module for Wallpaper {
    fn name(&self) -> &'static str {
        "wallpaper"
    }

    fn description(&self) -> &'static str {
        "Desktop wallpaper image and fit style"
    }

    fn capture(&self, sys: &dyn System, ctx: &Ctx) -> Result<Table> {
        let mut t = Table::new();
        if let Some(src) = sys.wallpaper()? {
            let file = format!("wallpaper.{}", ext(&src));
            fs::create_dir_all(&ctx.assets)?;
            fs::copy(&src, ctx.assets.join(&file))
                .with_context(|| format!("cannot copy wallpaper {}", src.display()))?;
            t.insert("file".into(), Value::String(file));
        }
        if let Some(style) = current_style(sys)? {
            t.insert("style".into(), Value::String(style.into()));
        }
        Ok(t)
    }

    fn plan(&self, sys: &dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        let mut changes = Vec::new();
        if let Some(asset) = asset_path(section, ctx)? {
            let current = sys.wallpaper()?;
            let same = current
                .as_ref()
                .is_some_and(|c| files::same_content(c, &asset));
            if !same {
                changes.push(Change {
                    key: "file".into(),
                    from: current.map(|c| c.display().to_string()),
                    to: section["file"].to_string(),
                });
            }
        }
        if let Some(v) = section.get("style") {
            let Some(style) = v.as_str().filter(|s| STYLES.iter().any(|(n, ..)| n == s)) else {
                bail!("wallpaper.style: unknown value {v}");
            };
            let current = current_style(sys)?;
            if current != Some(style) {
                changes.push(Change {
                    key: "style".into(),
                    from: current.map(|s| format!("\"{s}\"")),
                    to: v.to_string(),
                });
            }
        }
        Ok(changes)
    }

    fn apply(&self, sys: &mut dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        let changes = self.plan(sys, section, ctx)?;
        if changes.is_empty() {
            return Ok(changes);
        }
        if let Some(style) = section.get("style").and_then(Value::as_str) {
            let (_, s, t) = STYLES.iter().find(|(n, ..)| *n == style).unwrap();
            sys.reg_set(DESKTOP, "WallpaperStyle", &RegValue::Str(s.to_string()))?;
            sys.reg_set(DESKTOP, "TileWallpaper", &RegValue::Str(t.to_string()))?;
        }
        // Setting the image (even the same one) makes Windows pick up the new style.
        let image = match asset_path(section, ctx)? {
            // Keep a stable copy so the setup folder can be deleted later.
            Some(asset) => Some(files::stable_copy(
                &asset,
                &ctx.data_dir.join("wallpapers"),
            )?),
            None => sys.wallpaper()?,
        };
        if let Some(image) = image {
            sys.set_wallpaper(&image)?;
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::mock::MockSystem;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("winrice-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn capture_then_apply_roundtrip() {
        let root = temp("wallpaper");
        let src = root.join("pic.PNG");
        fs::write(&src, b"image-a").unwrap();
        let mut sys = MockSystem::default()
            .with(DESKTOP, "WallpaperStyle", RegValue::Str("6".into()))
            .with(DESKTOP, "TileWallpaper", RegValue::Str("0".into()));
        sys.wallpaper = Some(src);
        let ctx = Ctx {
            assets: root.join("assets"),
            data_dir: root.join("data"),
        };

        let section = Wallpaper.capture(&sys, &ctx).unwrap();
        assert_eq!(section["file"].as_str(), Some("wallpaper.png"));
        assert_eq!(section["style"].as_str(), Some("fit"));
        assert!(Wallpaper.plan(&sys, &section, &ctx).unwrap().is_empty());

        let mut other = MockSystem::default();
        let changes = Wallpaper.apply(&mut other, &section, &ctx).unwrap();
        assert_eq!(changes.len(), 2);
        let applied = other.wallpaper.clone().unwrap();
        assert!(applied.starts_with(root.join("data")));
        assert_eq!(fs::read(applied).unwrap(), b"image-a");
        assert!(Wallpaper.plan(&other, &section, &ctx).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
