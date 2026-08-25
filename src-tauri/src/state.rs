use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use scenefinder_core::RunControl;

pub struct AppState {
    pub cache_dir: PathBuf,
    pub ctl: Arc<RunControl>,
    /// One scan job at a time: the progress counters are shared.
    pub busy: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(cache_dir: PathBuf) -> Self {
        AppState {
            cache_dir,
            ctl: Arc::new(RunControl::default()),
            busy: Arc::new(AtomicBool::new(false)),
        }
    }
}
