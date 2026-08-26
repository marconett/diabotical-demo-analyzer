//! Finds a player's standout moments ("scenes") in Diabotical demo files.
//!
//! The crate reads only what the scene finder needs from a demo: the plaintext
//! header, the container framing of the inflated stream, and the verified
//! movement, combat, player-state, and name messages used by the search.
//! Everything else in a demo is skipped without being decoded.

pub mod cache;
pub mod discover;
pub mod extract;
pub mod format;
pub mod input;
pub mod params;
pub mod pyset;
pub mod report;
pub mod run;
pub mod scan;
pub mod scenes;
pub mod text;

pub use cache::{Cache, CacheStats, FileIdentity};
pub use discover::{discover_players, DiscoverResult, PlayerHit, DISCOVER_BUDGET};
pub use extract::DemoExtract;
pub use format::header::{DemoMeta, Magic};
pub use input::{list_demos, list_demos_many, DemoFile};
pub use params::{Condition, Params, RuleGroup, RuleQuery, SearchRule};
pub use report::{evaluate, DemoReport, RunReport, Scene, SceneMetric};
pub use run::{run, RunControl};
pub use scan::{scan_demo, ScanError, ScanOptions};
