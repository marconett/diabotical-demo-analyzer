//! Parallel orchestration of scans over many demos, with progress counters
//! and cancellation shared with the caller.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use rayon::prelude::*;

use crate::cache::{Cache, FileIdentity};
use crate::extract::DemoExtract;
use crate::input::DemoFile;
use crate::params::Params;
use crate::report::{evaluate, DemoReport, RunReport};
use crate::scan::{scan_demo, ScanError, ScanOptions};

#[derive(Default)]
pub struct RunControl {
    pub cancel: AtomicBool,
    pub files_done: AtomicU32,
    pub files_total: AtomicU32,
    pub bytes_done: AtomicU64,
    pub bytes_total: AtomicU64,
    pub cache_hits: AtomicU32,
}

impl RunControl {
    pub fn reset(&self, files: &[DemoFile]) {
        self.cancel.store(false, Ordering::Relaxed);
        self.files_done.store(0, Ordering::Relaxed);
        self.files_total
            .store(files.len() as u32, Ordering::Relaxed);
        self.bytes_done.store(0, Ordering::Relaxed);
        self.bytes_total
            .store(files.iter().map(|f| f.size).sum(), Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Indices of `files` ordered largest first, so the slowest scans start
/// early and the parallel tail stays short.
pub(crate) fn schedule(files: &[DemoFile]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..files.len()).collect();
    order.sort_by(|a, b| files[*b].size.cmp(&files[*a].size).then(a.cmp(b)));
    order
}

pub(crate) enum Loaded {
    Cached(DemoExtract),
    Scanned(DemoExtract),
    Failed(String),
    Cancelled,
}

/// Loads a demo's extract from the cache or by scanning it. With a budget the
/// prefix-only cache entries are accepted too.
pub(crate) fn load(
    path: &Path,
    cache: Option<&Cache>,
    ctl: &RunControl,
    budget: Option<u64>,
) -> Loaded {
    if ctl.is_cancelled() {
        return Loaded::Cancelled;
    }
    let identity = match FileIdentity::of(path) {
        Ok(id) => Some(id),
        Err(e) => return Loaded::Failed(e.to_string()),
    };
    if let (Some(cache), Some(id)) = (cache, identity.as_ref()) {
        if let Some(ex) = cache.get(id) {
            if ex.complete || budget.is_some() {
                ctl.cache_hits.fetch_add(1, Ordering::Relaxed);
                ctl.bytes_done.fetch_add(id.size, Ordering::Relaxed);
                return Loaded::Cached(ex);
            }
        }
    }
    let opts = ScanOptions {
        inflate_budget: budget,
        cancel: Some(&ctl.cancel),
        bytes_read: Some(&ctl.bytes_done),
    };
    match scan_demo(path, &opts) {
        Ok(ex) => {
            if let (Some(cache), Some(id)) = (cache, identity.as_ref()) {
                let _ = cache.put(id, &ex);
            }
            Loaded::Scanned(ex)
        }
        Err(ScanError::Cancelled) => Loaded::Cancelled,
        Err(e) => Loaded::Failed(e.to_string()),
    }
}

pub fn run(
    files: &[DemoFile],
    params: &Params,
    cache: Option<&Cache>,
    ctl: &RunControl,
) -> Result<RunReport, String> {
    params.validate()?;
    let started = Instant::now();
    ctl.reset(files);
    let order = schedule(files);
    let mut reports: Vec<(usize, DemoReport)> = order
        .par_iter()
        .map(|&i| {
            let file = &files[i];
            let t0 = Instant::now();
            let report = match load(&file.path, cache, ctl, None) {
                Loaded::Cached(ex) => {
                    let mut r = evaluate(&ex, params, &file.path);
                    r.from_cache = true;
                    r
                }
                Loaded::Scanned(ex) => evaluate(&ex, params, &file.path),
                Loaded::Failed(e) => DemoReport::failed(&file.path, e),
                Loaded::Cancelled => DemoReport::failed(&file.path, "cancelled".into()),
            };
            ctl.files_done.fetch_add(1, Ordering::Relaxed);
            let mut report = report;
            report.scan_ms = t0.elapsed().as_millis() as u32;
            (i, report)
        })
        .collect();
    reports.sort_by_key(|(i, _)| *i);
    let demos = reports.into_iter().map(|(_, r)| r).collect();
    Ok(RunReport::assemble(
        params,
        demos,
        ctl.is_cancelled(),
        ctl.cache_hits.load(Ordering::Relaxed) as usize,
        started.elapsed().as_millis() as u64,
    ))
}
