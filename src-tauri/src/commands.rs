use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use scenefinder_core::{
    Cache, CacheStats, DemoFile, DiscoverResult, Params, RunControl, RunReport,
};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::state::AppState;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: &'static str,
    pub files_done: u32,
    pub files_total: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub cache_hits: u32,
    pub elapsed_ms: u64,
    pub finished: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DemoList {
    pub files: Vec<DemoFile>,
    pub total_bytes: u64,
}

fn snapshot(phase: &'static str, ctl: &RunControl, started: Instant, finished: bool) -> Progress {
    Progress {
        phase,
        files_done: ctl.files_done.load(Ordering::Relaxed),
        files_total: ctl.files_total.load(Ordering::Relaxed),
        bytes_done: ctl.bytes_done.load(Ordering::Relaxed),
        bytes_total: ctl.bytes_total.load(Ordering::Relaxed),
        cache_hits: ctl.cache_hits.load(Ordering::Relaxed),
        elapsed_ms: started.elapsed().as_millis() as u64,
        finished,
    }
}

/// Runs a scan job on the blocking pool while a reporter thread streams
/// progress snapshots; the last snapshot is sent after the job finished.
async fn run_job<T: Send + 'static>(
    state: &AppState,
    phase: &'static str,
    on_progress: Channel<Progress>,
    job: impl FnOnce(&RunControl, Option<&Cache>) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("another scan is still running".into());
    }
    let ctl = Arc::clone(&state.ctl);
    let busy = Arc::clone(&state.busy);
    let cache_dir = state.cache_dir.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let done = Arc::new(AtomicBool::new(false));
        let reporter = {
            let ctl = Arc::clone(&ctl);
            let done = Arc::clone(&done);
            thread::spawn(move || loop {
                let finished = done.load(Ordering::SeqCst);
                let _ = on_progress.send(snapshot(phase, &ctl, started, finished));
                if finished {
                    break;
                }
                thread::sleep(PROGRESS_INTERVAL);
            })
        };
        let cache = Cache::open(&cache_dir).ok();
        let result = job(&ctl, cache.as_ref());
        done.store(true, Ordering::SeqCst);
        let _ = reporter.join();
        busy.store(false, Ordering::SeqCst);
        result
    })
    .await
    .map_err(|e| e.to_string())?;
    result
}

fn to_files(paths: Vec<String>) -> Result<Vec<DemoFile>, String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    scenefinder_core::list_demos_many(&paths).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_demos(paths: Vec<String>) -> Result<DemoList, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let files = to_files(paths)?;
        let total_bytes = files.iter().map(|f| f.size).sum();
        Ok(DemoList { files, total_bytes })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn discover_players(
    state: State<'_, AppState>,
    paths: Vec<String>,
    on_progress: Channel<Progress>,
) -> Result<DiscoverResult, String> {
    run_job(&state, "discover", on_progress, move |ctl, cache| {
        let files = to_files(paths)?;
        Ok(scenefinder_core::discover_players(&files, cache, ctl))
    })
    .await
}

#[tauri::command]
pub async fn run_scene_finder(
    state: State<'_, AppState>,
    paths: Vec<String>,
    params: Params,
    on_progress: Channel<Progress>,
) -> Result<RunReport, String> {
    params.validate()?;
    run_job(&state, "run", on_progress, move |ctl, cache| {
        let files = to_files(paths)?;
        scenefinder_core::run(&files, &params, cache, ctl)
    })
    .await
}

#[tauri::command]
pub fn cancel(state: State<'_, AppState>) {
    state.ctl.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn cache_stats(state: State<'_, AppState>) -> Result<CacheStats, String> {
    Cache::open(&state.cache_dir)
        .and_then(|c| c.stats())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_cache(state: State<'_, AppState>) -> Result<CacheStats, String> {
    if state.busy.load(Ordering::SeqCst) {
        return Err("cannot clear the cache while a scan is running".into());
    }
    Cache::open(&state.cache_dir)
        .and_then(|c| c.clear())
        .map_err(|e| e.to_string())
}
