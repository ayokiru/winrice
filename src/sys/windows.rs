use super::{RegValue, System};
use anyhow::{Context, Result, bail};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Graphics::Gdi::AddFontResourceW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SPI_GETDESKWALLPAPER, SPI_SETCURSORS, SPI_SETDESKWALLPAPER,
    SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SendMessageTimeoutW, SystemParametersInfoW, WM_FONTCHANGE,
    WM_SETTINGCHANGE,
};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_BINARY};

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
        if let Ok(v) = key.get_value::<String, _>(name) {
            return Ok(Some(RegValue::Str(v)));
        }
        Ok(key
            .get_raw_value(name)
            .ok()
            .filter(|v| v.vtype == REG_BINARY)
            .map(|v| RegValue::Binary(v.bytes.into_owned())))
    }

    fn reg_list(&self, machine: bool, path: &str) -> Result<Vec<(String, String)>> {
        let root = RegKey::predef(if machine {
            HKEY_LOCAL_MACHINE
        } else {
            HKEY_CURRENT_USER
        });
        let Ok(key) = root.open_subkey(path) else {
            return Ok(Vec::new());
        };
        Ok(key
            .enum_values()
            .filter_map(|v| v.ok())
            .filter_map(|(name, _)| key.get_value::<String, _>(&name).ok().map(|v| (name, v)))
            .collect())
    }

    fn reg_set(&mut self, path: &str, name: &str, value: &RegValue) -> Result<()> {
        let (key, _) = self
            .hkcu
            .create_subkey(path)
            .with_context(|| format!("cannot open HKCU\\{path}"))?;
        match value {
            RegValue::Dword(v) => key.set_value(name, v),
            RegValue::Str(v) => key.set_value(name, v),
            RegValue::Binary(v) => key.set_raw_value(
                name,
                &winreg::RegValue {
                    bytes: v.as_slice().into(),
                    vtype: REG_BINARY,
                },
            ),
        }
        .with_context(|| format!("cannot write HKCU\\{path}\\{name}"))
    }

    fn reg_delete(&mut self, path: &str, name: &str) -> Result<()> {
        let Ok(key) = self.hkcu.open_subkey_with_flags(path, KEY_SET_VALUE) else {
            return Ok(());
        };
        match key.delete_value(name) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(e).with_context(|| format!("cannot delete HKCU\\{path}\\{name}"))
            }
            _ => Ok(()),
        }
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

    fn reload_cursors(&mut self) -> Result<()> {
        // CursorBaseSize is only picked up through this undocumented call (what Settings uses).
        const SPI_SETCURSORSIZE: u32 = 0x2029;
        if let Some(RegValue::Dword(size)) =
            self.reg_get(r"Control Panel\Cursors", "CursorBaseSize")?
        {
            unsafe {
                SystemParametersInfoW(
                    SPI_SETCURSORSIZE,
                    0,
                    size as usize as *mut _,
                    SPIF_UPDATEINIFILE,
                )
            };
        }
        // SPI_SETCURSORS reloads the files from the registry; it fails when asked to update the ini file.
        let ok = unsafe { SystemParametersInfoW(SPI_SETCURSORS, 0, std::ptr::null_mut(), 0) };
        if ok == 0 {
            bail!("cannot reload cursors: {}", std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn load_font(&mut self, path: &Path) -> Result<()> {
        let w = wide(path.as_os_str());
        if unsafe { AddFontResourceW(w.as_ptr()) } == 0 {
            bail!("cannot load font {}", path.display());
        }
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_FONTCHANGE,
                0,
                0,
                SMTO_ABORTIFHUNG,
                1000,
                std::ptr::null_mut(),
            );
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
