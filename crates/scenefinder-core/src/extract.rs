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
use crate::format::walk::{
    MSG_ACCURACY, MSG_DAMAGE, MSG_ENTITY_EVENT, MSG_MOVEMENT, MSG_PROPERTY, MSG_TEXT,
};

const PROPERTY_LEN: usize = 25;
const KEY_ASSIGN_TEAM: u8 = 0x08;
const KEY_FLAG_STATE: u8 = 0x09;
const KEY_FRAG_FEED: u8 = 0x0f;
const KEY_ROUND_SCORE: u8 = 0x18;
const KEY_DAMAGE_DEALT: u8 = 0x25;
const TEXT_HEADER_LEN: usize = 12;
const TEXT_PLAYER_INFO: u8 = 0x04;
const ENTITY_EVENT_LEN: usize = 24;
const EVENT_SET_POV_ENTITY: u8 = 0x07;
const EVENT_SIPHONATOR: u8 = 0x87;
const MOVEMENT_LEN: usize = 12;
const DAMAGE_LEN: usize = 35;
const ACCURACY_LEN: usize = 16;
const ACCURACY_SUBTYPE: u8 = 0x01;
const MAX_SPEED_SAMPLE_GAP: f32 = 0.25;
const RING_OUT_WEAPON: u8 = 205;

