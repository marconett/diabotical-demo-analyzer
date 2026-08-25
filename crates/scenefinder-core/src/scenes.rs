//! The scene search: sliding damage/frag windows over one player's events.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
