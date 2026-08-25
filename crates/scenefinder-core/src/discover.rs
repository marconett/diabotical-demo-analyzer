//! Lists the player names found in a set of demos by inflating only a
//! bounded prefix of each (names are announced near the start of a match).

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cache::Cache;
use crate::input::DemoFile;
use crate::report::file_name;
use crate::run::{load, schedule, Loaded, RunControl};

/// Inflated bytes scanned per demo when it is not cached yet.
pub const DISCOVER_BUDGET: u64 = 16 << 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerHit {
    pub name: String,
    /// Number of demos the name was seen in.
    pub demos: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverResult {
    pub players: Vec<PlayerHit>,
    pub scanned: u32,
    pub from_cache: u32,
    pub errors: Vec<String>,
    pub cancelled: bool,
    pub elapsed_ms: u64,
}

pub fn discover_players(
    files: &[DemoFile],
    cache: Option<&Cache>,
    ctl: &RunControl,
) -> DiscoverResult {
    let started = std::time::Instant::now();
    ctl.reset(files);
    let order = schedule(files);
    let per_file: Vec<(Vec<String>, Option<String>)> = order
        .par_iter()
        .map(|&i| {
            let file = &files[i];
            let result = match load(&file.path, cache, ctl, Some(DISCOVER_BUDGET)) {
                Loaded::Cached(ex) | Loaded::Scanned(ex) => {
                    (ex.all_names().into_iter().collect(), None)
                }
                Loaded::Failed(e) => (Vec::new(), Some(format!("{}: {e}", file_name(&file.path)))),
                Loaded::Cancelled => (Vec::new(), None),
            };
            ctl.files_done.fetch_add(1, Ordering::Relaxed);
            result
        })
        .collect();
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut errors = Vec::new();
    for (names, err) in per_file {
        for n in names {
            *counts.entry(n).or_default() += 1;
        }
        if let Some(e) = err {
            errors.push(e);
        }
    }
    let mut players: Vec<PlayerHit> = counts
        .into_iter()
        .map(|(name, demos)| PlayerHit { name, demos })
        .collect();
    players.sort_by(|a, b| b.demos.cmp(&a.demos).then_with(|| a.name.cmp(&b.name)));
    let from_cache = ctl.cache_hits.load(Ordering::Relaxed);
    DiscoverResult {
        players,
        scanned: files.len() as u32 - from_cache,
        from_cache,
        errors,
        cancelled: ctl.is_cancelled(),
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}
