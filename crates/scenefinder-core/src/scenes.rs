//! The scene search: sliding damage/frag windows over one player's events.

use std::collections::BTreeMap;

use crate::extract::{SpeedSample, WeaponDamage};
use crate::params::{RuleQuery, SearchRule};
use crate::pyset::PySet;

/// With a round-winning frag required, the round may close this many seconds
/// after the window ends.
pub const WIN_GRACE: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    pub ts: f64,
    pub dmg: u64,
    pub frags: u64,
}

/// Turns a cumulative (ts, total) series into (ts, damage-dealt-now) events.
/// The running total only grows, so a non-increasing sample is treated as a
/// counter reset (a fresh life or round) and re-anchored rather than counted
/// as a burst.
pub fn deltas(samples: &[(f32, u32)]) -> Vec<(f64, u64)> {
    let mut out = Vec::with_capacity(samples.len());
    let mut prev: u64 = 0;
    for &(ts, total) in samples {
        let total = u64::from(total);
        let delta = if total >= prev { total - prev } else { total };
        if delta > 0 {
            out.push((f64::from(ts), delta));
        }
        prev = total;
    }
    out
}

/// Merges damage deltas and kill times into one sorted event list.
pub fn events(deltas: Vec<(f64, u64)>, frag_times: &[f32]) -> Vec<Event> {
    let mut ev: Vec<Event> = deltas
        .into_iter()
        .map(|(ts, dmg)| Event { ts, dmg, frags: 0 })
        .collect();
    ev.extend(frag_times.iter().map(|&ts| Event {
        ts: f64::from(ts),
        dmg: 0,
        frags: 1,
    }));
    ev.sort_by(|a, b| {
        a.ts.total_cmp(&b.ts)
            .then(a.dmg.cmp(&b.dmg))
            .then(a.frags.cmp(&b.frags))
    });
    ev
}

#[derive(Debug, Clone, Copy)]
pub struct Criteria {
    pub dmg: Option<u64>,
    pub frags: Option<u64>,
    /// Both thresholds must hold (AND) rather than either (OR).
    pub and: bool,
}

impl Criteria {
    /// With no threshold every window qualifies.
    pub fn qualifies(&self, d: u64, f: u64) -> bool {
        match (self.dmg, self.frags) {
            (None, None) => true,
            (Some(dt), None) => d >= dt,
            (None, Some(ft)) => f >= ft,
            (Some(dt), Some(ft)) => {
                if self.and {
                    d >= dt && f >= ft
                } else {
                    d >= dt || f >= ft
                }
            }
        }
    }

