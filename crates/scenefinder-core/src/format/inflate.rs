//! Streaming gzip inflate that tolerates a cut-off stream.
//!
//! Recordings are frequently truncated (the gzip member ends before its
//! end-of-stream marker), so an error after some output has been produced is
//! treated as the end of the usable data rather than a failure.

use std::io::{self, BufRead, Read};

use flate2::bufread::MultiGzDecoder;

pub struct Inflater<R: BufRead> {
    dec: MultiGzDecoder<R>,
    pub total_out: u64,
    pub truncated: bool,
    pub done: bool,
}

impl<R: BufRead> Inflater<R> {
    pub fn new(r: R) -> Self {
        Inflater {
            dec: MultiGzDecoder::new(r),
            total_out: 0,
            truncated: false,
            done: false,
        }
    }

    /// Reads the next inflated bytes into `dst`. `Ok(0)` means no more usable
    /// data. An error before any output means the blob is not gzip at all.
    pub fn read_chunk(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        if self.done {
            return Ok(0);
        }
        loop {
            match self.dec.read(dst) {
                Ok(0) => {
                    self.done = true;
                    return Ok(0);
                }
                Ok(n) => {
                    self.total_out += n as u64;
                    return Ok(n);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    self.done = true;
                    if self.total_out > 0 {
                        self.truncated = true;
                        return Ok(0);
                    }
                    return Err(e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn gz(data: &[u8]) -> Vec<u8> {
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    fn drain(bytes: &[u8]) -> (Vec<u8>, bool) {
        let mut inf = Inflater::new(io::Cursor::new(bytes));
        let mut out = Vec::new();
        let mut buf = [0u8; 1000];
        loop {
            let n = inf.read_chunk(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        (out, inf.truncated)
    }

    #[test]
    fn complete_stream() {
        let data: Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
        let (out, truncated) = drain(&gz(&data));
        assert_eq!(out, data);
        assert!(!truncated);
    }

    #[test]
    fn truncated_stream_keeps_prefix() {
        let data: Vec<u8> = (0..200_000u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect();
        let full = gz(&data);
        let cut = &full[..full.len() / 2];
        let (out, truncated) = drain(cut);
        assert!(truncated);
        assert!(!out.is_empty());
        assert!(out.len() < data.len());
        assert_eq!(&out[..], &data[..out.len()]);
    }

    #[test]
    fn garbage_is_an_error() {
        let mut inf = Inflater::new(io::Cursor::new(b"this is not gzip".to_vec()));
        let mut buf = [0u8; 64];
        assert!(inf.read_chunk(&mut buf).is_err());
    }
}
