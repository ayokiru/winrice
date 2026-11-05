//! Where a setup comes from: a local folder or a GitHub repository.

use crate::setup;
use anyhow::{Context, Result, bail};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

const MAX_DOWNLOAD: u64 = 100 * 1024 * 1024;
const MAX_UNPACKED: u64 = 300 * 1024 * 1024;

#[derive(Debug, PartialEq)]
pub struct Remote {
    pub owner: String,
    pub repo: String,
    pub path: Option<String>,
    pub reference: String,
}

impl Remote {
    pub fn label(&self) -> String {
        let mut s = format!("github.com/{}/{}", self.owner, self.repo);
        if let Some(p) = &self.path {
            s.push('/');
            s.push_str(p);
        }
        s
    }
}

fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

/// Accepts `owner/repo[/path][@ref]` and github.com URLs (including `/tree/<ref>/path`).
pub fn parse(spec: &str) -> Option<Remote> {
    let s = spec.trim().trim_end_matches('/');
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    let s = s.strip_prefix("www.").unwrap_or(s);
    let s = s.strip_prefix("github.com/").unwrap_or(s);
    let (s, mut reference) = match s.rsplit_once('@') {
        Some((a, r)) if valid_ref(r) => (a, r.to_string()),
        Some(_) => return None,
        None => (s, "HEAD".to_string()),
    };
    let mut parts = s.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?.trim_end_matches(".git");
    if !valid_name(owner) || !valid_name(repo) {
        return None;
    }
    let mut rest: Vec<&str> = parts.collect();
    if rest.len() >= 2 && rest[0] == "tree" {
        reference = rest[1].to_string();
        rest.drain(..2);
    }
    if !rest.iter().all(|p| valid_name(p)) {
        return None;
    }
    Some(Remote {
        owner: owner.into(),
        repo: repo.into(),
        path: (!rest.is_empty()).then(|| rest.join("/")),
        reference,
    })
}

fn valid_ref(r: &str) -> bool {
    !r.is_empty()
        && !r.contains("..")
        && r.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
}

fn download(remote: &Remote) -> Result<Vec<u8>> {
    let url = format!(
        "https://github.com/{}/{}/archive/{}.zip",
        remote.owner, remote.repo, remote.reference
    );
    // System TLS (schannel on Windows): no C toolchain, uses the Windows cert store.
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(tls).build().into();
    let mut resp = match agent
        .get(&url)
        .header("User-Agent", concat!("winrice/", env!("CARGO_PKG_VERSION")))
        .call()
    {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(404)) => {
            bail!("{} not found (is the repository public?)", remote.label())
        }
        Err(e) => return Err(e).with_context(|| format!("cannot download {url}")),
    };
    resp.body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .read_to_vec()
        .context("download failed or is too large")
}

/// Unpacks a GitHub archive, dropping its top-level `<repo>-<ref>/` folder.
fn extract(bytes: &[u8], dest: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).context("invalid archive")?;
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::create_dir_all(dest)?;
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let rel: PathBuf = name
            .components()
            .skip(1)
            .filter(|c| matches!(c, Component::Normal(_)))
            .collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        total += entry.size();
        if total > MAX_UNPACKED {
            bail!("archive is too large");
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        fs::write(out, buf)?;
    }
    Ok(())
}

