//! Pulls the per-player event streams the scene finder needs out of the
//! message stream, independent of any search parameters (so the result can
//! be cached and reused for every query).
//!
//! Property messages (0xDD, 25 bytes): `u32 object_id` at byte 3, `u32 value`
//! at byte 11, `u8 key` at byte 24. Keys used: 0x08 assign_team (object_id =
//! player, value = team), 0x0f frag_feed (object_id = victim, value = killer),
//! 0x18 team_multi_round_score (object_id = team, value = score), 0x25
//! total_damage_dealt (object_id = dealer, value = running total).
//! Text messages (0xFF): `u8 flag` at byte 3, `u32 size` at byte 8, payload
//! from byte 12. Flag 0x04 announces a player as `local_id;uuid;name`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::format::header::DemoMeta;
use crate::format::walk::{MSG_PROPERTY, MSG_TEXT};

const PROPERTY_LEN: usize = 25;
const KEY_ASSIGN_TEAM: u8 = 0x08;
const KEY_FRAG_FEED: u8 = 0x0f;
const KEY_ROUND_SCORE: u8 = 0x18;
const KEY_DAMAGE_DEALT: u8 = 0x25;
const TEXT_HEADER_LEN: usize = 12;
const TEXT_PLAYER_INFO: u8 = 0x04;

/// A frag at most this long before a round-score increment is the frag that
/// closed the round.
pub const WIN_FRAG_COUPLING: f64 = 0.1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoExtract {
    pub meta: Option<DemoMeta>,
    /// Name announcements in stream order; a player id can be rebound.
    pub names: Vec<(u32, String)>,
    /// Names that were bound at the moment one of their messages was seen.
    pub names_seen: BTreeSet<String>,
    /// dealer player id -> (timestamp, cumulative damage dealt)
    pub dmg: BTreeMap<u32, Vec<(f32, u32)>>,
    /// killer player id -> kill timestamps
    pub frags: BTreeMap<u32, Vec<f32>>,
    /// player id -> (timestamp of the round-winning frag, round number)
    pub wins: BTreeMap<u32, Vec<(f32, u32)>>,
    pub msg_count: u64,
    pub inflated_bytes: u64,
    pub truncated_gzip: bool,
    pub stopped_early: bool,
    /// False when the scan stopped at a byte budget before the stream ended.
    pub complete: bool,
}

impl DemoExtract {
    /// player id -> final name (the last announcement wins).
    pub fn final_names(&self) -> HashMap<u32, &str> {
        let mut m = HashMap::new();
        for (pid, name) in &self.names {
            m.insert(*pid, name.as_str());
        }
        m
    }

    /// Every distinct name announced in the demo (plus DBSR header names).
    pub fn all_names(&self) -> BTreeSet<String> {
        let mut set: BTreeSet<String> = self.names.iter().map(|(_, n)| n.clone()).collect();
        if let Some(meta) = &self.meta {
            set.extend(meta.agents.iter().cloned());
        }
        set
    }
}

#[derive(Default)]
pub struct Extractor {
    out: DemoExtract,
    name_now: HashMap<u32, String>,
    team: HashMap<u32, u32>,
    round_score: HashMap<u32, u32>,
    last_frag: Option<(f32, u32, u32)>,
}

