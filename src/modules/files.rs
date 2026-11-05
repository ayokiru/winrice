//! File helpers shared by modules that ship assets.

use crate::sys::System;
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// FNV-1a, only used to name applied asset copies.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}

pub fn ext(path: &Path, fallback: &str) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_ascii_lowercase(),
        None => fallback.into(),
    }
}

/// Where `asset` lives after apply, so the setup folder can be deleted later.
pub fn stable_path(asset: &Path, dir: &Path) -> Result<PathBuf> {
    let bytes = fs::read(asset)?;
    Ok(dir.join(format!("{:016x}.{}", hash(&bytes), ext(asset, "bin"))))
}

pub fn stable_copy(asset: &Path, dir: &Path) -> Result<PathBuf> {
    let dest = stable_path(asset, dir)?;
    if !dest.exists() {
        fs::create_dir_all(dir)?;
        fs::copy(asset, &dest)?;
    }
    Ok(dest)
}

pub fn same_content(a: &Path, b: &Path) -> bool {
    match (fs::read(a), fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Expands `%VAR%` using the system environment.
pub fn expand(sys: &dyn System, s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        let Some(len) = rest[start + 1..].find('%') else {
            break;
        };
        let name = &rest[start + 1..start + 1 + len];
        out.push_str(&rest[..start]);
        match sys.env(name) {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[start..start + len + 2]),
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::mock::MockSystem;

    #[test]
    fn expands_known_vars_only() {
        let sys = MockSystem::default().env("SystemRoot", r"C:\Windows");
        assert_eq!(
            expand(&sys, r"%SystemRoot%\cursors\a.cur"),
            r"C:\Windows\cursors\a.cur"
        );
        assert_eq!(expand(&sys, "%NOPE%\\x"), "%NOPE%\\x");
        assert_eq!(expand(&sys, "100%"), "100%");
    }
}
