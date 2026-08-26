//! Evaluates search parameters against an extracted demo and assembles the
//! per-demo and per-run reports.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::extract::DemoExtract;
use crate::format::header::DemoMeta;
use crate::params::{weapon_name, Params};
use crate::scenes::{
    advanced_scenes, advanced_win_scenes, deltas, events, query_scenes, scenes, win_scenes,
    AdvancedCriteria, Hit, QueryStreams,
};
use crate::text::{clock, fmt_g, py_repr, render_report};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub start: f64,
    pub clock: String,
    pub player: String,
    pub damage: Option<u64>,
    pub frags: Option<u64>,
    pub weapon: Option<u32>,
    pub speed: Option<u64>,
    pub speed_duration: Option<f64>,
    /// Game-reported match-to-date accuracy, in percent.
    pub accuracy: Option<f64>,
    pub round: Option<u32>,
    #[serde(default)]
    pub criteria: Vec<SceneMetric>,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneMetric {
    pub rule_id: String,
    pub label: String,
    pub value: f64,
    pub secondary: Option<f64>,
    pub display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoReport {
    pub path: String,
    pub file_name: String,
    pub meta: Option<DemoMeta>,
    pub error: Option<String>,
    /// None when none of the requested players dealt damage or scored.
    pub scene_count: Option<usize>,
    pub scenes: Vec<Scene>,
    pub missing_players: Vec<String>,
    pub players_seen: Vec<String>,
    pub truncated: bool,
    pub stopped_early: bool,
    pub from_cache: bool,
    pub scan_ms: u32,
    pub out_lines: Vec<String>,
    pub err_lines: Vec<String>,
}

impl DemoReport {
    pub fn failed(path: &Path, error: String) -> Self {
        let file_name = file_name(path);
        DemoReport {
            path: path.to_string_lossy().into_owned(),
            err_lines: vec![format!("# {file_name}: {error}")],
            file_name,
            meta: None,
            error: Some(error),
            scene_count: None,
            scenes: Vec::new(),
            missing_players: Vec::new(),
            players_seen: Vec::new(),
            truncated: false,
            stopped_early: false,
            from_cache: false,
            scan_ms: 0,
            out_lines: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    pub header_line: String,
    pub demos: Vec<DemoReport>,
    pub demo_count: usize,
    /// Demos in which at least one requested player was active.
    pub demos_with_player: usize,
    pub total_scenes: usize,
    /// More than one demo: demos without scenes are left out of the text.
    pub bulk: bool,
    pub cancelled: bool,
    pub cache_hits: usize,
    pub elapsed_ms: u64,
    pub text: String,
}

impl RunReport {
    pub fn assemble(
        params: &Params,
        demos: Vec<DemoReport>,
        cancelled: bool,
        cache_hits: usize,
        elapsed_ms: u64,
    ) -> Self {
        let demo_count = demos.len();
        let demos_with_player = demos.iter().filter(|d| d.scene_count.is_some()).count();
        let total_scenes = demos.iter().map(|d| d.scene_count.unwrap_or(0)).sum();
        let mut r = RunReport {
            header_line: params.header_line(),
            demos,
            demo_count,
            demos_with_player,
            total_scenes,
            bulk: demo_count > 1,
            cancelled,
            cache_hits,
            elapsed_ms,
            text: String::new(),
        };
        r.text = render_report(&r);
        r
    }
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

pub fn evaluate(ex: &DemoExtract, params: &Params, path: &Path) -> DemoReport {
    let file_name = file_name(path);
    let mut out = Vec::new();
    let mut err = Vec::new();
    if let Some(meta) = &ex.meta {
        out.push(format!(
            "# {file_name}  [{}]  app={} mode={} map={}",
            meta.magic.as_str(),
            meta.app_version,
            meta.game_mode,
            meta.map_name
        ));
    }

    let names = ex.final_names();
    let mut active: Vec<u32> = ex
        .dmg
        .keys()
        .chain(ex.frags.keys())
        .chain(ex.weapon_accuracy.keys())
        .chain(ex.speeds.keys())
        .chain(ex.siphonator.keys())
        .chain(ex.flag_carrier.keys())
        .chain(ex.fallout_deaths.keys())
        .copied()
        .collect();
    if let Some(pov) = ex.pov_id {
        active.push(pov);
    }
    active.sort_unstable();
    active.dedup();

    let mut wanted: Vec<&str> = Vec::new();
    for p in &params.players {
        if !wanted.contains(&p.as_str()) {
            wanted.push(p);
        }
    }
    let ids: Vec<(&str, Vec<u32>)> = wanted
        .iter()
        .map(|want| {
            (
                *want,
                active
                    .iter()
                    .copied()
                    .filter(|pid| names.get(pid) == Some(want))
                    .collect::<Vec<u32>>(),
            )
        })
        .collect();
    let missing: Vec<String> = ids
        .iter()
        .filter(|(_, pids)| pids.is_empty())
        .map(|(w, _)| w.to_string())
        .collect();
    let players_seen: Vec<String> = ex.names_seen.iter().cloned().collect();
    if !missing.is_empty() {
        let quoted: Vec<String> = missing.iter().map(|m| py_repr(m)).collect();
        err.push(format!(
            "# no searchable data for player named {} in {file_name}",
            quoted.join(", ")
        ));
        if !players_seen.is_empty() {
            err.push(format!("# players seen: {}", players_seen.join(", ")));
        }
    }
    let mut report = DemoReport {
        path: path.to_string_lossy().into_owned(),
        file_name: file_name.clone(),
        meta: ex.meta.clone(),
        error: None,
        scene_count: None,
        scenes: Vec::new(),
        missing_players: missing.clone(),
        players_seen,
        truncated: ex.truncated_gzip,
        stopped_early: ex.stopped_early,
        from_cache: false,
        scan_ms: 0,
        out_lines: out,
        err_lines: err,
    };
    if missing.len() == ids.len() {
        return report;
    }

    let use_query = params.rule_query.is_some();
    let use_advanced = !use_query
        && (params.speed.is_some()
            || params.accuracy.is_some()
            || (params.weapon.is_some() && params.damage.is_some()));
    let criteria = params.criteria();
    let advanced_criteria = AdvancedCriteria {
        dmg: params.damage,
        frags: params.frags,
        speed: params.speed,
        speed_duration: params.speed_duration.unwrap_or(0.0),
        accuracy: params.accuracy,
        and: params.condition == crate::params::Condition::And,
    };
    let empty_dmg: Vec<(f32, u32)> = Vec::new();
    let empty_frags: Vec<f32> = Vec::new();
    let empty_wins: Vec<(f32, u32)> = Vec::new();
    let empty_accuracy: Vec<(f32, f32)> = Vec::new();
    let empty_accuracy_map = std::collections::BTreeMap::new();
    let empty_state: Vec<(f32, bool)> = Vec::new();
    let mut found: Vec<(FoundHit, &str)> = Vec::new();
    for (want, pids) in &ids {
        for pid in pids {
            let mut frag_times = ex.frags.get(pid).unwrap_or(&empty_frags).clone();
            frag_times.sort_by(f32::total_cmp);
            let damage_events = if let Some(weapon) = params.weapon.filter(|_| use_advanced) {
                if ex.pov_id == Some(*pid) {
                    ex.weapon_damage
                        .iter()
                        .filter(|event| event.weapon == weapon)
                        .map(|event| (f64::from(event.ts), u64::from(event.damage)))
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                deltas(ex.dmg.get(pid).unwrap_or(&empty_dmg))
            };
            let ev = events(damage_events, &frag_times);
            if let Some(query) = &params.rule_query {
                let mut frag_weapons = ex.frag_weapons.get(pid).cloned().unwrap_or_default();
                if frag_weapons.is_empty() {
                    frag_weapons.extend(frag_times.iter().map(|time| (*time, u32::MAX)));
                }
                frag_weapons.sort_by(|a, b| a.0.total_cmp(&b.0));
                let total_damage = deltas(ex.dmg.get(pid).unwrap_or(&empty_dmg));
                let streams = QueryStreams {
                    damage: &total_damage,
                    weapon_damage: if ex.pov_id == Some(*pid) {
                        &ex.weapon_damage
                    } else {
                        &[]
                    },
                    frags: &frag_weapons,
                    speeds: ex.speeds.get(pid).map(Vec::as_slice).unwrap_or_default(),
                    accuracy: ex.weapon_accuracy.get(pid).unwrap_or(&empty_accuracy_map),
                    wins: ex.wins.get(pid).unwrap_or(&empty_wins),
                    siphonator: ex.siphonator.get(pid).unwrap_or(&empty_state),
                    flag_carrier: ex.flag_carrier.get(pid).unwrap_or(&empty_state),
                    fallout_deaths: ex.fallout_deaths.get(pid).unwrap_or(&empty_frags),
                };
                found.extend(
                    query_scenes(&streams, query, params.window.unwrap_or(0.0))
                        .into_iter()
                        .map(|hit| {
                            let accuracy = hit
                                .criteria
                                .iter()
                                .find(|metric| metric.label.contains("accuracy"))
                                .map(|metric| metric.value);
                            (
                                FoundHit {
                                    start: hit.start,
                                    damage: Some(hit.damage),
                                    frags: Some(hit.frags),
                                    weapon: None,
                                    speed: hit.speed,
                                    speed_duration: None,
                                    accuracy,
                                    round: hit.round,
                                    criteria: hit
                                        .criteria
                                        .into_iter()
                                        .map(|metric| SceneMetric {
                                            rule_id: metric.rule_id,
                                            label: metric.label,
                                            value: metric.value,
                                            secondary: metric.secondary,
                                            display: metric.display,
                                        })
                                        .collect(),
                                },
                                *want,
                            )
                        }),
                );
            } else if use_advanced {
                let speeds = ex.speeds.get(pid).map(Vec::as_slice).unwrap_or_default();
                let accuracy = params
                    .weapon
                    .and_then(|weapon| ex.weapon_accuracy.get(pid)?.get(&weapon))
                    .unwrap_or(&empty_accuracy);
                let window = params.window.unwrap_or(0.0);
                let hits = if params.win {
                    advanced_win_scenes(
                        &ev,
                        speeds,
                        accuracy,
                        ex.wins.get(pid).unwrap_or(&empty_wins),
                        window,
                        &advanced_criteria,
                    )
                } else {
                    advanced_scenes(&ev, speeds, accuracy, window, &advanced_criteria)
                };
                found.extend(hits.into_iter().map(|h| {
                    (
                        FoundHit {
                            start: h.start,
                            damage: Some(h.damage),
                            frags: Some(h.frags),
                            weapon: params.weapon,
                            speed: h.speed,
                            speed_duration: h.speed_duration,
                            accuracy: h.accuracy,
                            round: h.round,
                            criteria: Vec::new(),
                        },
                        *want,
                    )
                }));
            } else {
                let hits = if params.win {
                    win_scenes(
                        &ev,
                        ex.wins.get(pid).unwrap_or(&empty_wins),
                        params.window,
                        &criteria,
                    )
                } else {
                    scenes(&ev, params.window.unwrap_or(0.0), &criteria)
                };
                found.extend(hits.into_iter().map(|h| (FoundHit::legacy(h), *want)));
            }
        }
    }
    found.sort_by(|(a, an), (b, bn)| {
        a.start
            .total_cmp(&b.start)
            .then_with(|| an.cmp(bn))
            .then(a.damage.cmp(&b.damage))
            .then(a.frags.cmp(&b.frags))
            .then(a.speed.cmp(&b.speed))
            .then(a.round.cmp(&b.round))
    });

    for (hit, want) in found {
        let line = if use_query {
            query_line(&hit, want, params.window.unwrap_or(0.0))
        } else if use_advanced {
            advanced_line(&hit, want, params.window.unwrap_or(0.0))
        } else {
            match hit.damage {
                None => format!(
                    "{} {want} scored the round-winning frag (round {})",
                    clock(hit.start),
                    hit.round.unwrap_or(0)
                ),
                Some(damage) => {
                    let mut tail = format!("{} frags", hit.frags.unwrap_or(0));
                    if let Some(r) = hit.round {
                        tail.push_str(&format!(", won round {r}"));
                    }
                    format!(
                        "{} {want} did {damage} damage in {} seconds ({tail})",
                        clock(hit.start),
                        fmt_g(params.window.unwrap_or(0.0))
                    )
                }
            }
        };
        report.out_lines.push(line.clone());
        report.scenes.push(Scene {
            start: hit.start,
            clock: clock(hit.start),
            player: want.to_string(),
            damage: hit.damage,
            frags: hit.frags,
            weapon: hit.weapon,
            speed: hit.speed,
            speed_duration: hit.speed_duration,
            accuracy: hit.accuracy,
            round: hit.round,
            criteria: hit.criteria,
            line,
        });
    }
    report
        .out_lines
        .push(format!("# {} scene(s)", report.scenes.len()));
    report.scene_count = Some(report.scenes.len());
    report
}

#[derive(Debug, Clone)]
struct FoundHit {
    start: f64,
    damage: Option<u64>,
    frags: Option<u64>,
    weapon: Option<u32>,
    speed: Option<u64>,
    speed_duration: Option<f64>,
    accuracy: Option<f64>,
    round: Option<u32>,
    criteria: Vec<SceneMetric>,
}

impl FoundHit {
    fn legacy(hit: Hit) -> Self {
        FoundHit {
            start: hit.start,
            damage: hit.damage,
            frags: hit.frags,
            weapon: None,
            speed: None,
            speed_duration: None,
            accuracy: None,
            round: hit.round,
            criteria: Vec::new(),
        }
    }
}

fn query_line(hit: &FoundHit, player: &str, window: f64) -> String {
    let metrics = hit
        .criteria
        .iter()
        .map(|metric| format!("{}: {}", metric.label, metric.display))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{} {player} matched in {} seconds ({metrics})",
        clock(hit.start),
        fmt_g(window)
    )
}

fn advanced_line(hit: &FoundHit, player: &str, window: f64) -> String {
    let mut metrics = Vec::new();
    if let Some(damage) = hit.damage {
        let label = hit
            .weapon
            .map(|weapon| format!("{} damage", weapon_name(weapon)))
            .unwrap_or_else(|| "damage".into());
        metrics.push(format!("{damage} {label}"));
    }
    if let Some(frags) = hit.frags {
        metrics.push(format!("{frags} frags"));
    }
    if let Some(speed) = hit.speed {
        let duration = hit.speed_duration.unwrap_or(0.0);
        if duration > 0.0 {
            metrics.push(format!(
                "max speed {speed}, {}s above threshold",
                fmt_g(duration)
            ));
        } else {
            metrics.push(format!("max speed {speed}"));
        }
    }
    if let Some(accuracy) = hit.accuracy {
        metrics.push(format!("{}% reported accuracy", fmt_g(accuracy)));
    }
    if let Some(round) = hit.round {
        metrics.push(format!("won round {round}"));
    }
    format!(
        "{} {player}: {} in {} seconds",
        clock(hit.start),
        metrics.join(", "),
        fmt_g(window)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{SpeedSample, WeaponDamage};
    use crate::params::Condition;

    #[test]
    fn evaluates_combined_pov_weapon_and_speed_metrics() {
        let mut ex = DemoExtract::default();
        ex.names.push((1, "alice".into()));
        ex.pov_id = Some(1);
        ex.weapon_damage = vec![
            WeaponDamage {
                ts: 1.0,
                weapon: 4,
                damage: 100,
            },
            WeaponDamage {
                ts: 1.1,
                weapon: 5,
                damage: 500,
            },
        ];
        ex.speeds.entry(1).or_default().push(SpeedSample {
            ts: 1.2,
            speed: 1_500,
            duration: 0.1,
        });
        ex.weapon_accuracy
            .entry(1)
            .or_default()
            .entry(4)
            .or_default()
            .push((0.9, 0.4));
        let params = Params {
            players: vec!["alice".into()],
            damage: Some(90),
            frags: None,
            weapon: Some(4),
            speed: Some(1_400),
            speed_duration: Some(0.1),
            accuracy: Some(35.0),
            condition: Condition::And,
            win: false,
            window: Some(2.0),
            rule_query: None,
        };

        let report = evaluate(&ex, &params, Path::new("demo.rbr"));
        assert_eq!(report.scene_count, Some(1));
        let scene = &report.scenes[0];
        assert_eq!(scene.weapon, Some(4));
        assert_eq!(scene.damage, Some(100));
        assert_eq!(scene.speed, Some(1_500));
        assert!((scene.speed_duration.unwrap() - 0.1).abs() < 1e-6);
        assert!((scene.accuracy.unwrap() - 40.0).abs() < 0.001);
        assert!(scene.line.contains("Rocket Launcher damage"));
        assert!(scene.line.contains("above threshold"));
    }
}