/// Folders (relative to `root`) that contain a setup.toml, up to 3 levels deep.
fn find_setups(root: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if dir.join(setup::FILE).is_file() {
        out.push(dir.strip_prefix(root).unwrap_or(dir).to_path_buf());
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut dirs: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for d in dirs {
        find_setups(root, &d, depth - 1, out);
    }
}

fn pick_setup(root: &Path, remote: &Remote) -> Result<PathBuf> {
    let base = match &remote.path {
        Some(p) => root.join(p),
        None => root.to_path_buf(),
    };
    if base.join(setup::FILE).is_file() {
        return Ok(base);
    }
    let mut found = Vec::new();
    find_setups(root, &base, 3, &mut found);
    match found.as_slice() {
        [] => bail!("no {} found in {}", setup::FILE, remote.label()),
        [one] => Ok(root.join(one)),
        many => {
            let list: Vec<String> = many
                .iter()
                .map(|p| {
                    let p = p.to_string_lossy().replace('\\', "/");
                    format!("  winrice apply {}/{}/{p}", remote.owner, remote.repo)
                })
                .collect();
            bail!(
                "{} has several setups, pick one:\n{}",
                remote.label(),
                list.join("\n")
            )
        }
    }
}

pub enum Source {
    Local(PathBuf),
    Remote(PathBuf, Remote),
}

impl Source {
    pub fn dir(&self) -> &Path {
        match self {
            Source::Local(p) | Source::Remote(p, _) => p,
        }
    }
}

pub fn resolve(spec: &str, data_dir: &Path) -> Result<Source> {
    let path = Path::new(spec);
    if path.exists() {
        return Ok(Source::Local(setup::resolve(path)));
    }
    let Some(remote) = parse(spec) else {
        bail!("\"{spec}\" is not a setup folder or a GitHub repository (owner/repo)");
    };
    crate::ui::step(format_args!("Downloading {}...", remote.label()));
    let bytes = download(&remote)?;
    let dest = data_dir
        .join("remote")
        .join(format!("{}_{}", remote.owner, remote.repo));
    extract(&bytes, &dest)?;
    let dir = pick_setup(&dest, &remote)?;
    Ok(Source::Remote(dir, remote))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_specs() {
        let r = parse("ayokiru/tokyo-night").unwrap();
        assert_eq!(
            (r.owner.as_str(), r.repo.as_str()),
            ("ayokiru", "tokyo-night")
        );
        assert_eq!((r.path, r.reference.as_str()), (None, "HEAD"));

        let r = parse("ayokiru/setups/minimal/dark@v2").unwrap();
        assert_eq!(r.path.as_deref(), Some("minimal/dark"));
        assert_eq!(r.reference, "v2");

        let r = parse("https://github.com/ayokiru/setups/tree/main/minimal/").unwrap();
        assert_eq!(r.path.as_deref(), Some("minimal"));
        assert_eq!(r.reference, "main");

        assert_eq!(parse("https://github.com/a/b.git").unwrap().repo, "b");
        assert!(parse("just-a-name").is_none());
        assert!(parse("a/b/../etc").is_none());
        assert!(parse("a/b@../x").is_none());
    }

    fn zip_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in files {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
        buf.into_inner()
    }

    #[test]
    fn extracts_and_picks_setup() {
        let root = std::env::temp_dir().join(format!("winrice-source-{}", std::process::id()));
        let bytes = zip_with(&[
            ("repo-main/README.md", b"hi"),
            ("repo-main/dark/setup.toml", b"[meta]\nname = \"dark\"\n"),
            ("repo-main/light/setup.toml", b"[meta]\nname = \"light\"\n"),
            ("repo-main/dark/assets/wallpaper.jpg", b"img"),
        ]);
        extract(&bytes, &root).unwrap();
        assert!(root.join("dark/assets/wallpaper.jpg").is_file());

        let mut remote = parse("a/repo").unwrap();
        let err = pick_setup(&root, &remote).unwrap_err().to_string();
        assert!(err.contains("winrice apply a/repo/dark"));
        remote.path = Some("light".into());
        assert_eq!(pick_setup(&root, &remote).unwrap(), root.join("light"));
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod network {
    use super::*;

    /// Run with `cargo test -- --ignored`: needs internet.
    #[test]
    #[ignore]
    fn downloads_public_repo() {
        let root = std::env::temp_dir().join(format!("winrice-net-{}", std::process::id()));
        let err = resolve("ayokiru/winrice", &root).err().unwrap().to_string();
        assert!(err.contains("no setup.toml found"), "{err}");
        assert!(root.join("remote/ayokiru_winrice/Cargo.toml").is_file());
        let err = resolve("ayokiru/does-not-exist-xyz", &root)
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("not found"), "{err}");
        fs::remove_dir_all(root).unwrap();
    }
}
