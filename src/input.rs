// SPDX-License-Identifier: GPL-3.0-or-later

//! Where skins come from: files, folders of files, and http(s) addresses.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// The most read from an address.
const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;

/// How long a download may take in all.
const TIMEOUT: Duration = Duration::from_secs(60);

/// The file name endings a folder is searched for.
const SKIN_ENDINGS: [&str; 3] = ["wsz", "wal", "zip"];

/// One skin to render, or a base to lay under skins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    File(PathBuf),
    Url(String),
}

impl Source {
    /// A path, or an address when it starts with `http://` or `https://`.
    pub fn parse(text: &str) -> Source {
        let lower = text.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            Source::Url(text.to_string())
        } else {
            Source::File(PathBuf::from(text))
        }
    }

    /// The skin's name: the file name without its ending. Scripts know the skin by it, and
    /// the preview is named after it.
    pub fn name(&self) -> String {
        let file = match self {
            Source::File(path) => path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            Source::Url(url) => {
                let path = url.split(['?', '#']).next().unwrap_or_default();
                percent_decode(path.rsplit('/').next().unwrap_or_default())
            }
        };
        let stem = match file.rsplit_once('.') {
            Some((stem, _)) if !stem.is_empty() => stem.to_string(),
            _ => file,
        };
        if stem.is_empty() {
            "skin".to_string()
        } else {
            stem
        }
    }

    /// The source's bytes.
    pub fn read(&self) -> Result<Vec<u8>, String> {
        match self {
            Source::File(path) => std::fs::read(path).map_err(|e| e.to_string()),
            Source::Url(url) => download(url),
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::File(path) => write!(f, "{}", path.display()),
            Source::Url(url) => write!(f, "{url}"),
        }
    }
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(concat!("walkit-cli/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

/// `%41` and the like as the bytes they stand for.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The skins `inputs` name, in order: a file or address as it is, a folder as the skin files
/// in it by name (and in the folders under it when `recursive`).
pub fn expand(inputs: &[String], recursive: bool) -> Result<Vec<Source>, String> {
    let mut sources = Vec::new();
    for input in inputs {
        match Source::parse(input) {
            Source::File(path) if path.is_dir() => folder(&path, recursive, &mut sources)?,
            source => sources.push(source),
        }
    }
    Ok(sources)
}

fn folder(dir: &Path, recursive: bool, sources: &mut Vec<Source>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            if recursive {
                folder(&path, recursive, sources)?;
            }
        } else if path.extension().is_some_and(|e| {
            SKIN_ENDINGS
                .iter()
                .any(|ending| e.eq_ignore_ascii_case(ending))
        }) {
            sources.push(Source::File(path));
        }
    }
    Ok(())
}

/// The inputs a list file names: one per line; empty lines and lines starting with `#` are
/// skipped.
pub fn list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_is_named_after_its_file() {
        assert_eq!(Source::parse("skins/Blue Steel.wsz").name(), "Blue Steel");
        assert_eq!(
            Source::parse("https://example.org/skins/Blue%20Steel.wsz?download=1").name(),
            "Blue Steel"
        );
        assert_eq!(Source::parse("https://example.org/").name(), "skin");
        assert_eq!(Source::parse(".wsz").name(), ".wsz");
    }

    #[test]
    fn a_list_skips_comments_and_empty_lines() {
        let text = "# skins\n\n a.wsz \nhttps://example.org/b.wal\n";
        assert_eq!(list(text), ["a.wsz", "https://example.org/b.wal"]);
    }
}
