//! Evaluates search parameters against an extracted demo and assembles the
//! per-demo and per-run reports.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::extract::DemoExtract;
use crate::format::header::DemoMeta;
use crate::params::Params;
use crate::scenes::{deltas, events, scenes, win_scenes, Hit};
use crate::text::{clock, fmt_g, py_repr, render_report};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub start: f64,
    pub clock: String,
    pub player: String,
    pub damage: Option<u64>,
    pub frags: Option<u64>,
    pub round: Option<u32>,
    pub line: String,
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
    let mut active: Vec<u32> = ex.dmg.keys().chain(ex.frags.keys()).copied().collect();
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
            "# no player named {} dealt damage or scored in {file_name}",
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

    let criteria = params.criteria();
    let empty_dmg: Vec<(f32, u32)> = Vec::new();
    let empty_frags: Vec<f32> = Vec::new();
    let empty_wins: Vec<(f32, u32)> = Vec::new();
    let mut found: Vec<(Hit, &str)> = Vec::new();
    for (want, pids) in &ids {
        for pid in pids {
            let mut frag_times = ex.frags.get(pid).unwrap_or(&empty_frags).clone();
            frag_times.sort_by(f32::total_cmp);
            let ev = events(deltas(ex.dmg.get(pid).unwrap_or(&empty_dmg)), &frag_times);
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
            found.extend(hits.into_iter().map(|h| (h, *want)));
        }
    }
    found.sort_by(|(a, an), (b, bn)| {
        a.start
            .total_cmp(&b.start)
            .then_with(|| an.cmp(bn))
            .then(a.damage.cmp(&b.damage))
            .then(a.frags.cmp(&b.frags))
            .then(a.round.cmp(&b.round))
    });

    for (hit, want) in found {
        let line = match hit.damage {
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
        };
        report.out_lines.push(line.clone());
        report.scenes.push(Scene {
            start: hit.start,
            clock: clock(hit.start),
            player: want.to_string(),
            damage: hit.damage,
            frags: hit.frags,
            round: hit.round,
            line,
        });
    }
    report
        .out_lines
        .push(format!("# {} scene(s)", report.scenes.len()));
    report.scene_count = Some(report.scenes.len());
    report
}