    /// Ranks windows: by damage when a damage threshold is set, else by frags.
    pub fn score(&self, d: u64, f: u64) -> (u64, u64) {
        if self.dmg.is_some() {
            (d, f)
        } else {
            (f, d)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub start: f64,
    pub damage: Option<u64>,
    pub frags: Option<u64>,
    pub round: Option<u32>,
}

/// One hit per qualifying stretch, reported at its single best window.
///
/// A window of `window` seconds is slid over every event (each as a start).
/// Overlapping qualifying windows form one stretch, reported once at its
/// peak; reporting the peak (not the first window to cross the line) keeps
/// results monotonic in the thresholds.
pub fn scenes(ev: &[Event], window: f64, c: &Criteria) -> Vec<Hit> {
    let n = ev.len();
    let mut peaks: Vec<((u64, u64), f64, u64, u64)> = Vec::new();
    let mut best: Option<((u64, u64), f64, u64, u64)> = None;
    let mut prev_start = 0.0f64;
    let mut j = 0usize;
    let mut dsum = 0u64;
    let mut fsum = 0u64;
    for i in 0..n {
        let end = ev[i].ts + window;
        while j < n && ev[j].ts <= end {
            dsum += ev[j].dmg;
            fsum += ev[j].frags;
            j += 1;
        }
        if c.qualifies(dsum, fsum) {
            if let Some(b) = best {
                if ev[i].ts - prev_start > window {
                    peaks.push(b);
                    best = None;
                }
            }
            let s = c.score(dsum, fsum);
            if best.is_none_or(|b| s > b.0) {
                best = Some((s, ev[i].ts, dsum, fsum));
            }
            prev_start = ev[i].ts;
        }
        dsum -= ev[i].dmg;
        fsum -= ev[i].frags;
    }
    if let Some(b) = best {
        peaks.push(b);
    }
    peaks
        .into_iter()
        .map(|(_, start, d, f)| Hit {
            start,
            damage: Some(d),
            frags: Some(f),
            round: None,
        })
        .collect()
}

/// One hit per round-winning frag whose surrounding window meets the
/// thresholds. Candidate windows contain the frag or end at most `WIN_GRACE`
/// seconds before it, starting at an event or exactly `window` (plus the
/// grace) before the frag. Without a window the frag itself is the hit.
///
/// Equal-scoring candidates are resolved the way the reference tool does:
/// the first one in CPython set iteration order wins (see `pyset`).
pub fn win_scenes(
    ev: &[Event],
    wins: &[(f32, u32)],
    window: Option<f64>,
    c: &Criteria,
) -> Vec<Hit> {
    let times: Vec<f64> = ev.iter().map(|e| e.ts).collect();
    let mut pd = Vec::with_capacity(ev.len() + 1);
    let mut pf = Vec::with_capacity(ev.len() + 1);
    pd.push(0u64);
    pf.push(0u64);
    for e in ev {
        pd.push(pd.last().unwrap() + e.dmg);
        pf.push(pf.last().unwrap() + e.frags);
    }
    let lower = |x: f64| times.partition_point(|t| *t < x);
    let upper = |x: f64| times.partition_point(|t| *t <= x);
    let sums = |s: f64, e: f64| {
        let (lo, hi) = (lower(s), upper(e));
        (pd[hi] - pd[lo], pf[hi] - pf[lo])
    };

    let mut out = Vec::new();
    for &(w, round_no) in wins {
        let w = f64::from(w);
        let Some(window) = window else {
            out.push(Hit {
                start: w,
                damage: None,
                frags: None,
                round: Some(round_no),
            });
            continue;
        };
        let earliest = w - window - WIN_GRACE;
        let mut starts = PySet::new();
        starts.add(w - window);
        starts.add(earliest);
        for &t in &times[lower(earliest)..upper(w)] {
            starts.add(t);
        }
        let mut best: Option<((u64, u64), f64, u64, u64)> = None;
        for s in starts.iter() {
            let (d, f) = sums(s, s + window);
            if c.qualifies(d, f) {
                let sc = c.score(d, f);
                if best.is_none_or(|b| sc > b.0) {
                    best = Some((sc, s, d, f));
                }
            }
        }
        if let Some((_, s, d, f)) = best {
            out.push(Hit {
                start: s,
                damage: Some(d),
                frags: Some(f),
                round: Some(round_no),
            });
        }
    }
    out
}

#[derive(Debug, Clone, Copy)]
pub struct AdvancedCriteria {
    pub dmg: Option<u64>,
    pub frags: Option<u64>,
    pub speed: Option<u64>,
    pub speed_duration: f64,
    /// Game-reported match-to-date accuracy, in percent.
    pub accuracy: Option<f64>,
    pub and: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedHit {
    pub start: f64,
    pub damage: u64,
    pub frags: u64,
    pub speed: Option<u64>,
    pub speed_duration: Option<f64>,
    pub accuracy: Option<f64>,
    pub round: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
struct Metrics {
    damage: u64,
    frags: u64,
    speed: u64,
    speed_duration: f64,
    accuracy: Option<f64>,
}

impl AdvancedCriteria {
    fn qualifies(&self, m: &Metrics) -> bool {
        let mut rules = Vec::with_capacity(4);
        if let Some(threshold) = self.dmg {
            rules.push(m.damage >= threshold);
        }
        if let Some(threshold) = self.frags {
            rules.push(m.frags >= threshold);
        }
        if let Some(threshold) = self.speed {
            rules.push(
                m.speed >= threshold && m.speed_duration + f64::EPSILON >= self.speed_duration,
            );
        }
        if let Some(threshold) = self.accuracy {
            rules.push(m.accuracy.is_some_and(|value| value >= threshold));
        }
        if rules.is_empty() {
            true
        } else if self.and {
            rules.into_iter().all(|v| v)
        } else {
            rules.into_iter().any(|v| v)
        }
    }

    fn score(&self, m: &Metrics) -> [u64; 5] {
        let accuracy = (m.accuracy.unwrap_or(0.0) * 1_000.0).round() as u64;
        let duration = (m.speed_duration * 1_000.0).round() as u64;
        if self.dmg.is_some() {
            [m.damage, m.frags, m.speed, duration, accuracy]
        } else if self.frags.is_some() {
            [m.frags, m.damage, m.speed, duration, accuracy]
        } else if self.speed.is_some() {
            [m.speed, duration, m.damage, m.frags, accuracy]
        } else {
            [accuracy, m.damage, m.frags, m.speed, duration]
        }
    }
}

struct MetricIndex<'a> {
    event_times: Vec<f64>,
    damage_prefix: Vec<u64>,
    frag_prefix: Vec<u64>,
    speed_times: Vec<f64>,
    speed_duration_prefix: Vec<f64>,
    speed_tree: RangeMax,
    accuracy: &'a [(f32, f32)],
    speed_threshold: Option<u64>,
}

impl<'a> MetricIndex<'a> {
    fn new(
        events: &[Event],
        speeds: &'a [SpeedSample],
        accuracy: &'a [(f32, f32)],
        speed_threshold: Option<u64>,
    ) -> Self {
        let mut event_times = Vec::with_capacity(events.len());
        let mut damage_prefix = vec![0];
        let mut frag_prefix = vec![0];
        for event in events {
            event_times.push(event.ts);
            damage_prefix.push(damage_prefix.last().unwrap() + event.dmg);
            frag_prefix.push(frag_prefix.last().unwrap() + event.frags);
        }
        let speed_times: Vec<f64> = speeds.iter().map(|s| f64::from(s.ts)).collect();
        let mut speed_duration_prefix = Vec::with_capacity(speeds.len() + 1);
        speed_duration_prefix.push(0.0);
        for sample in speeds {
            let duration = if speed_threshold.is_some_and(|t| u64::from(sample.speed) >= t) {
                f64::from(sample.duration)
            } else {
                0.0
            };
            speed_duration_prefix.push(speed_duration_prefix.last().unwrap() + duration);
        }
        MetricIndex {
            event_times,
            damage_prefix,
            frag_prefix,
            speed_times,
            speed_duration_prefix,
            speed_tree: RangeMax::new(speeds.iter().map(|s| u64::from(s.speed))),
            accuracy,
            speed_threshold,
        }
    }

    fn metrics(&self, start: f64, end: f64) -> Metrics {
        let elo = self.event_times.partition_point(|ts| *ts < start);
        let ehi = self.event_times.partition_point(|ts| *ts <= end);
        let slo = self.speed_times.partition_point(|ts| *ts < start);
        let shi = self.speed_times.partition_point(|ts| *ts <= end);
        let ai = self
            .accuracy
            .partition_point(|(ts, _)| f64::from(*ts) <= end);
        Metrics {
            damage: self.damage_prefix[ehi] - self.damage_prefix[elo],
            frags: self.frag_prefix[ehi] - self.frag_prefix[elo],
            speed: self.speed_tree.query(slo, shi),
            speed_duration: if self.speed_threshold.is_some() {
                self.speed_duration_prefix[shi] - self.speed_duration_prefix[slo]
            } else {
                0.0
            },
            accuracy: ai
                .checked_sub(1)
                .map(|i| f64::from(self.accuracy[i].1) * 100.0),
        }
    }

    fn all_times(&self) -> Vec<f64> {
        let mut times = self.event_times.clone();
        times.extend(self.speed_times.iter().copied());
        times.extend(self.accuracy.iter().map(|(ts, _)| f64::from(*ts)));
        times.sort_by(f64::total_cmp);
        times.dedup_by(|a, b| a.total_cmp(b).is_eq());
        times
    }
}

struct RangeMax {
    size: usize,
    tree: Vec<u64>,
}

impl RangeMax {
    fn new(values: impl IntoIterator<Item = u64>) -> Self {
        let values: Vec<u64> = values.into_iter().collect();
        let size = values.len().next_power_of_two().max(1);
        let mut tree = vec![0; size * 2];
        tree[size..size + values.len()].copy_from_slice(&values);
        for i in (1..size).rev() {
            tree[i] = tree[i * 2].max(tree[i * 2 + 1]);
        }
        RangeMax { size, tree }
    }

    fn query(&self, mut left: usize, mut right: usize) -> u64 {
        left += self.size;
        right += self.size;
        let mut max = 0;
        while left < right {
            if left % 2 == 1 {
                max = max.max(self.tree[left]);
                left += 1;
            }
            if right % 2 == 1 {
                right -= 1;
                max = max.max(self.tree[right]);
            }
            left /= 2;
            right /= 2;
        }
        max
    }
}

fn advanced_hit(start: f64, metrics: Metrics, round: Option<u32>) -> AdvancedHit {
    AdvancedHit {
        start,
        damage: metrics.damage,
        frags: metrics.frags,
        speed: (metrics.speed > 0).then_some(metrics.speed),
        speed_duration: (metrics.speed_duration > 0.0).then_some(metrics.speed_duration),
        accuracy: metrics.accuracy,
        round,
    }
}

/// General metric search used when speed, a weapon, or reported accuracy is
/// requested. It keeps the legacy damage/frag search untouched for exact
/// compatibility with the reference scene finder.
pub fn advanced_scenes(
    events: &[Event],
    speeds: &[SpeedSample],
    accuracy: &[(f32, f32)],
    window: f64,
    criteria: &AdvancedCriteria,
) -> Vec<AdvancedHit> {
    let index = MetricIndex::new(events, speeds, accuracy, criteria.speed);
    let mut peaks: Vec<([u64; 5], f64, Metrics)> = Vec::new();
    let mut best: Option<([u64; 5], f64, Metrics)> = None;
    let mut previous = 0.0;
    for start in index.all_times() {
        let metrics = index.metrics(start, start + window);
        if criteria.qualifies(&metrics) {
            if let Some(peak) = best {
                if start - previous > window {
                    peaks.push(peak);
                    best = None;
                }
            }
            let score = criteria.score(&metrics);
            if best.is_none_or(|current| score > current.0) {
                best = Some((score, start, metrics));
            }
            previous = start;
        }
    }
    if let Some(peak) = best {
        peaks.push(peak);
    }
    peaks
        .into_iter()
        .map(|(_, start, metrics)| advanced_hit(start, metrics, None))
        .collect()
}

pub fn advanced_win_scenes(
    events: &[Event],
    speeds: &[SpeedSample],
    accuracy: &[(f32, f32)],
    wins: &[(f32, u32)],
    window: f64,
    criteria: &AdvancedCriteria,
) -> Vec<AdvancedHit> {
    let index = MetricIndex::new(events, speeds, accuracy, criteria.speed);
    let all_times = index.all_times();
    let mut hits = Vec::new();
    for &(win, round) in wins {
        let win = f64::from(win);
        let earliest = win - window - WIN_GRACE;
        let mut candidates = vec![win - window, earliest];
        let lo = all_times.partition_point(|ts| *ts < earliest);
        let hi = all_times.partition_point(|ts| *ts <= win);
        candidates.extend_from_slice(&all_times[lo..hi]);
        let mut best: Option<([u64; 5], f64, Metrics)> = None;
        for start in candidates {
            let metrics = index.metrics(start, start + window);
            if criteria.qualifies(&metrics) {
                let score = criteria.score(&metrics);
                if best.is_none_or(|current| score > current.0) {
                    best = Some((score, start, metrics));
                }
            }
        }
        if let Some((_, start, metrics)) = best {
            hits.push(advanced_hit(start, metrics, Some(round)));
        }
    }
    hits
}

/// All streams used by the flexible GUI rule query for one player.
pub struct QueryStreams<'a> {
    pub damage: &'a [(f64, u64)],
    pub weapon_damage: &'a [WeaponDamage],
    pub frags: &'a [(f32, u32)],
    pub speeds: &'a [SpeedSample],
    pub accuracy: &'a BTreeMap<u32, Vec<(f32, f32)>>,
    pub wins: &'a [(f32, u32)],
    pub siphonator: &'a [(f32, bool)],
    pub flag_carrier: &'a [(f32, bool)],
    pub fallout_deaths: &'a [f32],
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryMetric {
    pub rule_id: String,
    pub label: String,
    pub value: f64,
    pub secondary: Option<f64>,
    pub display: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryHit {
    pub start: f64,
    pub damage: u64,
    pub frags: u64,
    pub speed: Option<u64>,
    pub round: Option<u32>,
    pub criteria: Vec<QueryMetric>,
}

#[derive(Default)]
struct SumIndex {
    times: Vec<f64>,
    prefix: Vec<u64>,
}

impl SumIndex {
    fn new(mut values: Vec<(f64, u64)>) -> Self {
        values.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out = SumIndex {
            times: Vec::with_capacity(values.len()),
            prefix: vec![0],
        };
        for (time, value) in values {
            out.times.push(time);
            out.prefix
                .push(out.prefix.last().copied().unwrap_or(0) + value);
        }
        out
    }

    fn sum(&self, start: f64, end: f64) -> u64 {
        let lo = self.times.partition_point(|time| *time < start);
        let hi = self.times.partition_point(|time| *time <= end);
        self.prefix[hi] - self.prefix[lo]
    }
}

struct QueryIndex<'a> {
    damage: SumIndex,
    weapon_damage: BTreeMap<u32, SumIndex>,
    frags: SumIndex,
    weapon_frags: BTreeMap<u32, SumIndex>,
    speed_times: Vec<f64>,
    speeds: &'a [SpeedSample],
    speed_max: RangeMax,
    accuracy: &'a BTreeMap<u32, Vec<(f32, f32)>>,
    wins: &'a [(f32, u32)],
    siphonator: &'a [(f32, bool)],
    flag_carrier: &'a [(f32, bool)],
    fallout_deaths: SumIndex,
}

impl<'a> QueryIndex<'a> {
    fn new(streams: &'a QueryStreams<'a>) -> Self {
        let mut weapon_damage: BTreeMap<u32, Vec<(f64, u64)>> = BTreeMap::new();
        for event in streams.weapon_damage {
            weapon_damage
                .entry(event.weapon)
                .or_default()
                .push((f64::from(event.ts), u64::from(event.damage)));
        }
        let mut weapon_frags: BTreeMap<u32, Vec<(f64, u64)>> = BTreeMap::new();
        for &(time, weapon) in streams.frags {
            weapon_frags
                .entry(weapon)
                .or_default()
                .push((f64::from(time), 1));
        }
        QueryIndex {
            damage: SumIndex::new(streams.damage.to_vec()),
            weapon_damage: weapon_damage
                .into_iter()
                .map(|(weapon, values)| (weapon, SumIndex::new(values)))
                .collect(),
            frags: SumIndex::new(
                streams
                    .frags
                    .iter()
                    .map(|(time, _)| (f64::from(*time), 1))
                    .collect(),
            ),
            weapon_frags: weapon_frags
                .into_iter()
                .map(|(weapon, values)| (weapon, SumIndex::new(values)))
                .collect(),
            speed_times: streams
                .speeds
                .iter()
                .map(|sample| f64::from(sample.ts))
                .collect(),
            speeds: streams.speeds,
            speed_max: RangeMax::new(streams.speeds.iter().map(|sample| u64::from(sample.speed))),
            accuracy: streams.accuracy,
            wins: streams.wins,
            siphonator: streams.siphonator,
            flag_carrier: streams.flag_carrier,
            fallout_deaths: SumIndex::new(
                streams
                    .fallout_deaths
                    .iter()
                    .map(|time| (f64::from(*time), 1))
                    .collect(),
            ),
        }
    }

    fn speed(&self, start: f64, end: f64, threshold: u64) -> (u64, f64) {
        let mut lo = self.speed_times.partition_point(|time| *time < start);
        if lo > 0 {
            let previous = &self.speeds[lo - 1];
            if f64::from(previous.ts + previous.duration) > start {
                lo -= 1;
            }
        }
        let hi = self.speed_times.partition_point(|time| *time <= end);
        let duration = self.speeds[lo..hi]
            .iter()
            .filter(|sample| u64::from(sample.speed) >= threshold)
            .map(|sample| {
                let sample_start = f64::from(sample.ts).max(start);
                let sample_end = f64::from(sample.ts + sample.duration).min(end);
                (sample_end - sample_start).max(0.0)
            })
            .sum();
        (self.speed_max.query(lo, hi), duration)
    }

    fn accuracy(&self, weapon: u32, end: f64) -> Option<f64> {
        let values = self.accuracy.get(&weapon)?;
        let index = values.partition_point(|(time, _)| f64::from(*time) <= end);
        index.checked_sub(1).map(|i| f64::from(values[i].1) * 100.0)
    }

    fn win(&self, start: f64, end: f64) -> Option<u32> {
        self.wins
            .iter()
            .find(|(time, _)| (start..=end).contains(&f64::from(*time)))
            .map(|(_, round)| *round)
    }
}

fn state_duration(states: &[(f32, bool)], start: f64, end: f64) -> f64 {
    let mut active = states
        .iter()
        .take_while(|(time, _)| f64::from(*time) <= start)
        .last()
        .is_some_and(|(_, active)| *active);
    let mut cursor = start;
    let mut duration = 0.0;
    for &(time, next) in states {
        let time = f64::from(time);
        if time <= start || time > end {
            continue;
        }
        if active {
            duration += time - cursor;
        }
        active = next;
        cursor = time;
    }
    if active {
        duration += end - cursor;
    }
    duration.max(0.0)
}

fn query_metric(
    index: &QueryIndex<'_>,
    rule: &SearchRule,
    start: f64,
    end: f64,
) -> (bool, QueryMetric, f64) {
    let (value, secondary, qualified, ratio, display) = match rule {
        SearchRule::Damage {
            minimum, weapon, ..
        } => {
            let value = weapon
                .and_then(|weapon| index.weapon_damage.get(&weapon))
                .map_or_else(
                    || {
                        if weapon.is_some() {
                            0
                        } else {
                            index.damage.sum(start, end)
                        }
                    },
                    |i| i.sum(start, end),
                );
            (
                value as f64,
                None,
                value >= *minimum,
                value as f64 / (*minimum).max(1) as f64,
                value.to_string(),
            )
        }
        SearchRule::Frags {
            minimum, weapon, ..
        } => {
            let value = weapon
                .and_then(|weapon| index.weapon_frags.get(&weapon))
                .map_or_else(
                    || {
                        if weapon.is_some() {
                            0
                        } else {
                            index.frags.sum(start, end)
                        }
                    },
                    |i| i.sum(start, end),
                );
            (
                value as f64,
                None,
                value >= *minimum,
                value as f64 / (*minimum).max(1) as f64,
                value.to_string(),
            )
        }
        SearchRule::Speed {
            minimum, duration, ..
        } => {
            let (speed, actual_duration) = index.speed(start, end, *minimum);
            let qualified = speed >= *minimum && actual_duration + f64::EPSILON >= *duration;
            let duration_ratio = if *duration > 0.0 {
                actual_duration / duration
            } else {
                1.0
            };
            (
                speed as f64,
                Some(actual_duration),
                qualified,
                speed as f64 / (*minimum).max(1) as f64 + duration_ratio,
                format!("{speed} / {actual_duration:.1}s"),
            )
        }
        SearchRule::Accuracy {
            minimum, weapon, ..
        } => {
            let value = index.accuracy(*weapon, end).unwrap_or(0.0);
            (
                value,
                None,
                value >= *minimum,
                value / minimum.max(1.0),
                format!("{value:.1}%"),
            )
        }
        SearchRule::Siphonator { .. } => {
            let value = state_duration(index.siphonator, start, end);
            (
                value,
                None,
                value > 0.0,
                (value > 0.0) as u8 as f64,
                if value > 0.0 {
                    format!("{value:.1}s")
                } else {
                    "no".into()
                },
            )
        }
        SearchRule::FlagCarrier { .. } => {
            let value = state_duration(index.flag_carrier, start, end);
            (
                value,
                None,
                value > 0.0,
                (value > 0.0) as u8 as f64,
                if value > 0.0 {
                    format!("{value:.1}s")
                } else {
                    "no".into()
                },
            )
        }
        SearchRule::RoundWin { .. } => {
            let won = index.win(start, end).is_some();
            (
                won as u8 as f64,
                None,
                won,
                won as u8 as f64,
                if won { "yes".into() } else { "no".into() },
            )
        }
        SearchRule::FalloutDeath { exclude, .. } => {
            let count = index.fallout_deaths.sum(start, end);
            let qualified = if *exclude { count == 0 } else { count > 0 };
            (
                if *exclude {
                    -(count as f64)
                } else {
                    count as f64
                },
                None,
                qualified,
                qualified as u8 as f64,
                if count == 0 {
                    "none".into()
                } else if count == 1 {
                    "1 fallout".into()
                } else {
                    format!("{count} fallouts")
                },
            )
        }
    };
    (
        qualified,
        QueryMetric {
            rule_id: rule.id().to_string(),
            label: rule.description(),
            value,
            secondary,
            display,
        },
        ratio.min(10.0),
    )
}

/// Runs a two-level boolean query: each group combines its own rules, then
/// the root condition combines the groups. One peak is kept per overlapping
/// qualifying stretch.
pub fn query_scenes(streams: &QueryStreams<'_>, query: &RuleQuery, window: f64) -> Vec<QueryHit> {
    let index = QueryIndex::new(streams);
    let mut candidates = Vec::new();
    for rule in query.groups.iter().flat_map(|group| &group.rules) {
        match rule {
            SearchRule::Damage { weapon, .. } => match weapon {
                Some(weapon) => candidates.extend(
                    index
                        .weapon_damage
                        .get(weapon)
                        .into_iter()
                        .flat_map(|i| i.times.iter().copied()),
                ),
                None => candidates.extend(index.damage.times.iter().copied()),
            },
            SearchRule::Frags { weapon, .. } => match weapon {
                Some(weapon) => candidates.extend(
                    index
                        .weapon_frags
                        .get(weapon)
                        .into_iter()
                        .flat_map(|i| i.times.iter().copied()),
                ),
                None => candidates.extend(index.frags.times.iter().copied()),
            },
            SearchRule::Speed { .. } => candidates.extend(index.speed_times.iter().copied()),
            SearchRule::Accuracy { weapon, .. } => candidates.extend(
                index
                    .accuracy
                    .get(weapon)
                    .into_iter()
                    .flat_map(|v| v.iter().map(|(t, _)| f64::from(*t))),
            ),
            SearchRule::Siphonator { .. } => {
                candidates.extend(index.siphonator.iter().map(|(t, _)| f64::from(*t)))
            }
            SearchRule::FlagCarrier { .. } => {
                candidates.extend(index.flag_carrier.iter().map(|(t, _)| f64::from(*t)))
            }
            SearchRule::RoundWin { .. } => {
                candidates.extend(index.wins.iter().map(|(t, _)| f64::from(*t)))
            }
            SearchRule::FalloutDeath { exclude, .. } => {
                if !exclude {
                    candidates.extend(index.fallout_deaths.times.iter().copied());
                }
            }
        }
    }
    candidates.sort_by(f64::total_cmp);
    candidates.dedup_by(|a, b| a.total_cmp(b).is_eq());

    let mut peaks = Vec::new();
    let mut best: Option<(f64, QueryHit)> = None;
    let mut previous = 0.0;
    for start in candidates {
        let end = start + window;
        let mut metrics = Vec::new();
        let mut score = 0.0;
        let group_results: Vec<bool> = query
            .groups
            .iter()
            .map(|group| {
                let results: Vec<bool> = group
                    .rules
                    .iter()
                    .map(|rule| {
                        let (qualified, metric, ratio) = query_metric(&index, rule, start, end);
                        metrics.push(metric);
                        score += ratio;
                        qualified
                    })
                    .collect();
                group.condition.combine(results)
            })
            .collect();
        if query.condition.combine(group_results) {
            if let Some((_, hit)) = &best {
                if start - previous > window {
                    peaks.push(best.take().unwrap().1);
                } else {
                    let _ = hit;
                }
            }
            let (max_speed, _) = index.speed(start, end, 0);
            let hit = QueryHit {
                start,
                damage: index.damage.sum(start, end),
                frags: index.frags.sum(start, end),
                speed: (max_speed > 0).then_some(max_speed),
                round: index.win(start, end),
                criteria: metrics,
            };
            if best
                .as_ref()
                .is_none_or(|(best_score, _)| score.total_cmp(best_score).is_gt())
            {
                best = Some((score, hit));
            }
            previous = start;
        }
    }
    if let Some((_, hit)) = best {
        peaks.push(hit);
    }
    peaks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Condition, RuleGroup, RuleQuery, SearchRule};

    #[test]
    fn deltas_reset_semantics() {
        let d = deltas(&[(1.0, 100), (2.0, 150), (3.0, 40), (4.0, 90), (5.0, 90)]);
        assert_eq!(d, vec![(1.0, 100), (2.0, 50), (3.0, 40), (4.0, 50)]);
    }

    fn ev(list: &[(f64, u64, u64)]) -> Vec<Event> {
        list.iter()
            .map(|&(ts, dmg, frags)| Event { ts, dmg, frags })
            .collect()
    }

    #[test]
    fn peak_per_stretch_and_gap_split() {
        let e = ev(&[
            (0.0, 100, 0),
            (1.0, 100, 0),
            (2.0, 150, 0),
            (3.0, 10, 0),
            (20.0, 300, 0),
            (21.0, 5, 1),
        ]);
        let c = Criteria {
            dmg: Some(200),
            frags: None,
            and: true,
        };
        let hits = scenes(&e, 2.0, &c);
        assert_eq!(hits.len(), 2);
        assert_eq!(
            hits[0],
            Hit {
                start: 0.0,
                damage: Some(350),
                frags: Some(0),
                round: None
            }
        );
        assert_eq!(
            hits[1],
            Hit {
                start: 20.0,
                damage: Some(305),
                frags: Some(1),
                round: None
            }
        );
    }

    #[test]
    fn and_or_and_frag_ranking() {
        let e = ev(&[(0.0, 50, 0), (0.5, 0, 1), (1.0, 0, 1), (30.0, 500, 0)]);
        let both_and = Criteria {
            dmg: Some(100),
            frags: Some(2),
            and: true,
        };
        assert!(scenes(&e, 5.0, &both_and).is_empty());
        let both_or = Criteria {
            dmg: Some(100),
            frags: Some(2),
            and: false,
        };
        let hits = scenes(&e, 5.0, &both_or);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].start, 0.0);
        assert_eq!(hits[1].damage, Some(500));
        let frags_only = Criteria {
            dmg: None,
            frags: Some(2),
            and: true,
        };
        let hits = scenes(&e, 5.0, &frags_only);
        assert_eq!(
            hits,
            vec![Hit {
                start: 0.0,
                damage: Some(50),
                frags: Some(2),
                round: None
            }]
        );
    }

