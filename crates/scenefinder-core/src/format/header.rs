//! The plaintext header at the start of a demo file, ahead of the gzip stream.
//!
//! Layout (all little-endian): `magic[4]`, `u32 format_version`, then three
//! length-prefixed strings (`u32 len` + UTF-8) app_version / game_mode /
//! map_name, then a magic-specific block. EVGR (client recording, `.rbr`):
//! `u32 created_at` + 16 bytes. DBSR (server recording, `.srd`): `f64
//! time_elapsed`, `u32 n` agents of {pstr username, pstr steam_id, u64 guid,
//! u32 score}, `u32 m` leavers of {pstr name, u64 guid}. The gzip stream
//! follows immediately and runs to the end of the file.

use std::fmt;
use std::io::{self, Read};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Magic {
    #[serde(rename = "EVGR")]
    Evgr,
    #[serde(rename = "DBSR")]
    Dbsr,
}

impl Magic {
    pub fn as_str(self) -> &'static str {
        match self {
            Magic::Evgr => "EVGR",
            Magic::Dbsr => "DBSR",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoMeta {
    pub magic: Magic,
    pub format_version: u32,
    pub app_version: String,
    pub game_mode: String,
    pub map_name: String,
    pub created_at: Option<u32>,
    pub time_elapsed: Option<f64>,
    pub agents: Vec<String>,
}

#[derive(Debug)]
pub enum FormatError {
    Io(io::Error),
    NotADemo(String),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Io(e) => write!(f, "{e}"),
            FormatError::NotADemo(why) => write!(f, "not a Diabotical demo ({why})"),
        }
    }
}

impl std::error::Error for FormatError {}

impl From<io::Error> for FormatError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            FormatError::NotADemo("header is cut short".into())
        } else {
            FormatError::Io(e)
        }
    }
}

const MAX_STRING: u32 = 64 * 1024;
const MAX_LIST: u32 = 4096;

fn read_array<R: Read, const N: usize>(r: &mut R) -> io::Result<[u8; N]> {
    let mut buf = [0u8; N];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

fn read_u32<R: Read>(r: &mut R) -> io::Result<u32> {
    Ok(u32::from_le_bytes(read_array(r)?))
}

fn read_u64<R: Read>(r: &mut R) -> io::Result<u64> {
    Ok(u64::from_le_bytes(read_array(r)?))
}

fn read_f64<R: Read>(r: &mut R) -> io::Result<f64> {
    Ok(f64::from_le_bytes(read_array(r)?))
}

fn read_pstr<R: Read>(r: &mut R) -> Result<String, FormatError> {
    let len = read_u32(r)?;
    if len > MAX_STRING {
        return Err(FormatError::NotADemo(format!(
            "string length {len} out of range"
        )));
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn read_count<R: Read>(r: &mut R, what: &str) -> Result<u32, FormatError> {
    let n = read_u32(r)?;
    if n > MAX_LIST {
        return Err(FormatError::NotADemo(format!(
            "{what} count {n} out of range"
        )));
    }
    Ok(n)
}

/// Parses the header and leaves `r` positioned at the first byte of the gzip stream.
pub fn read_header<R: Read>(r: &mut R) -> Result<DemoMeta, FormatError> {
    let magic = match &read_array::<R, 4>(r)? {
        b"EVGR" => Magic::Evgr,
        b"DBSR" => Magic::Dbsr,
        other => {
            return Err(FormatError::NotADemo(format!(
                "bad magic {:?}",
                String::from_utf8_lossy(other)
            )))
        }
    };
    let format_version = read_u32(r)?;
    let app_version = read_pstr(r)?;
    let game_mode = read_pstr(r)?;
    let map_name = read_pstr(r)?;
    let mut meta = DemoMeta {
        magic,
        format_version,
        app_version,
        game_mode,
        map_name,
        created_at: None,
        time_elapsed: None,
        agents: Vec::new(),
    };
    match magic {
        Magic::Evgr => {
            meta.created_at = Some(read_u32(r)?);
            read_array::<R, 16>(r)?;
        }
        Magic::Dbsr => {
            meta.time_elapsed = Some(read_f64(r)?);
            let agents = read_count(r, "agent")?;
            for _ in 0..agents {
                let username = read_pstr(r)?;
                read_pstr(r)?;
                read_u64(r)?;
                read_u32(r)?;
                meta.agents.push(username);
            }
            let leavers = read_count(r, "leaver")?;
            for _ in 0..leavers {
                read_pstr(r)?;
                read_u64(r)?;
            }
        }
    }
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pstr(s: &str) -> Vec<u8> {
        let mut v = (s.len() as u32).to_le_bytes().to_vec();
        v.extend_from_slice(s.as_bytes());
        v
    }

    #[test]
    fn evgr_header() {
        let mut bytes = b"EVGR".to_vec();
        bytes.extend_from_slice(&6u32.to_le_bytes());
        bytes.extend(pstr("0.20.471s"));
        bytes.extend(pstr("wipeout"));
        bytes.extend(pstr("wo_map"));
        bytes.extend_from_slice(&1234u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 16]);
        bytes.extend_from_slice(b"GZ");
        let mut cur = io::Cursor::new(bytes);
        let meta = read_header(&mut cur).unwrap();
        assert_eq!(meta.magic, Magic::Evgr);
        assert_eq!(meta.format_version, 6);
        assert_eq!(meta.app_version, "0.20.471s");
        assert_eq!(meta.game_mode, "wipeout");
        assert_eq!(meta.map_name, "wo_map");
        assert_eq!(meta.created_at, Some(1234));
        assert_eq!(cur.position(), (cur.get_ref().len() - 2) as u64);
    }

    #[test]
    fn dbsr_header() {
        let mut bytes = b"DBSR".to_vec();
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend(pstr("0.20"));
        bytes.extend(pstr("duel"));
        bytes.extend(pstr("map"));
        bytes.extend_from_slice(&12.5f64.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        for name in ["alice", "bob"] {
            bytes.extend(pstr(name));
            bytes.extend(pstr("steam"));
            bytes.extend_from_slice(&7u64.to_le_bytes());
            bytes.extend_from_slice(&3u32.to_le_bytes());
        }
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend(pstr("leaver"));
        bytes.extend_from_slice(&9u64.to_le_bytes());
        bytes.extend_from_slice(b"GZ");
        let mut cur = io::Cursor::new(bytes);
        let meta = read_header(&mut cur).unwrap();
        assert_eq!(meta.magic, Magic::Dbsr);
        assert_eq!(meta.time_elapsed, Some(12.5));
        assert_eq!(meta.agents, vec!["alice", "bob"]);
        assert_eq!(cur.position(), (cur.get_ref().len() - 2) as u64);
    }

    #[test]
    fn rejects_other_files() {
        let mut cur = io::Cursor::new(b"PK\x03\x04somethingelse".to_vec());
        assert!(matches!(
            read_header(&mut cur),
            Err(FormatError::NotADemo(_))
        ));
        let mut cur = io::Cursor::new(b"EV".to_vec());
        assert!(matches!(
            read_header(&mut cur),
            Err(FormatError::NotADemo(_))
        ));
    }
}
