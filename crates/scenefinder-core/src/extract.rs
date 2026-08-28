//! Pulls the per-player event streams the scene finder needs out of the
//! message stream, independent of any search parameters (so the result can
//! be cached and reused for every query).
//!
//! Property messages (0xDD, 25 bytes): `u32 object_id` at byte 3, `u32 entity`
//! at byte 7, `u32 value` at byte 11, `u8 key` at byte 24. Keys used: 0x08
//! assign_team (object_id =
//! player, value = team), 0x0f frag_feed (object_id = victim, value = killer),
//! 0x18 team_multi_round_score (object_id = team, value = score), 0x25
//! total_damage_dealt (object_id = dealer, value = running total), and 0x3c
//! ammo_usage (object_id = weapon, entity = player).
//! Entity events (0xDE, 24 bytes) identify the recording POV (key 0x07) and
//! spectating transitions (key 0x7e). Damage instances (0xE3, 35 bytes) are
//! server confirmations sent to the attacker and their current followers.
//! Text messages (0xFF): `u8 flag` at byte 3, `u32 size` at byte 8, payload
//! from byte 12. Flag 0x04 announces a player as `local_id;uuid;name`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::format::header::DemoMeta;
use crate::format::walk::{MSG_DAMAGE_INSTANCE, MSG_ENTITY_EVENT, MSG_PROPERTY, MSG_TEXT};

const PROPERTY_LEN: usize = 25;
const ENTITY_EVENT_LEN: usize = 24;
const DAMAGE_INSTANCE_LEN: usize = 35;
const KEY_ASSIGN_TEAM: u8 = 0x08;
const KEY_FRAG_FEED: u8 = 0x0f;
const KEY_ROUND_SCORE: u8 = 0x18;
const KEY_DAMAGE_DEALT: u8 = 0x25;
const KEY_AMMO_USAGE: u8 = 0x3c;
const EVENT_SET_POV: u8 = 0x07;
const EVENT_SPECTATE_POV: u8 = 0x7e;
const TEXT_HEADER_LEN: usize = 12;
const TEXT_PLAYER_INFO: u8 = 0x04;

/// A frag at most this long before a round-score increment is the frag that
/// closed the round.
pub const WIN_FRAG_COUPLING: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeaponAttempt {
    pub ts: f32,
    pub weapon: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeaponHit {
    pub ts: f32,
    pub weapon: u32,
    pub damage: u32,
}

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
    /// Player whose client created this demo, when announced by the stream.
    pub pov_id: Option<u32>,
    /// player id -> ammo-consuming weapon actions
    pub weapon_attempts: BTreeMap<u32, Vec<WeaponAttempt>>,
    /// effective viewed player id -> server-confirmed damaging weapon hits
    pub weapon_hits: BTreeMap<u32, Vec<WeaponHit>>,
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
    spectate_pov: Option<u32>,
}

impl Extractor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_msg(&mut self, ts: f32, msg: &[u8]) {
        let Some(&kind) = msg.first() else {
            return;
        };
        match kind {
            MSG_PROPERTY => self.on_property(ts, msg),
            MSG_ENTITY_EVENT => self.on_entity_event(msg),
            MSG_DAMAGE_INSTANCE => self.on_damage_instance(ts, msg),
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
            KEY_ASSIGN_TEAM | KEY_FRAG_FEED | KEY_ROUND_SCORE | KEY_DAMAGE_DEALT | KEY_AMMO_USAGE
        ) {
            return;
        }
        let object_id = u32::from_le_bytes(msg[3..7].try_into().unwrap());
        let entity = u32::from_le_bytes(msg[7..11].try_into().unwrap());
        let value = u32::from_le_bytes(msg[11..15].try_into().unwrap());
        if key == KEY_ASSIGN_TEAM {
            self.team.insert(object_id, value);
        }
        self.mark_seen(if key == KEY_AMMO_USAGE {
            entity
        } else {
            object_id
        });
        match key {
            KEY_DAMAGE_DEALT => {
                self.out.dmg.entry(object_id).or_default().push((ts, value));
            }
            KEY_FRAG_FEED => {
                self.mark_seen(value);
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
            KEY_AMMO_USAGE => {
                self.out
                    .weapon_attempts
                    .entry(entity)
                    .or_default()
                    .push(WeaponAttempt {
                        ts,
                        weapon: object_id,
                    });
            }
            _ => {}
        }
    }