    #[test]
    fn win_grace_and_tie_rule() {
        let e = ev(&[(10.0, 100, 0), (11.0, 150, 0), (12.0, 0, 1)]);
        let c = Criteria {
            dmg: Some(200),
            frags: None,
            and: true,
        };
        assert_eq!(
            win_scenes(&e, &[(12.8, 3)], Some(2.0), &c),
            vec![Hit {
                start: 10.0,
                damage: Some(250),
                frags: Some(1),
                round: Some(3)
            }]
        );
        assert!(win_scenes(&e, &[(14.5, 3)], Some(2.0), &c).is_empty());
        assert_eq!(
            win_scenes(&e, &[(12.8, 3)], None, &c),
            vec![Hit {
                start: f64::from(12.8f32),
                damage: None,
                frags: None,
                round: Some(3)
            }]
        );
        let any = Criteria {
            dmg: None,
            frags: None,
            and: true,
        };
        let hits = win_scenes(&e, &[(12.0, 1)], Some(2.0), &any);
        assert_eq!(hits[0].start, 10.0);
        assert_eq!(hits[0].frags, Some(1));
        let tie = ev(&[(0.5, 100, 0), (5.0, 0, 1)]);
        let hits = win_scenes(&tie, &[(5.0, 1)], Some(5.0), &any);
        assert_eq!(hits[0].start, 0.0);
    }

