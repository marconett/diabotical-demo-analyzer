//! Finds a player's standout moments ("scenes") in Diabotical demo files.
//!
//! The crate reads only what the scene finder needs from a demo: the plaintext
//! header, the container framing of the inflated stream, and two message
//! layouts (damage / frag / team / round properties and player-name
//! announcements). Everything else in a demo is skipped without being decoded.

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
pub use params::{Condition, Params};
pub use report::{evaluate, DemoReport, RunReport, Scene};
pub use run::{run, RunControl};
pub use scan::{scan_demo, ScanError, ScanOptions};
