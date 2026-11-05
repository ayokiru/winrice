//! Accent color: Explorer\Accent palette plus the DWM colorization values.
//!
//! The palette has 8 RGBA colors: light3, light2, light1, base, dark1, dark2, dark3, extra.
//! Captured palettes are applied as is; a bare `color` gets an approximated palette.

use super::{Change, Ctx, Module};
use crate::sys::{RegValue, System};
use anyhow::{Result, bail};
use toml::{Table, Value};

const ACCENT: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent";
const DWM: &str = r"Software\Microsoft\Windows\DWM";
const DESKTOP: &str = r"Control Panel\Desktop";

type Rgb = [u8; 3];

pub struct Accent;

fn parse_hex(s: &str) -> Result<Rgb> {
    let h = s.trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("accent: \"{s}\" is not a #RRGGBB color");
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap();
    Ok([byte(0), byte(2), byte(4)])
}

fn hex(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn abgr(c: Rgb, alpha: u8) -> u32 {
    u32::from_le_bytes([c[0], c[1], c[2], alpha])
}

fn argb(c: Rgb, alpha: u8) -> u32 {
    u32::from_be_bytes([alpha, c[0], c[1], c[2]])
}

fn mix(c: Rgb, to: u8, t: f32) -> Rgb {
    c.map(|v| (v as f32 + (to as f32 - v as f32) * t).round() as u8)
}

/// Approximates the palette Windows derives from a single accent color.
fn generate(base: Rgb) -> Vec<Rgb> {
    vec![
        mix(base, 255, 0.7),
        mix(base, 255, 0.45),
        mix(base, 255, 0.2),
        base,
        mix(base, 0, 0.15),
        mix(base, 0, 0.4),
        mix(base, 0, 0.65),
        base,
    ]
}

fn current_palette(sys: &dyn System) -> Result<Option<Vec<Rgb>>> {
    Ok(match sys.reg_get(ACCENT, "AccentPalette")? {
        Some(RegValue::Binary(b)) if b.len() == 32 => {
            Some(b.chunks(4).map(|c| [c[0], c[1], c[2]]).collect())
        }
        _ => None,
    })
}

fn current_auto(sys: &dyn System) -> Result<Option<bool>> {
    Ok(match sys.reg_get(DESKTOP, "AutoColorization")? {
        Some(RegValue::Dword(n)) => Some(n != 0),
        Some(RegValue::Str(s)) => Some(s.trim() == "1"),
        _ => None,
    })
}

fn target_palette(section: &Table) -> Result<Option<Vec<Rgb>>> {
    if let Some(p) = section.get("palette") {
        let Some(list) = p.as_array().filter(|l| l.len() == 8) else {
            bail!("accent.palette must be a list of 8 colors");
        };
        let colors = list
            .iter()
            .map(|v| parse_hex(v.as_str().unwrap_or_default()))
            .collect::<Result<Vec<_>>>()?;
        return Ok(Some(colors));
    }
    match section.get("color") {
        Some(Value::String(c)) => Ok(Some(generate(parse_hex(c)?))),
        Some(v) => bail!("accent.color: expected \"#RRGGBB\", got {v}"),
        None => Ok(None),
    }
}

impl Module for Accent {
    fn name(&self) -> &'static str {
        "accent"
    }

    fn description(&self) -> &'static str {
        "Accent color and its palette, or accent from wallpaper"
    }

    fn capture(&self, sys: &dyn System, _ctx: &Ctx) -> Result<Table> {
        let mut t = Table::new();
        if let Some(auto) = current_auto(sys)? {
            t.insert("auto".into(), Value::Boolean(auto));
        }
        if let Some(p) = current_palette(sys)? {
            t.insert("color".into(), Value::String(hex(p[3])));
            let list = p.iter().map(|c| Value::String(hex(*c))).collect();
            t.insert("palette".into(), Value::Array(list));
        }
        Ok(t)
    }

    fn plan(&self, sys: &dyn System, section: &Table, _ctx: &Ctx) -> Result<Vec<Change>> {
        let mut changes = Vec::new();
        let auto = match section.get("auto") {
            Some(Value::Boolean(b)) => Some(*b),
            Some(v) => bail!("accent.auto: expected true or false, got {v}"),
            None => None,
        };
        if let Some(auto) = auto {
            let cur = current_auto(sys)?;
            if cur != Some(auto) {
                changes.push(Change {
                    key: "auto".into(),
                    from: cur.map(|c| c.to_string()),
                    to: auto.to_string(),
                });
            }
        }
        // With auto on, Windows picks the color from the wallpaper.
        if auto == Some(true) {
            return Ok(changes);
        }
        if let Some(target) = target_palette(section)? {
            let cur = current_palette(sys)?;
            if cur.as_ref() != Some(&target) {
                changes.push(Change {
                    key: "color".into(),
                    from: cur.map(|p| format!("\"{}\"", hex(p[3]))),
                    to: format!("\"{}\"", hex(target[3])),
                });
            }
        }
        Ok(changes)
    }

    fn apply(&self, sys: &mut dyn System, section: &Table, ctx: &Ctx) -> Result<Vec<Change>> {
        let changes = self.plan(sys, section, ctx)?;
        for c in &changes {
            match c.key.as_str() {
                "auto" => {
                    let on = section["auto"].as_bool() == Some(true);
                    sys.reg_set(DESKTOP, "AutoColorization", &RegValue::Dword(on.into()))?;
                }
                _ => {
                    let p = target_palette(section)?.unwrap();
                    let bytes = p.iter().flat_map(|c| [c[0], c[1], c[2], 0]).collect();
                    sys.reg_set(ACCENT, "AccentPalette", &RegValue::Binary(bytes))?;
                    // Checked on Windows 11: menu and DWM accent = base, Start = dark1.
                    sys.reg_set(
                        ACCENT,
                        "AccentColorMenu",
                        &RegValue::Dword(abgr(p[3], 0xFF)),
                    )?;
                    sys.reg_set(ACCENT, "StartColorMenu", &RegValue::Dword(abgr(p[4], 0xFF)))?;
                    sys.reg_set(DWM, "AccentColor", &RegValue::Dword(abgr(p[3], 0xFF)))?;
                    let colorization = RegValue::Dword(argb(p[3], 0xC4));
                    sys.reg_set(DWM, "ColorizationColor", &colorization)?;
                    sys.reg_set(DWM, "ColorizationAfterglow", &colorization)?;
                    // Only turn off accent-from-wallpaper; a missing value already means off.
                    if section.get("auto").is_none() && current_auto(sys)? == Some(true) {
                        sys.reg_set(DESKTOP, "AutoColorization", &RegValue::Dword(0))?;
                    }
                }
            }
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::mock::MockSystem;

    // Windows default blue palette.
    const DEFAULT: [u8; 32] = [
        0x99, 0xEB, 0xFF, 0, 0x4C, 0xC2, 0xFF, 0, 0x00, 0x91, 0xF8, 0, 0x00, 0x78, 0xD4, 0, 0x00,
        0x67, 0xC0, 0, 0x00, 0x3E, 0x92, 0, 0x00, 0x1A, 0x68, 0, 0xF7, 0x63, 0x0C, 0,
    ];

    fn ctx() -> Ctx {
        Ctx {
            assets: "a".into(),
            data_dir: "d".into(),
        }
    }

    #[test]
    fn capture_and_apply_exact_palette() {
        let source = MockSystem::default()
            .with(ACCENT, "AccentPalette", RegValue::Binary(DEFAULT.to_vec()))
            .with(DESKTOP, "AutoColorization", RegValue::Dword(0));
        let section = Accent.capture(&source, &ctx()).unwrap();
        assert_eq!(section["color"].as_str(), Some("#0078D4"));
        assert_eq!(section["palette"].as_array().unwrap().len(), 8);

        let mut target = MockSystem::default();
        let changes = Accent.apply(&mut target, &section, &ctx()).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(
            target.reg_get(ACCENT, "AccentPalette").unwrap(),
            Some(RegValue::Binary(DEFAULT.to_vec()))
        );
        assert_eq!(
            target.reg_get(ACCENT, "AccentColorMenu").unwrap(),
            Some(RegValue::Dword(0xFFD47800))
        );
        assert_eq!(
            target.reg_get(ACCENT, "StartColorMenu").unwrap(),
            Some(RegValue::Dword(0xFFC06700))
        );
        assert_eq!(
            target.reg_get(DWM, "AccentColor").unwrap(),
            Some(RegValue::Dword(0xFFD47800))
        );
        assert_eq!(
            target.reg_get(DWM, "ColorizationColor").unwrap(),
            Some(RegValue::Dword(0xC40078D4))
        );
        assert!(Accent.plan(&target, &section, &ctx()).unwrap().is_empty());
    }

    #[test]
    fn bare_color_generates_palette() {
        let section: Table = toml::from_str("color = \"#e81123\"").unwrap();
        let mut sys = MockSystem::default().with(DESKTOP, "AutoColorization", RegValue::Dword(1));
        Accent.apply(&mut sys, &section, &ctx()).unwrap();
        let Some(RegValue::Binary(b)) = sys.reg_get(ACCENT, "AccentPalette").unwrap() else {
            panic!("no palette");
        };
        assert_eq!(&b[12..15], &[0xE8, 0x11, 0x23]);
        assert_eq!(
            sys.reg_get(DESKTOP, "AutoColorization").unwrap(),
            Some(RegValue::Dword(0))
        );
    }

    #[test]
    fn auto_skips_colors() {
        let section: Table = toml::from_str("auto = true\ncolor = \"#e81123\"").unwrap();
        let changes = Accent
            .plan(&MockSystem::default(), &section, &ctx())
            .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].key, "auto");
        assert!(
            toml::from_str::<Table>("color = \"red\"")
                .map(|t| target_palette(&t).is_err())
                .unwrap()
        );
    }

    #[test]
    fn missing_auto_colorization_stays_missing() {
        let section: Table = toml::from_str("color = \"#e81123\"").unwrap();
        let mut sys = MockSystem::default();
        Accent.apply(&mut sys, &section, &ctx()).unwrap();
        assert_eq!(sys.reg_get(DESKTOP, "AutoColorization").unwrap(), None);
    }
}