/// A frag at most this long before a round-score increment is the frag that
/// closed the round.
pub const WIN_FRAG_COUPLING: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpeedSample {
    pub ts: f32,
    /// Server-reported horizontal units/second.
    pub speed: u16,
    /// Time until the next movement update (zero across long gaps).
    pub duration: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeaponDamage {
    pub ts: f32,
    pub weapon: u32,
    /// Actual applied damage (lethal overkill is excluded).
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
    /// killer player id -> (timestamp, killing weapon id)
    #[serde(default)]
    pub frag_weapons: BTreeMap<u32, Vec<(f32, u32)>>,
    /// victim player id -> ring-out/fallout death timestamps.
    #[serde(default)]
    pub fallout_deaths: BTreeMap<u32, Vec<f32>>,
    /// player id -> (timestamp of the round-winning frag, round number)
    pub wins: BTreeMap<u32, Vec<(f32, u32)>>,
    /// player id -> server-reported horizontal movement samples.
    #[serde(default)]
    pub speeds: BTreeMap<u32, Vec<SpeedSample>>,
    /// The recording client's POV entity, set explicitly by entity event 0x07.
    pub pov_id: Option<u32>,
    /// POV hit-confirmation damage with a weapon id.
    pub weapon_damage: Vec<WeaponDamage>,
    /// player id -> weapon id -> (timestamp, game-reported match accuracy 0..1)
    pub weapon_accuracy: BTreeMap<u32, BTreeMap<u32, Vec<(f32, f32)>>>,
    /// player id -> (timestamp, siphonator active). Same-time updates collapse.
    #[serde(default)]
    pub siphonator: BTreeMap<u32, Vec<(f32, bool)>>,
    /// player id -> (timestamp, carrying the flag). Same-time updates collapse.
    #[serde(default)]
    pub flag_carrier: BTreeMap<u32, Vec<(f32, bool)>>,
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
    current_flag_carrier: Option<u32>,
}

impl Extractor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_msg(&mut self, ts: f32, msg: &[u8]) {
        match msg[0] {
            MSG_PROPERTY => self.on_property(ts, msg),
            MSG_ENTITY_EVENT => self.on_entity_event(ts, msg),
            MSG_DAMAGE => self.on_damage(ts, msg),
            MSG_ACCURACY => self.on_accuracy(ts, msg),
            MSG_MOVEMENT => self.on_movement(ts, msg),
            MSG_TEXT => self.on_text(msg),
            _ => {}
        }
    }

    fn mark_seen(&mut self, player: u32) {
        if let Some(name) = self.name_now.get(&player) {
            self.out.names_seen.insert(name.clone());
        }
    }

    fn push_state(stream: &mut Vec<(f32, bool)>, ts: f32, active: bool) {
        if let Some(last) = stream.last_mut().filter(|last| last.0 == ts) {
            last.1 = active;
        } else if stream.last().is_none_or(|last| last.1 != active) {
            stream.push((ts, active));
        }
    }

    fn on_entity_event(&mut self, ts: f32, msg: &[u8]) {
        if msg.len() < ENTITY_EVENT_LEN {
            return;
        }
        let player = u32::from_le_bytes(msg[3..7].try_into().unwrap());
        let key = msg[7];
        let argument = u32::from_le_bytes(msg[8..12].try_into().unwrap());
        match key {
            EVENT_SET_POV_ENTITY => self.out.pov_id = Some(player),
            EVENT_SIPHONATOR => {
                Self::push_state(
                    self.out.siphonator.entry(player).or_default(),
                    ts,
                    argument != 0,
                );
                self.mark_seen(player);
            }
            _ => {}
        }
    }

    fn on_movement(&mut self, ts: f32, msg: &[u8]) {
        if msg.len() < MOVEMENT_LEN || !ts.is_finite() {
            return;
        }
        let player = u32::from_le_bytes(msg[4..8].try_into().unwrap());
        let speed = f32::from_le_bytes(msg[8..12].try_into().unwrap());
        if !speed.is_finite() || speed < 0.0 {
            return;
        }
        let stream = self.out.speeds.entry(player).or_default();
        if let Some(last) = stream.last_mut() {
            if last.ts == ts {
                last.speed = speed.round().clamp(0.0, f32::from(u16::MAX)) as u16;
                return;
            }
            let duration = ts - last.ts;
            if duration > 0.0 && duration <= MAX_SPEED_SAMPLE_GAP {
                last.duration = duration;
            }
        }
        stream.push(SpeedSample {
            ts,
            speed: speed.round().clamp(0.0, f32::from(u16::MAX)) as u16,
            duration: 0.0,
        });
        self.mark_seen(player);
    }

    fn on_damage(&mut self, ts: f32, msg: &[u8]) {
        if msg.len() < DAMAGE_LEN {
            return;
        }
        let damage = u32::from_le_bytes(msg[11..15].try_into().unwrap());
        if damage == 0 {
            return;
        }
        self.out.weapon_damage.push(WeaponDamage {
            ts,
            weapon: u32::from_le_bytes(msg[15..19].try_into().unwrap()),
            damage,
        });
    }

    fn on_accuracy(&mut self, ts: f32, msg: &[u8]) {
        if msg.len() < ACCURACY_LEN || msg[3] != ACCURACY_SUBTYPE {
            return;
        }
        let player = u32::from_le_bytes(msg[4..8].try_into().unwrap());
        let weapon = u32::from_le_bytes(msg[8..12].try_into().unwrap());
        let accuracy = f32::from_le_bytes(msg[12..16].try_into().unwrap());
        if accuracy.is_finite() && (0.0..=1.0).contains(&accuracy) {
            self.out
                .weapon_accuracy
                .entry(player)
                .or_default()
                .entry(weapon)
                .or_default()
                .push((ts, accuracy));
        }
    }

    fn on_property(&mut self, ts: f32, msg: &[u8]) {
        if msg.len() < PROPERTY_LEN {
            return;
        }
        let key = msg[24];
        if !matches!(
            key,
            KEY_ASSIGN_TEAM | KEY_FLAG_STATE | KEY_FRAG_FEED | KEY_ROUND_SCORE | KEY_DAMAGE_DEALT
        ) {
            return;
        }
        let object_id = u32::from_le_bytes(msg[3..7].try_into().unwrap());
        let value = u32::from_le_bytes(msg[11..15].try_into().unwrap());
        // MsgProperty::flags is the single byte at +0x17. For frag_feed this
        // is the killing weapon; value2 at +0x0f is the victim's held weapon.
        let flags = msg[23];
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
            KEY_FLAG_STATE => {
                if value == 1 {
                    if let Some(previous) = self.current_flag_carrier.replace(object_id) {
                        if previous != object_id {
                            Self::push_state(
                                self.out.flag_carrier.entry(previous).or_default(),
                                ts,
                                false,
                            );
                        }
                    }
                    Self::push_state(
                        self.out.flag_carrier.entry(object_id).or_default(),
                        ts,
                        true,
                    );
                } else {
                    Self::push_state(
                        self.out.flag_carrier.entry(object_id).or_default(),
                        ts,
                        false,
                    );
                    if self.current_flag_carrier == Some(object_id) {
                        self.current_flag_carrier = None;
                    }
                }
            }
            KEY_FRAG_FEED => {
                self.out.frags.entry(value).or_default().push(ts);
                self.out
                    .frag_weapons
                    .entry(value)
                    .or_default()
                    .push((ts, u32::from(flags)));
                if flags == RING_OUT_WEAPON {
                    self.out
                        .fallout_deaths
                        .entry(object_id)
                        .or_default()
                        .push(ts);
                }
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

    fn prop_with_flags(key: u8, object_id: u32, value: u32, flags: u32) -> Vec<u8> {
        let mut m = vec![0u8; 25];
        m[0] = 0xDD;
        m[3..7].copy_from_slice(&object_id.to_le_bytes());
        m[11..15].copy_from_slice(&value.to_le_bytes());
        m[23] = flags as u8;
        m[24] = key;
        m
    }

    fn prop(key: u8, object_id: u32, value: u32) -> Vec<u8> {
        prop_with_flags(key, object_id, value, 0)
    }

    fn player_info(payload: &str) -> Vec<u8> {
        let mut m = vec![0xFF, 0, 0, 4];
        m.extend_from_slice(&0u32.to_le_bytes());
        m.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        m.extend_from_slice(payload.as_bytes());
        m
    }

    fn entity_event(player: u32, key: u8, argument: u32) -> Vec<u8> {
        let mut msg = vec![0; ENTITY_EVENT_LEN];
        msg[0] = MSG_ENTITY_EVENT;
        msg[3..7].copy_from_slice(&player.to_le_bytes());
        msg[7] = key;
        msg[8..12].copy_from_slice(&argument.to_le_bytes());
        msg
    }

    fn movement(player: u32, speed: f32) -> Vec<u8> {
        let mut msg = vec![0; MOVEMENT_LEN];
        msg[0] = MSG_MOVEMENT;
        msg[4..8].copy_from_slice(&player.to_le_bytes());
        msg[8..12].copy_from_slice(&speed.to_le_bytes());
        msg
    }

    fn damage(actual: u32, weapon: u32) -> Vec<u8> {
        let mut msg = vec![0; DAMAGE_LEN];
        msg[0] = MSG_DAMAGE;
        msg[11..15].copy_from_slice(&actual.to_le_bytes());
        msg[15..19].copy_from_slice(&weapon.to_le_bytes());
        msg
    }

    fn accuracy(player: u32, weapon: u32, value: f32) -> Vec<u8> {
        let mut msg = vec![0; ACCURACY_LEN];
        msg[0] = MSG_ACCURACY;
        msg[3] = ACCURACY_SUBTYPE;
        msg[4..8].copy_from_slice(&player.to_le_bytes());
        msg[8..12].copy_from_slice(&weapon.to_le_bytes());
        msg[12..16].copy_from_slice(&value.to_le_bytes());
        msg
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
        ex.on_msg(6.0, &prop_with_flags(0x0f, 2, 1, 4)); // alice kills bob
        ex.on_msg(6.05, &prop(0x18, 0, 1)); // first score seen: not an increment
        ex.on_msg(20.0, &prop(0x0f, 2, 1));
        ex.on_msg(20.05, &prop(0x18, 0, 2)); // increment within 0.1 s: alice wins round 2
        ex.on_msg(30.0, &prop(0x0f, 1, 2)); // bob kills alice
        ex.on_msg(30.5, &prop(0x18, 1, 1)); // too late after the frag
        ex.on_msg(40.0, &prop(0x0f, 1, 1)); // suicide
        ex.on_msg(40.01, &prop(0x18, 0, 3));
        ex.on_msg(50.0, &prop_with_flags(0x0f, 2, 1, 205)); // bob falls out
        let out = ex.finish();
        assert_eq!(
            out.names,
            vec![
                (1, "alice".to_string()),
                (2, "bob;with;semicolons".to_string())
            ]
        );
        assert_eq!(out.dmg[&1], vec![(5.0, 100), (6.0, 180)]);
        assert_eq!(out.frags[&1], vec![6.0, 20.0, 40.0, 50.0]);
        assert_eq!(out.frag_weapons[&1][0], (6.0, 4));
        assert_eq!(out.fallout_deaths[&2], vec![50.0]);
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

    #[test]
    fn extracts_verified_weapon_movement_and_state_metrics() {
        let mut ex = Extractor::new();
        ex.on_msg(0.0, &player_info("7;uuid;pov"));
        ex.on_msg(0.1, &entity_event(7, EVENT_SET_POV_ENTITY, 0));
        ex.on_msg(1.0, &damage(73, 4));
        ex.on_msg(1.1, &accuracy(7, 4, 0.375));
        ex.on_msg(2.0, &movement(7, 1_200.0));
        ex.on_msg(2.05, &movement(7, 9_407.2));
        ex.on_msg(2.10, &movement(7, 800.0));
        // The round-reset enable+disable pair at one timestamp collapses to
        // inactive and therefore cannot create a fake active interval.
        ex.on_msg(3.0, &entity_event(7, EVENT_SIPHONATOR, 1));
        ex.on_msg(3.0, &entity_event(7, EVENT_SIPHONATOR, 0));
        ex.on_msg(4.0, &entity_event(7, EVENT_SIPHONATOR, 1));
        ex.on_msg(5.0, &prop(KEY_FLAG_STATE, 7, 1));
        ex.on_msg(6.0, &prop(KEY_FLAG_STATE, 7, 2));

        let out = ex.finish();
        assert_eq!(out.pov_id, Some(7));
        assert_eq!(
            out.weapon_damage,
            vec![WeaponDamage {
                ts: 1.0,
                weapon: 4,
                damage: 73
            }]
        );
        assert_eq!(out.weapon_accuracy[&7][&4], vec![(1.1, 0.375)]);
        assert_eq!(out.speeds[&7].len(), 3);
        assert_eq!(out.speeds[&7][1].speed, 9_407);
        assert!((out.speeds[&7][0].duration - 0.05).abs() < 0.001);
        assert_eq!(out.siphonator[&7], vec![(3.0, false), (4.0, true)]);
        assert_eq!(out.flag_carrier[&7], vec![(5.0, true), (6.0, false)]);
    }
}
