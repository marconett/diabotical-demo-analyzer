//! Persistent per-demo extraction cache: one small file per demo, keyed by
//! the demo's absolute path and validated against its size and mtime.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::extract::DemoExtract;

const MAGIC: &[u8; 4] = b"SFXC";
/// Bump whenever `DemoExtract` or the extraction rules change.
const CACHE_VERSION: u16 = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIdentity {
    pub path: String,
    pub size: u64,
    pub mtime_ns: u64,
}

impl FileIdentity {
    pub fn of(path: &Path) -> io::Result<Self> {
        let abs = std::path::absolute(path)?;
        let md = fs::metadata(&abs)?;
        let mtime_ns = md
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Ok(FileIdentity {
            path: abs.to_string_lossy().into_owned(),
            size: md.len(),
            mtime_ns,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    identity: FileIdentity,
    extract: DemoExtract,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub entries: u64,
    pub bytes: u64,
}

pub struct Cache {
    dir: PathBuf,
    counter: AtomicU64,
}

fn fnv1a64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

impl Cache {
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Cache {
            dir,
            counter: AtomicU64::new(0),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn entry_path(&self, id: &FileIdentity) -> PathBuf {
        self.dir.join(format!("{:016x}.sfc", fnv1a64(&id.path)))
    }

    fn read_entry(&self, id: &FileIdentity) -> Option<CacheEntry> {
        let mut f = fs::File::open(self.entry_path(id)).ok()?;
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).ok()?;
        if bytes.len() < 6
            || &bytes[..4] != MAGIC
            || u16::from_le_bytes([bytes[4], bytes[5]]) != CACHE_VERSION
        {
            return None;
        }
        let entry: CacheEntry = postcard::from_bytes(&bytes[6..]).ok()?;
        (entry.identity == *id).then_some(entry)
    }

    /// The cached extract for this exact file (path, size, mtime), if any.
    pub fn get(&self, id: &FileIdentity) -> Option<DemoExtract> {
        self.read_entry(id).map(|e| e.extract)
    }

    /// Stores an extract. A complete entry is never replaced by a prefix-only one.
    pub fn put(&self, id: &FileIdentity, extract: &DemoExtract) -> io::Result<()> {
        if !extract.complete {
            if let Some(existing) = self.read_entry(id) {
                if existing.extract.complete {
                    return Ok(());
                }
            }
        }
        let entry = CacheEntry {
            identity: id.clone(),
            extract: extract.clone(),
        };
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&CACHE_VERSION.to_le_bytes());
        bytes.extend(postcard::to_stdvec(&entry).map_err(|e| io::Error::other(e.to_string()))?);
        let final_path = self.entry_path(id);
        let tmp = self.dir.join(format!(
            ".tmp-{}-{}",
            std::process::id(),
            self.counter.fetch_add(1, Ordering::Relaxed)
        ));
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&bytes)?;
        }
        fs::rename(&tmp, &final_path)
    }

    pub fn stats(&self) -> io::Result<CacheStats> {
        let mut stats = CacheStats::default();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            if entry.path().extension().is_some_and(|e| e == "sfc") {
                stats.entries += 1;
                stats.bytes += entry.metadata()?.len();
            }
        }
        Ok(stats)
    }

    pub fn clear(&self) -> io::Result<CacheStats> {
        let stats = self.stats()?;
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "sfc") {
                fs::remove_file(path)?;
            }
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "scenefinder-cache-test-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn round_trip_and_identity_check() {
        let dir = temp_dir("rt");
        let cache = Cache::open(&dir).unwrap();
        let id = FileIdentity {
            path: "/x/demo.rbr".into(),
            size: 10,
            mtime_ns: 20,
        };
        let mut ex = DemoExtract {
            complete: true,
            ..Default::default()
        };
        ex.names.push((1, "alice".into()));
        ex.dmg.entry(1).or_default().push((1.5, 100));
        assert!(cache.get(&id).is_none());
        cache.put(&id, &ex).unwrap();
        let back = cache.get(&id).unwrap();
        assert_eq!(back.names, ex.names);
        assert_eq!(back.dmg, ex.dmg);
        let changed = FileIdentity {
            size: 11,
            ..id.clone()
        };
        assert!(cache.get(&changed).is_none());
        let prefix = DemoExtract {
            complete: false,
            ..Default::default()
        };
        cache.put(&id, &prefix).unwrap();
        assert!(cache.get(&id).unwrap().complete);
        assert_eq!(cache.stats().unwrap().entries, 1);
        assert_eq!(cache.clear().unwrap().entries, 1);
        assert_eq!(cache.stats().unwrap().entries, 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
