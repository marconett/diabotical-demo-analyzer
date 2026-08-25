//! Input discovery: a single demo file or a folder walked recursively.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoFile {
    pub path: PathBuf,
    pub size: u64,
}

fn is_demo(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("rbr") || e.eq_ignore_ascii_case("srd"))
}

/// A file yields itself; a directory yields every `.rbr` / `.srd` below it
/// (symlinks are not followed), sorted by path.
pub fn list_demos(input: &Path) -> io::Result<Vec<DemoFile>> {
    let md = fs::metadata(input)?;
    if md.is_file() {
        return Ok(vec![DemoFile {
            path: input.to_path_buf(),
            size: md.len(),
        }]);
    }
    let mut out = Vec::new();
    let mut stack = vec![input.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let path = entry.path();
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() && is_demo(&path) {
                out.push(DemoFile {
                    size: entry.metadata()?.len(),
                    path,
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Lists demos for several inputs in the order given (folders expand in
/// place), dropping duplicates.
pub fn list_demos_many(inputs: &[PathBuf]) -> io::Result<Vec<DemoFile>> {
    let mut out: Vec<DemoFile> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for input in inputs {
        for f in list_demos(input)? {
            if seen.insert(f.path.clone()) {
                out.push(f);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_recursively_and_filters() {
        let root =
            std::env::temp_dir().join(format!("scenefinder-input-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("one.rbr"), b"x").unwrap();
        fs::write(root.join("a/two.SRD"), b"xx").unwrap();
        fs::write(root.join("a/b/three.rbr"), b"xxx").unwrap();
        fs::write(root.join("a/b/notes.txt"), b"x").unwrap();
        let files = list_demos(&root).unwrap();
        let names: Vec<String> = files
            .iter()
            .map(|f| {
                f.path
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["a/b/three.rbr", "a/two.SRD", "one.rbr"]);
        assert_eq!(files[0].size, 3);
        let single = list_demos(&root.join("one.rbr")).unwrap();
        assert_eq!(single.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }
}
