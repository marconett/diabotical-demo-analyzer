//! Walks the inflated frame stream and hands out the raw bytes of the game
//! messages the scene finder decodes.
//!
//! EVGR streams start with a `u32` and 5 padding bytes, then a sequence of
//! records tagged by a `u8` type: 1 = `f32 ts, 5 pad, u32 len, msg[len], 8
//! pad` (the only record carrying a game message); 2 = 24 bytes; 3 = `f32` + 2
//! bytes; 4 = `f32` + 5 bytes; 5 = `f32 ts, u32 len, data[len]` + 15 bytes.
//! Any other type ends the walk. DBSR streams are a flat sequence of frames
//! `u8 lead, f32 ts, u32 tick, u64 guid, u32 size, msg[size]`.
//!
//! The walker is fed the stream in chunks and only ever consumes complete
//! records. Records it does not need are skipped by length, so a large
//! irrelevant record never has to be buffered in full.

use super::header::Magic;

pub const MSG_PROPERTY: u8 = 0xDD;
pub const MSG_ENTITY_EVENT: u8 = 0xDE;
pub const MSG_DAMAGE: u8 = 0xE3;
pub const MSG_ACCURACY: u8 = 0xE5;
pub const MSG_MOVEMENT: u8 = 0xEA;
pub const MSG_TEXT: u8 = 0xFF;

const EVGR_STREAM_HEADER: u64 = 9;
const EVGR_MSG_HEADER: usize = 14;
const EVGR_MSG_TRAILER: usize = 8;
const DBSR_FRAME_HEADER: usize = 21;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    UnknownRecordType { rt: u8, offset: u64 },
    Oversize(u64),
}

pub struct Walker {
    magic: Magic,
    /// Bytes of the current irrelevant record that still have to be dropped
    /// before `feed` can continue.
    pub skip: u64,
    pub stopped: Option<Stop>,
    /// Message-bearing records seen (relevant or not).
    pub msg_count: u64,
    /// Absolute stream offset of the next byte `feed` expects.
    pub pos: u64,
}

fn u32_at(buf: &[u8], off: usize) -> usize {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize
}

