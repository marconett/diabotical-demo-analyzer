//! Scans one demo file: header, streaming inflate, container walk, extraction.

use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::extract::{DemoExtract, Extractor};
use crate::format::header::{read_header, FormatError};
use crate::format::inflate::Inflater;
use crate::format::walk::{Item, Stop, Walker};

const READ_BUF: usize = 1 << 20;
const CHUNK: usize = 1 << 20;
const MAX_RECORD: usize = 64 << 20;

#[derive(Default, Clone, Copy)]
pub struct ScanOptions<'a> {
    /// Stop after this many inflated bytes (the extract is then marked incomplete).
    pub inflate_budget: Option<u64>,
    pub cancel: Option<&'a AtomicBool>,
    /// Incremented with every compressed byte read from the file.
    pub bytes_read: Option<&'a AtomicU64>,
}

#[derive(Debug)]
pub enum ScanError {
    Io(io::Error),
    NotADemo(String),
    Cancelled,
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::Io(e) => write!(f, "{e}"),
            ScanError::NotADemo(why) => write!(f, "not a Diabotical demo ({why})"),
            ScanError::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<io::Error> for ScanError {
    fn from(e: io::Error) -> Self {
        ScanError::Io(e)
    }
}

impl From<FormatError> for ScanError {
    fn from(e: FormatError) -> Self {
        match e {
            FormatError::Io(e) => ScanError::Io(e),
            FormatError::NotADemo(why) => ScanError::NotADemo(why),
        }
    }
}

struct CountingReader<'a> {
    inner: File,
    counter: Option<&'a AtomicU64>,
}

impl Read for CountingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if let Some(c) = self.counter {
            c.fetch_add(n as u64, Ordering::Relaxed);
        }
        Ok(n)
    }
}

pub fn scan_demo(path: &Path, opts: &ScanOptions) -> Result<DemoExtract, ScanError> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(
        READ_BUF,
        CountingReader {
            inner: file,
            counter: opts.bytes_read,
        },
    );
    let meta = read_header(&mut reader)?;
    let mut inflater = Inflater::new(reader);
    let mut walker = Walker::new(meta.magic);
    let mut extractor = Extractor::new();
    let mut buf = vec![0u8; CHUNK];
    let mut filled = 0usize;
    let mut budget_hit = false;
    let mut sink = |item: Item<'_>| match item {
        Item::Message { ts, data } => extractor.on_msg(ts, data),
    };

    loop {
        if opts.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(ScanError::Cancelled);
        }
        if walker.skip > 0 && filled > 0 {
            let drop = (filled as u64).min(walker.skip) as usize;
            buf.copy_within(drop..filled, 0);
            filled -= drop;
            walker.skipped(drop as u64);
        }
        if walker.skip == 0 && filled > 0 {
            let consumed = walker.feed(&buf[..filled], &mut sink);
            buf.copy_within(consumed..filled, 0);
            filled -= consumed;
            if walker.stopped.is_some() {
                break;
            }
            if walker.skip > 0 {
                continue;
            }
            if filled == buf.len() {
                if buf.len() >= MAX_RECORD {
                    walker.stopped = Some(Stop::Oversize(walker.pos));
                    break;
                }
                buf.resize(buf.len() * 2, 0);
            }
        }
        if budget_hit || inflater.done {
            break;
        }
        let n = inflater.read_chunk(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
        if opts.inflate_budget.is_some_and(|b| inflater.total_out >= b) && !inflater.done {
            budget_hit = true;
        }
    }
    if budget_hit && inflater.done {
        budget_hit = false;
    }

    let mut out = extractor.finish();
    out.meta = Some(meta);
    out.msg_count = walker.msg_count;
    out.inflated_bytes = inflater.total_out;
    out.truncated_gzip = inflater.truncated;
    out.stopped_early = walker.stopped.is_some();
    out.complete = !budget_hit;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.rbr")
    }

    #[test]
    fn scans_the_tiny_fixture() {
        let ex = scan_demo(&fixture(), &ScanOptions::default()).unwrap();
        let meta = ex.meta.as_ref().unwrap();
        assert_eq!(meta.magic.as_str(), "EVGR");
        assert_eq!(meta.app_version, "0.20.471s");
        assert_eq!(meta.game_mode, "wipeout");
        assert_eq!(meta.map_name, "wo_wellspring");
        assert_eq!(ex.names, vec![(1, "27 shaft avg n1".to_string())]);
        assert!(ex.complete);
        assert!(ex.msg_count > 200);
    }

    #[test]
    fn budget_marks_incomplete() {
        let ex = scan_demo(
            &fixture(),
            &ScanOptions {
                inflate_budget: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!ex.complete);
    }

    #[test]
    fn cancel_is_reported() {
        let flag = AtomicBool::new(true);
        let err = scan_demo(
            &fixture(),
            &ScanOptions {
                cancel: Some(&flag),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(matches!(err, ScanError::Cancelled));
    }

    #[test]
    fn non_demo_is_rejected() {
        let err = scan_demo(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
            &ScanOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(err, ScanError::NotADemo(_)));
    }
}