    fn on_entity_event(&mut self, msg: &[u8]) {
        if msg.len() < ENTITY_EVENT_LEN {
            return;
        }
        let object_id = u32::from_le_bytes(msg[3..7].try_into().unwrap());
        let key = msg[7];
        let argument = u32::from_le_bytes(msg[8..12].try_into().unwrap());
        match key {
            EVENT_SET_POV => self.out.pov_id = Some(object_id),
            EVENT_SPECTATE_POV => {
                self.spectate_pov = (argument != 0).then_some(object_id);
            }
            _ => {}
        }
    }

    fn on_damage_instance(&mut self, ts: f32, msg: &[u8]) {
        if msg.len() < DAMAGE_INSTANCE_LEN {
            return;
        }
        let damage = u32::from_le_bytes(msg[11..15].try_into().unwrap());
        if damage == 0 {
            return;
        }
        let Some(player) = self.spectate_pov.or(self.out.pov_id) else {
            return;
        };
        let weapon = u32::from_le_bytes(msg[15..19].try_into().unwrap());
        self.mark_seen(player);
        self.out
            .weapon_hits
            .entry(player)
            .or_default()
            .push(WeaponHit { ts, weapon, damage });
    }

    fn mark_seen(&mut self, player: u32) {
        if let Some(name) = self.name_now.get(&player) {
            self.out.names_seen.insert(name.clone());
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

    fn ammo_usage(player: u32, weapon: u32) -> Vec<u8> {
        let mut m = prop(KEY_AMMO_USAGE, weapon, 0);
        m[7..11].copy_from_slice(&player.to_le_bytes());
        m
    }

    fn entity_event(player: u32, key: u8, argument: u32) -> Vec<u8> {
        let mut m = vec![0u8; ENTITY_EVENT_LEN];
        m[0] = MSG_ENTITY_EVENT;
        m[3..7].copy_from_slice(&player.to_le_bytes());
        m[7] = key;
        m[8..12].copy_from_slice(&argument.to_le_bytes());
        m
    }

    fn damage_instance(weapon: u32, damage: u32) -> Vec<u8> {
        let mut m = vec![0u8; DAMAGE_INSTANCE_LEN];
        m[0] = MSG_DAMAGE_INSTANCE;
        m[11..15].copy_from_slice(&damage.to_le_bytes());
        m[15..19].copy_from_slice(&weapon.to_le_bytes());
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
        ex.on_msg(1.0, &[]);
        ex.on_msg(1.0, &[MSG_ENTITY_EVENT]);
        ex.on_msg(1.0, &[MSG_DAMAGE_INSTANCE]);
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

    #[test]
    fn attributes_weapon_attempts_and_hits_to_the_effective_pov() {
        let mut ex = Extractor::new();
        ex.on_msg(0.0, &player_info("1;uuid;alice"));
        ex.on_msg(0.0, &player_info("2;uuid;bob"));
        ex.on_msg(0.1, &entity_event(1, EVENT_SET_POV, 0));

        ex.on_msg(1.0, &ammo_usage(1, 5));
        ex.on_msg(1.1, &damage_instance(5, 7));
        ex.on_msg(2.0, &damage_instance(5, 0)); // heal/rejected damage is not a hit

        ex.on_msg(3.0, &entity_event(2, EVENT_SPECTATE_POV, 1));
        ex.on_msg(3.1, &ammo_usage(2, 5));
        ex.on_msg(3.2, &damage_instance(5, 6));
        ex.on_msg(4.0, &entity_event(2, EVENT_SPECTATE_POV, 0));
        ex.on_msg(4.1, &damage_instance(5, 5));

        let out = ex.finish();
        assert_eq!(out.pov_id, Some(1));
        assert_eq!(
            out.weapon_attempts[&1],
            vec![WeaponAttempt { ts: 1.0, weapon: 5 }]
        );
        assert_eq!(
            out.weapon_attempts[&2],
            vec![WeaponAttempt { ts: 3.1, weapon: 5 }]
        );
        assert_eq!(
            out.weapon_hits[&1],
            vec![
                WeaponHit {
                    ts: 1.1,
                    weapon: 5,
                    damage: 7
                },
                WeaponHit {
                    ts: 4.1,
                    weapon: 5,
                    damage: 5
                }
            ]
        );
        assert_eq!(
            out.weapon_hits[&2],
            vec![WeaponHit {
                ts: 3.2,
                weapon: 5,
                damage: 6
            }]
        );
        assert!(out.names_seen.contains("alice"));
        assert!(out.names_seen.contains("bob"));
    }
}