fn f32_at(buf: &[u8], off: usize) -> f32 {
    f32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

fn is_relevant(magic: Magic, id: u8) -> bool {
    match magic {
        Magic::Evgr => matches!(
            id,
            MSG_PROPERTY | MSG_ENTITY_EVENT | MSG_DAMAGE | MSG_ACCURACY | MSG_MOVEMENT | MSG_TEXT
        ),
        // DamageInstance is unicast to an EVGR client's POV and cannot be
        // attributed in a server recording. The other messages carry ids.
        Magic::Dbsr => matches!(
            id,
            MSG_PROPERTY | MSG_ENTITY_EVENT | MSG_ACCURACY | MSG_MOVEMENT | MSG_TEXT
        ),
    }
}

pub enum Item<'a> {
    Message { ts: f32, data: &'a [u8] },
}

enum Step {
    /// Need more bytes before this record can be classified.
    NeedMore,
    /// A relevant record occupying `total` bytes from the current offset; the
    /// message itself is `buf[msg_start..msg_start + msg_len]`.
    Relevant {
        total: usize,
        msg_start: usize,
        msg_len: usize,
        ts: f32,
    },
    /// An irrelevant record of `total` bytes; `counts` says whether it carries
    /// a message (for `msg_count`).
    Skip {
        total: usize,
        counts: bool,
    },
    Stop(Stop),
}

impl Walker {
    pub fn new(magic: Magic) -> Self {
        let skip = match magic {
            Magic::Evgr => EVGR_STREAM_HEADER,
            Magic::Dbsr => 0,
        };
        Walker {
            magic,
            skip,
            stopped: None,
            msg_count: 0,
            pos: 0,
        }
    }

    /// Tells the walker that `n` bytes were dropped by the caller to satisfy `skip`.
    pub fn skipped(&mut self, n: u64) {
        self.skip -= n;
        self.pos += n;
    }

    fn classify(&self, buf: &[u8], off: usize) -> Step {
        let avail = buf.len() - off;
        match self.magic {
            Magic::Evgr => match buf[off] {
                1 => {
                    if avail < EVGR_MSG_HEADER {
                        return Step::NeedMore;
                    }
                    let len = u32_at(buf, off + 10);
                    let total = EVGR_MSG_HEADER + len + EVGR_MSG_TRAILER;
                    if len == 0 {
                        return Step::Skip {
                            total,
                            counts: true,
                        };
                    }
                    if avail < EVGR_MSG_HEADER + 1 {
                        return Step::NeedMore;
                    }
                    if is_relevant(self.magic, buf[off + EVGR_MSG_HEADER]) {
                        Step::Relevant {
                            total,
                            msg_start: off + EVGR_MSG_HEADER,
                            msg_len: len,
                            ts: f32_at(buf, off + 1),
                        }
                    } else {
                        Step::Skip {
                            total,
                            counts: true,
                        }
                    }
                }
                // The meaning of this fixed-size EVGR record is not verified;
                // movement comes from the explicit 0xEA message instead.
                2 => Step::Skip {
                    total: 25,
                    counts: false,
                },
                3 => Step::Skip {
                    total: 7,
                    counts: false,
                },
                4 => Step::Skip {
                    total: 10,
                    counts: false,
                },
                5 => {
                    if avail < 9 {
                        return Step::NeedMore;
                    }
                    let len = u32_at(buf, off + 5);
                    Step::Skip {
                        total: 9 + len + 15,
                        counts: false,
                    }
                }
                rt => Step::Stop(Stop::UnknownRecordType {
                    rt,
                    offset: self.pos + off as u64,
                }),
            },
            Magic::Dbsr => {
                if avail < DBSR_FRAME_HEADER {
                    return Step::NeedMore;
                }
                let size = u32_at(buf, off + 17);
                let total = DBSR_FRAME_HEADER + size;
                if size == 0 {
                    return Step::Skip {
                        total,
                        counts: true,
                    };
                }
                if avail < DBSR_FRAME_HEADER + 1 {
                    return Step::NeedMore;
                }
                if is_relevant(self.magic, buf[off + DBSR_FRAME_HEADER]) {
                    Step::Relevant {
                        total,
                        msg_start: off + DBSR_FRAME_HEADER,
                        msg_len: size,
                        ts: f32_at(buf, off + 1),
                    }
                } else {
                    Step::Skip {
                        total,
                        counts: true,
                    }
                }
            }
        }
    }

    /// Consumes complete records from `buf` and returns how many bytes were
    /// used. The caller keeps the remainder (an incomplete record) and feeds
    /// it again with more data appended. When `skip` is non-zero afterwards,
    /// that many further stream bytes belong to an irrelevant record and must
    /// be dropped (see `skipped`) before feeding again.
    pub fn feed(&mut self, buf: &[u8], sink: &mut impl FnMut(Item<'_>)) -> usize {
        let mut off = 0;
        while off < buf.len() && self.stopped.is_none() && self.skip == 0 {
            match self.classify(buf, off) {
                Step::NeedMore => break,
                Step::Relevant {
                    total,
                    msg_start,
                    msg_len,
                    ts,
                } => {
                    if buf.len() - off < total {
                        break;
                    }
                    self.msg_count += 1;
                    sink(Item::Message {
                        ts,
                        data: &buf[msg_start..msg_start + msg_len],
                    });
                    off += total;
                }
                Step::Skip { total, counts } => {
                    if counts {
                        self.msg_count += 1;
                    }
                    let avail = buf.len() - off;
                    if total <= avail {
                        off += total;
                    } else {
                        self.skip = (total - avail) as u64;
                        off = buf.len();
                    }
                }
                Step::Stop(stop) => {
                    self.stopped = Some(stop);
                    break;
                }
            }
        }
        self.pos += off as u64;
        off
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec_msg(ts: f32, msg: &[u8]) -> Vec<u8> {
        let mut v = vec![1u8];
        v.extend_from_slice(&ts.to_le_bytes());
        v.extend_from_slice(&[0; 5]);
        v.extend_from_slice(&(msg.len() as u32).to_le_bytes());
        v.extend_from_slice(msg);
        v.extend_from_slice(&[0; 8]);
        v
    }

    fn rec5(len: usize) -> Vec<u8> {
        let mut v = vec![5u8];
        v.extend_from_slice(&1.0f32.to_le_bytes());
        v.extend_from_slice(&(len as u32).to_le_bytes());
        v.extend(std::iter::repeat_n(0xAB, len));
        v.extend_from_slice(&[0; 15]);
        v
    }

    fn frame(ts: f32, msg: &[u8]) -> Vec<u8> {
        let mut v = vec![0u8];
        v.extend_from_slice(&ts.to_le_bytes());
        v.extend_from_slice(&7u32.to_le_bytes());
        v.extend_from_slice(&9u64.to_le_bytes());
        v.extend_from_slice(&(msg.len() as u32).to_le_bytes());
        v.extend_from_slice(msg);
        v
    }

    /// Drives a walker over `stream` in chunks of `chunk` bytes, mimicking the
    /// scan loop (carry-over + skip), and collects (ts, msg) pairs.
    pub(crate) fn drive(
        magic: Magic,
        stream: &[u8],
        chunk: usize,
    ) -> (Vec<(f32, Vec<u8>)>, Walker) {
        let mut walker = Walker::new(magic);
        let mut got = Vec::new();
        let mut buf: Vec<u8> = Vec::new();
        let mut pos = 0;
        loop {
            while walker.skip > 0 {
                let d = (buf.len() as u64).min(walker.skip) as usize;
                buf.drain(..d);
                walker.skipped(d as u64);
                if walker.skip > 0 {
                    if pos >= stream.len() {
                        return (got, walker);
                    }
                    let end = (pos + chunk).min(stream.len());
                    buf.extend_from_slice(&stream[pos..end]);
                    pos = end;
                }
            }
            let consumed = walker.feed(&buf, &mut |item| {
                let Item::Message { ts, data } = item;
                got.push((ts, data.to_vec()));
            });
            buf.drain(..consumed);
            if walker.stopped.is_some() {
                return (got, walker);
            }
            if pos >= stream.len() {
                return (got, walker);
            }
            let end = (pos + chunk).min(stream.len());
            buf.extend_from_slice(&stream[pos..end]);
            pos = end;
        }
    }

    fn evgr_stream() -> (Vec<u8>, Vec<(f32, Vec<u8>)>) {
        let prop: Vec<u8> = {
            let mut m = vec![0xDDu8; 25];
            m[24] = 0x25;
            m
        };
        let text: Vec<u8> = {
            let mut m = vec![0xFF, 0, 0, 4];
            m.extend_from_slice(&0u32.to_le_bytes());
            m.extend_from_slice(&5u32.to_le_bytes());
            m.extend_from_slice(b"1;u;n");
            m
        };
        let other = vec![0xDCu8; 36];
        let movement = vec![0xEAu8; 12];
        let entity = vec![0xDEu8; 24];
        let mut s = vec![0u8; 9];
        let mut expect = Vec::new();
        s.extend(rec_msg(1.5, &other));
        s.extend(rec_msg(1.75, &movement));
        expect.push((1.75, movement));
        s.extend(rec_msg(1.8, &entity));
        expect.push((1.8, entity));
        s.extend(rec_msg(2.0, &prop));
        expect.push((2.0, prop.clone()));
        s.extend([2u8]);
        s.extend([0u8; 24]);
        s.extend([3u8, 0, 0, 0, 0, 0, 0]);
        s.extend([4u8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        s.extend(rec5(5000));
        s.extend(rec_msg(3.25, &text));
        expect.push((3.25, text.clone()));
        s.extend(rec_msg(3.5, &[0xE1u8; 24]));
        s.extend(rec_msg(4.0, &prop));
        expect.push((4.0, prop));
        (s, expect)
    }

    #[test]
    fn evgr_one_shot_matches_chunked() {
        let (stream, expect) = evgr_stream();
        for chunk in [stream.len(), 1, 2, 3, 7, 13, 64, 1000] {
            let (got, walker) = drive(Magic::Evgr, &stream, chunk);
            assert_eq!(got, expect, "chunk size {chunk}");
            assert_eq!(walker.msg_count, 7, "chunk size {chunk}");
            assert!(walker.stopped.is_none());
        }
    }

    #[test]
    fn evgr_truncated_tail_is_dropped() {
        let (stream, expect) = evgr_stream();
        let cut = &stream[..stream.len() - 3];
        let (got, walker) = drive(Magic::Evgr, cut, 5);
        assert_eq!(got, expect[..expect.len() - 1].to_vec());
        assert!(walker.stopped.is_none());
    }

    #[test]
    fn evgr_unknown_record_type_stops() {
        let (mut stream, expect) = evgr_stream();
        let len_before = stream.len();
        stream.push(9);
        stream.extend(rec_msg(5.0, &[0xDDu8; 25]));
        let (got, walker) = drive(Magic::Evgr, &stream, 11);
        assert_eq!(got, expect);
        assert_eq!(
            walker.stopped,
            Some(Stop::UnknownRecordType {
                rt: 9,
                offset: len_before as u64
            })
        );
    }

    #[test]
    fn dbsr_frames() {
        let prop = vec![0xDDu8; 25];
        let mut s = Vec::new();
        s.extend(frame(0.5, &[0xDCu8; 36]));
        s.extend(frame(1.0, &prop));
        s.extend(frame(1.5, &[0xE4u8; 41]));
        s.extend(frame(2.0, &prop));
        for chunk in [s.len(), 1, 4, 22, 30] {
            let (got, walker) = drive(Magic::Dbsr, &s, chunk);
            assert_eq!(
                got,
                vec![(1.0, prop.clone()), (2.0, prop.clone())],
                "chunk {chunk}"
            );
            assert_eq!(walker.msg_count, 4);
        }
        let (got, _) = drive(Magic::Dbsr, &s[..s.len() - 1], 8);
        assert_eq!(got.len(), 1);
    }
}
