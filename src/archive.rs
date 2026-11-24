// SPDX-License-Identifier: GPL-3.0-or-later

//! Skins as folders: an archive unpacked into a folder to edit, and a folder packed back into
//! an archive.

use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::skin::{MAX_ENTRY, MAX_TOTAL};

/// The files a desktop leaves in a folder that are no part of a skin: they are not packed.
const JUNK: [&str; 4] = [".DS_Store", "Thumbs.db", "desktop.ini", "__MACOSX"];

/// What an archive is written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// A classic skin.
    Wsz,
    /// A modern skin.
    Wal,
    /// A plain zip.
    Zip,
}

impl Ending {
    /// The file name ending, without the dot.
    pub fn name(self) -> &'static str {
        match self {
            Ending::Wsz => "wsz",
            Ending::Wal => "wal",
            Ending::Zip => "zip",
        }
    }
}

/// What came out of an archive.
#[derive(Debug, Default)]
pub struct Unpacked {
    /// The files written.
    pub files: usize,
    /// The entries left out, each with why.
    pub skipped: Vec<String>,
}

/// Unpacks the archive `bytes` into `dir`, with the paths the archive gives. A folder that
/// is there and not empty is only written into when `force`: its files are replaced, the
/// others stay. Nothing is written when an entry would land outside `dir`.
pub fn unpack(bytes: &[u8], dir: &Path, force: bool) -> Result<Unpacked, String> {
    let mut zip =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("not a zip archive: {e}"))?;
    // every path is checked before anything is written
    let mut paths = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i).map_err(|e| e.to_string())?;
        // Windows zip tools write backslashes
        let name = entry.name().replace('\\', "/");
        let path = inside(&name).ok_or_else(|| {
            format!("the archive's \"{name}\" would land outside the folder; nothing unpacked")
        })?;
        paths.push((path, entry.is_dir()));
    }
    if !force && std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some()) {
        return Err(format!(
            "{} is there and not empty; --force writes into it",
            dir.display()
        ));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut unpacked = Unpacked::default();
    let mut total = 0;
    for (i, (path, is_dir)) in paths.into_iter().enumerate() {
        let target = dir.join(&path);
        if is_dir {
            std::fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
            continue;
        }
        let shown = path.to_string_lossy().replace('\\', "/");
        // an entry that does not open or unpack is left out, as Winamp leaves out a broken
        // file; the sizes an archive claims can lie, so what comes out is counted
        let mut bytes = Vec::new();
        let read = zip
            .by_index(i)
            .map_err(|e| e.to_string())
            .and_then(|mut entry| {
                entry
                    .by_ref()
                    .take(MAX_ENTRY + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())
            });
        let read = match read {
            Ok(read) => read as u64,
            Err(e) => {
                unpacked.skipped.push(format!("{shown}: {e}"));
                continue;
            }
        };
        if read > MAX_ENTRY {
            unpacked
                .skipped
                .push(format!("{shown}: larger than {} MB", MAX_ENTRY >> 20));
            continue;
        }
        total += read;
        if total > MAX_TOTAL {
            return Err(format!(
                "the archive unpacks to more than {} MB",
                MAX_TOTAL >> 20
            ));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&target, &bytes).map_err(|e| format!("{}: {e}", target.display()))?;
        unpacked.files += 1;
    }
    Ok(unpacked)
}

/// An archive's path as a path under the folder it unpacks into, a leading `/` dropped;
/// `None` for one that leaves it (`..`, a drive) or names nothing.
fn inside(name: &str) -> Option<PathBuf> {
    let mut path = PathBuf::new();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            part => {
                // a part that is more than a plain name (`C:`, a root) leaves the folder
                let mut components = Path::new(part).components();
                match (components.next(), components.next()) {
                    (Some(Component::Normal(_)), None) => path.push(part),
                    _ => return None,
                }
            }
        }
    }
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

/// Whether `dir` holds a skin: a `skin.xml` (modern) or a `main.bmp` (classic) in it, or in
/// the only folder in it, as an archive with its skin in a folder unpacks. `None` when it
/// holds none.
pub fn skin_kind(dir: &Path) -> Option<Ending> {
    let marker = |dir: &Path| {
        let names = names_in(dir);
        if names.iter().any(|n| n.eq_ignore_ascii_case("skin.xml")) {
            Some(Ending::Wal)
        } else if names.iter().any(|n| n.eq_ignore_ascii_case("main.bmp")) {
            Some(Ending::Wsz)
        } else {
            None
        }
    };
    marker(dir).or_else(|| match names_in(dir).as_slice() {
        [only] if dir.join(only).is_dir() => marker(&dir.join(only)),
        _ => None,
    })
}

/// The names in `dir`, without the desktop's own files, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !is_junk(n))
        .collect();
    names.sort();
    names
}

fn is_junk(name: &str) -> bool {
    JUNK.iter().any(|j| name.eq_ignore_ascii_case(j)) || name.starts_with("._")
}

/// The skin folders `dir` names: `dir` itself when it holds a skin, otherwise the folders in
/// it that do, by name.
pub fn skin_folders(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !dir.is_dir() {
        return Err(format!("{}: not a folder", dir.display()));
    }
    if skin_kind(dir).is_some() {
        return Ok(vec![dir.to_path_buf()]);
    }
    let folders: Vec<PathBuf> = names_in(dir)
        .into_iter()
        .map(|n| dir.join(n))
        .filter(|p| p.is_dir() && skin_kind(p).is_some())
        .collect();
    if folders.is_empty() {
        return Err(format!(
            "{}: no skin in it (a skin.xml or a main.bmp) or in the folders in it",
            dir.display()
        ));
    }
    Ok(folders)
}

/// The archive of the files under `dir`, at their paths under it. The same files always make
/// the same bytes: the entries go in by path, each with the same date (1980-01-01, the
/// earliest a zip holds) and permissions, so a skin packed twice is the same file.
pub fn pack(dir: &Path) -> Result<Vec<u8>, String> {
    let mut files = Vec::new();
    walk(dir, "", &mut files)?;
    if files.is_empty() {
        return Err(format!("{}: no files in it", dir.display()));
    }
    files.sort();
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for name in files {
        let path = dir.join(&name);
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

/// The files under `dir`, as archive paths that start with `prefix`.
fn walk(dir: &Path, prefix: &str, files: &mut Vec<String>) -> Result<(), String> {
    for name in names_in(dir) {
        let path = dir.join(&name);
        let entry = format!("{prefix}{name}");
        if path.is_dir() {
            walk(&path, &format!("{entry}/"), files)?;
        } else if path.is_file() {
            files.push(entry);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_leaves_the_folder_is_refused() {
        assert_eq!(inside("main.bmp"), Some(PathBuf::from("main.bmp")));
        assert_eq!(
            inside("Skin/./xml//player.xml"),
            Some(["Skin", "xml", "player.xml"].iter().collect())
        );
        assert_eq!(inside("../evil.bmp"), None);
        assert_eq!(inside("Skin/../../evil.bmp"), None);
        assert_eq!(
            inside("/etc/passwd"),
            Some(["etc", "passwd"].iter().collect())
        );
        assert_eq!(inside(""), None);
        assert_eq!(inside("Skin/"), Some(PathBuf::from("Skin")));
    }

    #[test]
    fn desktop_files_are_junk() {
        assert!(is_junk(".DS_Store"));
        assert!(is_junk("._main.bmp"));
        assert!(is_junk("thumbs.db"));
        assert!(!is_junk("main.bmp"));
    }
}