impl Extractor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_msg(&mut self, ts: f32, msg: &[u8]) {
        match msg[0] {
            MSG_PROPERTY => self.on_property(ts, msg),
            MSG_TEXT => self.on_text(msg),
            _ => {}
        }
    }

    fn on_property(&mut self, ts: f32, msg: &[u8]) {
        if msg.len() < PROPERTY_LEN {
            return;
        }
        let key = msg[24];
        if !matches!(
            key,
            KEY_ASSIGN_TEAM | KEY_FRAG_FEED | KEY_ROUND_SCORE | KEY_DAMAGE_DEALT
        ) {
            return;
        }
        let object_id = u32::from_le_bytes(msg[3..7].try_into().unwrap());
        let value = u32::from_le_bytes(msg[11..15].try_into().unwrap());
        if key == KEY_ASSIGN_TEAM {
            self.team.insert(object_id, value);
        }
        if let Some(name) = self.name_now.get(&object_id) {
            if !self.out.names_seen.contains(name) {
                self.out.names_seen.insert(name.clone());
            }
        }
        match key {
            KEY_DAMAGE_DEALT => {
                self.out.dmg.entry(object_id).or_default().push((ts, value));
            }
            KEY_FRAG_FEED => {
                self.out.frags.entry(value).or_default().push(ts);
                self.last_frag = Some((ts, value, object_id));
            }
            KEY_ROUND_SCORE => {
                let prev = self.round_score.insert(object_id, value);
                let increased = match prev {
                    None => false,
                    Some(p) => value > p,
                };
                if !increased {
                    return;
                }
                let round_no: u32 = self
                    .round_score
                    .values()
                    .fold(0u32, |a, v| a.wrapping_add(*v));
                let Some((fts, killer, victim)) = self.last_frag else {
                    return;
                };
                if f64::from(ts) - f64::from(fts) > WIN_FRAG_COUPLING {
                    return;
                }
                if killer != victim && self.team.get(&killer) == Some(&object_id) {
                    self.out
                        .wins
                        .entry(killer)
                        .or_default()
                        .push((fts, round_no));
                }
            }
            _ => {}
        }
    }

    fn on_text(&mut self, msg: &[u8]) {
        if msg.len() < TEXT_HEADER_LEN || msg[3] != TEXT_PLAYER_INFO {
            return;
        }
        let size = u32::from_le_bytes(msg[8..12].try_into().unwrap()) as usize;
        let payload =
            &msg[TEXT_HEADER_LEN..TEXT_HEADER_LEN + size.min(msg.len() - TEXT_HEADER_LEN)];
        let text = String::from_utf8_lossy(payload);
        let parts: Vec<&str> = text.splitn(3, ';').collect();
        if parts.len() != 3 || parts[0].is_empty() || !parts[0].bytes().all(|b| b.is_ascii_digit())
        {
            return;
        }
        let Ok(pid) = parts[0].parse::<u32>() else {
            return;
        };
        let name = parts[2].to_string();
        self.name_now.insert(pid, name.clone());
        self.out.names.push((pid, name));
    }

    pub fn finish(self) -> DemoExtract {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(key: u8, object_id: u32, value: u32) -> Vec<u8> {
        let mut m = vec![0u8; 25];
        m[0] = 0xDD;
        m[3..7].copy_from_slice(&object_id.to_le_bytes());
        m[11..15].copy_from_slice(&value.to_le_bytes());
        m[24] = key;
        m
    }

    fn player_info(payload: &str) -> Vec<u8> {
        let mut m = vec![0xFF, 0, 0, 4];
        m.extend_from_slice(&0u32.to_le_bytes());
        m.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        m.extend_from_slice(payload.as_bytes());
        m
    }

    #[test]
    fn collects_streams_and_wins() {
        let mut ex = Extractor::new();
        ex.on_msg(1.0, &player_info("1;uuid;alice"));
        ex.on_msg(1.0, &player_info("2;uuid;bob;with;semicolons"));
        ex.on_msg(1.0, &prop(0x08, 1, 0));
        ex.on_msg(1.0, &prop(0x08, 2, 1));
        ex.on_msg(5.0, &prop(0x25, 1, 100));
        ex.on_msg(6.0, &prop(0x25, 1, 180));
        ex.on_msg(6.0, &prop(0x0f, 2, 1)); // alice kills bob
        ex.on_msg(6.05, &prop(0x18, 0, 1)); // first score seen: not an increment
        ex.on_msg(20.0, &prop(0x0f, 2, 1));
        ex.on_msg(20.05, &prop(0x18, 0, 2)); // increment within 0.1 s: alice wins round 2
        ex.on_msg(30.0, &prop(0x0f, 1, 2)); // bob kills alice
        ex.on_msg(30.5, &prop(0x18, 1, 1)); // too late after the frag
        ex.on_msg(40.0, &prop(0x0f, 1, 1)); // suicide
        ex.on_msg(40.01, &prop(0x18, 0, 3));
        let out = ex.finish();
        assert_eq!(
            out.names,
            vec![
                (1, "alice".to_string()),
                (2, "bob;with;semicolons".to_string())
            ]
        );
        assert_eq!(out.dmg[&1], vec![(5.0, 100), (6.0, 180)]);
        assert_eq!(out.frags[&1], vec![6.0, 20.0, 40.0]);
        assert_eq!(out.frags[&2], vec![30.0]);
        assert_eq!(out.wins.get(&1), Some(&vec![(20.0, 2)]));
        assert!(!out.wins.contains_key(&2));
        assert!(out.names_seen.contains("alice"));
    }

    #[test]
    fn ignores_malformed() {
        let mut ex = Extractor::new();
        ex.on_msg(1.0, &player_info("x;uuid;name"));
        ex.on_msg(1.0, &player_info("nope"));
        ex.on_msg(1.0, &[0xDD, 0, 0]);
        ex.on_msg(
            1.0,
            &[
                0xFF, 0, 0, 4, 0, 0, 0, 0, 200, 0, 0, 0, b'1', b';', b'u', b';', b'n',
            ],
        );
        let out = ex.finish();
        assert_eq!(out.names, vec![(1, "n".to_string())]);
        assert!(out.dmg.is_empty());
    }
}
