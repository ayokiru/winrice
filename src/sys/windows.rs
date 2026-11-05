use super::{RegValue, System};
use anyhow::{Context, Result, bail};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SPI_GETDESKWALLPAPER, SPI_SETDESKWALLPAPER, SPIF_SENDCHANGE,
    SPIF_UPDATEINIFILE, SendMessageTimeoutW, SystemParametersInfoW, WM_SETTINGCHANGE,
};
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

pub struct WinSystem {
    hkcu: RegKey,
}

impl WinSystem {
    pub fn new() -> Self {
        Self {
            hkcu: RegKey::predef(HKEY_CURRENT_USER),
        }
    }
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

impl System for WinSystem {
    fn reg_get(&self, path: &str, name: &str) -> Result<Option<RegValue>> {
        let Ok(key) = self.hkcu.open_subkey(path) else {
            return Ok(None);
        };
        if let Ok(v) = key.get_value::<u32, _>(name) {
            return Ok(Some(RegValue::Dword(v)));
        }
        Ok(key.get_value::<String, _>(name).ok().map(RegValue::Str))
    }

    fn reg_set(&mut self, path: &str, name: &str, value: &RegValue) -> Result<()> {
        let (key, _) = self
            .hkcu
            .create_subkey(path)
            .with_context(|| format!("cannot open HKCU\\{path}"))?;
        match value {
            RegValue::Dword(v) => key.set_value(name, v),
            RegValue::Str(v) => key.set_value(name, v),
        }
        .with_context(|| format!("cannot write HKCU\\{path}\\{name}"))
    }

    fn wallpaper(&self) -> Result<Option<PathBuf>> {
        let mut buf = [0u16; 1024];
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETDESKWALLPAPER,
                buf.len() as u32,
                buf.as_mut_ptr().cast(),
                0,
            )
        };
        if ok == 0 {
            bail!("SPI_GETDESKWALLPAPER failed");
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let path = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
        if path.as_os_str().is_empty() {
            return Ok(None);
        }
        if path.exists() {
            return Ok(Some(path));
        }
        // The original file may be gone; Windows keeps a transcoded copy.
        let transcoded = std::env::var_os("APPDATA")
            .map(|d| PathBuf::from(d).join(r"Microsoft\Windows\Themes\TranscodedWallpaper"));
        Ok(transcoded.filter(|p| p.exists()))
    }

    fn set_wallpaper(&mut self, path: &Path) -> Result<()> {
        let mut w = wide(path.as_os_str());
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_SETDESKWALLPAPER,
                0,
                w.as_mut_ptr().cast(),
                SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
            )
        };
        if ok == 0 {
            bail!("cannot set wallpaper {}", path.display());
        }
        Ok(())
    }

    fn notify_changed(&mut self) -> Result<()> {
        for area in [Some("ImmersiveColorSet"), Some("TraySettings"), None] {
            let w = area.map(|a| wide(OsStr::new(a)));
            let lparam = w.as_ref().map_or(0, |w| w.as_ptr() as isize);
            unsafe {
                SendMessageTimeoutW(
                    HWND_BROADCAST,
                    WM_SETTINGCHANGE,
                    0,
                    lparam,
                    SMTO_ABORTIFHUNG,
                    1000,
                    std::ptr::null_mut(),
                );
            }
        }
        Ok(())
    }
}