    #[test]
    fn advanced_metrics_combine_speed_duration_and_accuracy() {
        let events = ev(&[(0.0, 100, 0), (0.4, 100, 1)]);
        let speeds = vec![
            SpeedSample {
                ts: 0.1,
                speed: 1_200,
                duration: 0.05,
            },
            SpeedSample {
                ts: 0.2,
                speed: 1_300,
                duration: 0.05,
            },
            SpeedSample {
                ts: 0.3,
                speed: 800,
                duration: 0.05,
            },
        ];
        let accuracy = vec![(0.0, 0.4), (0.25, 0.6)];
        let criteria = AdvancedCriteria {
            dmg: Some(200),
            frags: Some(1),
            speed: Some(1_000),
            speed_duration: 0.1,
            accuracy: Some(50.0),
            and: true,
        };
        let hits = advanced_scenes(&events, &speeds, &accuracy, 0.5, &criteria);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].damage, 200);
        assert_eq!(hits[0].frags, 1);
        assert_eq!(hits[0].speed, Some(1_300));
        assert!((hits[0].speed_duration.unwrap() - 0.1).abs() < 1e-6);
        assert!((hits[0].accuracy.unwrap() - 60.0).abs() < 0.001);

        let too_long = AdvancedCriteria {
            speed_duration: 0.11,
            ..criteria
        };
        assert!(advanced_scenes(&events, &speeds, &accuracy, 0.5, &too_long).is_empty());
    }

    #[test]
    fn grouped_query_combines_weapon_speed_and_state_rules() {
        let damage = vec![(0.0, 200), (10.0, 50)];
        let weapon_damage = vec![WeaponDamage {
            ts: 0.0,
            weapon: 4,
            damage: 150,
        }];
        let frags = vec![(10.2, 7)];
        let speeds = vec![
            SpeedSample {
                ts: 0.1,
                speed: 1_100,
                duration: 0.2,
            },
            SpeedSample {
                ts: 10.1,
                speed: 500,
                duration: 0.2,
            },
        ];
        let accuracy = BTreeMap::new();
        let wins = Vec::new();
        let siphonator = Vec::new();
        let flag = vec![(10.0, true), (11.0, false)];
        let no_fallouts = Vec::new();
        let fallout_deaths = vec![0.3];
        let mut streams = QueryStreams {
            damage: &damage,
            weapon_damage: &weapon_damage,
            frags: &frags,
            speeds: &speeds,
            accuracy: &accuracy,
            wins: &wins,
            siphonator: &siphonator,
            flag_carrier: &flag,
            fallout_deaths: &no_fallouts,
        };
        let query = RuleQuery {
            condition: Condition::Or,
            groups: vec![
                RuleGroup {
                    id: "movement".into(),
                    condition: Condition::And,
                    rules: vec![
                        SearchRule::Damage {
                            id: "rocket".into(),
                            minimum: 100,
                            weapon: Some(4),
                        },
                        SearchRule::Speed {
                            id: "speed".into(),
                            minimum: 1_000,
                            duration: 0.1,
                        },
                    ],
                },
                RuleGroup {
                    id: "flag-play".into(),
                    condition: Condition::And,
                    rules: vec![
                        SearchRule::FlagCarrier { id: "flag".into() },
                        SearchRule::Frags {
                            id: "pncr".into(),
                            minimum: 1,
                            weapon: Some(7),
                        },
                    ],
                },
            ],
        };
        let hits = query_scenes(&streams, &query, 2.0);
        assert_eq!(hits.len(), 2);
        assert_eq!(
            hits[0]
                .criteria
                .iter()
                .find(|m| m.rule_id == "rocket")
                .unwrap()
                .value,
            150.0
        );
        assert_eq!(
            hits[1]
                .criteria
                .iter()
                .find(|m| m.rule_id == "flag")
                .unwrap()
                .display,
            "1.0s"
        );
        assert_eq!(
            hits[1]
                .criteria
                .iter()
                .find(|m| m.rule_id == "pncr")
                .unwrap()
                .value,
            1.0
        );

        // The same high-speed burst is rejected when this player falls out
        // inside its window. Reversing the criterion makes that death itself
        // searchable.
        streams.fallout_deaths = &fallout_deaths;
        let clean_movement = RuleQuery {
            condition: Condition::And,
            groups: vec![RuleGroup {
                id: "clean-movement".into(),
                condition: Condition::And,
                rules: vec![
                    SearchRule::Speed {
                        id: "fast".into(),
                        minimum: 1_000,
                        duration: 0.1,
                    },
                    SearchRule::FalloutDeath {
                        id: "no-fallout".into(),
                        exclude: true,
                    },
                ],
            }],
        };
        assert!(query_scenes(&streams, &clean_movement, 2.0).is_empty());

        let find_fallout = RuleQuery {
            condition: Condition::And,
            groups: vec![RuleGroup {
                id: "fallouts".into(),
                condition: Condition::And,
                rules: vec![SearchRule::FalloutDeath {
                    id: "fallout".into(),
                    exclude: false,
                }],
            }],
        };
        let hits = query_scenes(&streams, &find_fallout, 2.0);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].criteria[0].display, "1 fallout");
    }
}
