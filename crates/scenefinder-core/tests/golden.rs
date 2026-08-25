//! Compares complete runs against the reference tool's output on the local
//! sample demos. Skipped when the samples are not present.

use std::path::PathBuf;

use scenefinder_core::{
    discover_players, list_demos_many, run, Cache, Condition, Params, RunControl,
};

fn samples() -> Option<PathBuf> {
    let dir = std::env::var_os("SCENEFINDER_SAMPLES")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../dbt-netcode/samples")
        });
    dir.join("WIPEOUT-2026_06_12-22_03_40.rbr")
        .exists()
        .then_some(dir)
}

fn params(
    players: &[&str],
    damage: Option<u64>,
    frags: Option<u64>,
    or: bool,
    win: bool,
    window: Option<f64>,
) -> Params {
    Params {
        players: players.iter().map(|s| s.to_string()).collect(),
        damage,
        frags,
        condition: if or { Condition::Or } else { Condition::And },
        win,
        window,
    }
}

fn text(paths: &[PathBuf], p: &Params, cache: Option<&Cache>) -> String {
    let files = list_demos_many(paths).unwrap();
    run(&files, p, cache, &RunControl::default()).unwrap().text
}

#[test]
fn golden_damage_or_frags() {
    let Some(dir) = samples() else { return };
    let out = text(
        &[dir.join("WIPEOUT-2026_06_12-22_03_40.rbr")],
        &params(
            &["27 shaft avg n1"],
            Some(200),
            Some(2),
            true,
            false,
            Some(5.0),
        ),
        None,
    );
    assert_eq!(
        out,
        "# scene-finder  player='27 shaft avg n1'  dmg >= 200 OR frags >= 2 in 5s\n\
\n\
# WIPEOUT-2026_06_12-22_03_40.rbr  [EVGR]  app=0.20.471s mode=wipeout map=woa_rekiem\n\
[02:56.95] 27 shaft avg n1 did 312 damage in 5 seconds (0 frags)\n\
[03:15.77] 27 shaft avg n1 did 403 damage in 5 seconds (1 frags)\n\
[04:51.93] 27 shaft avg n1 did 320 damage in 5 seconds (1 frags)\n\
[05:12.43] 27 shaft avg n1 did 238 damage in 5 seconds (1 frags)\n\
[05:33.14] 27 shaft avg n1 did 207 damage in 5 seconds (0 frags)\n\
[10:12.95] 27 shaft avg n1 did 214 damage in 5 seconds (1 frags)\n\
[15:14.01] 27 shaft avg n1 did 68 damage in 5 seconds (2 frags)\n\
[15:52.10] 27 shaft avg n1 did 263 damage in 5 seconds (0 frags)\n\
# 8 scene(s)\n"
    );
}

#[test]
fn golden_win_only_two_players() {
    let Some(dir) = samples() else { return };
    let out = text(
        &[dir.join("WIPEOUT-2026_06_12-22_03_40.rbr")],
        &params(
            &["27 shaft avg n1", "t0urizt"],
            None,
            None,
            false,
            true,
            None,
        ),
        None,
    );
    assert_eq!(
        out,
        "# scene-finder  players='27 shaft avg n1', 't0urizt'  round-winning frag\n\
\n\
# WIPEOUT-2026_06_12-22_03_40.rbr  [EVGR]  app=0.20.471s mode=wipeout map=woa_rekiem\n\
[04:30.67] 27 shaft avg n1 scored the round-winning frag (round 2)\n\
[05:18.22] t0urizt scored the round-winning frag (round 3)\n\
# 2 scene(s)\n"
    );
}

#[test]
fn golden_bulk_win_frags_with_cache_round_trip() {
    let Some(dir) = samples() else { return };
    let paths = [
        dir.join("WIPEOUT-2026_06_12-22_03_40.rbr"),
        dir.join("WIPEOUT-2026_06_17-23_32_33.rbr"),
    ];
    let p = params(&["27 shaft avg n1"], None, Some(3), false, true, Some(10.0));
    let expected =
        "# scene-finder  player='27 shaft avg n1'  frags >= 3 AND round-winning frag in 10s\n\
\n\
# WIPEOUT-2026_06_17-23_32_33.rbr  [EVGR]  app=0.20.471s mode=wipeout map=woa_rekiem\n\
[08:51.72] 27 shaft avg n1 did 309 damage in 10 seconds (3 frags, won round 6)\n\
# 1 scene(s)\n\
\n\
# 1 scene(s) across 2/2 demo(s)\n";
    let cache_dir =
        std::env::temp_dir().join(format!("scenefinder-golden-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache_dir);
    let cache = Cache::open(&cache_dir).unwrap();
    assert_eq!(text(&paths, &p, Some(&cache)), expected);
    let files = list_demos_many(&paths).unwrap();
    let ctl = RunControl::default();
    let cached = run(&files, &p, Some(&cache), &ctl).unwrap();
    assert_eq!(cached.text, expected);
    assert_eq!(cached.cache_hits, 2);
    assert!(cached.demos.iter().all(|d| d.from_cache));
    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn golden_missing_player_and_truncation() {
    let Some(dir) = samples() else { return };
    let files = list_demos_many(&[dir.join("WIPEOUT-2026_06_12-22_03_40.rbr")]).unwrap();
    let r = run(
        &files,
        &params(&["x"], Some(200), None, false, false, Some(5.0)),
        None,
        &RunControl::default(),
    )
    .unwrap();
    assert_eq!(
        r.text,
        "# scene-finder  player='x'  dmg >= 200 in 5s\n\
\n\
# WIPEOUT-2026_06_12-22_03_40.rbr  [EVGR]  app=0.20.471s mode=wipeout map=woa_rekiem\n\
# no player named 'x' dealt damage or scored in WIPEOUT-2026_06_12-22_03_40.rbr\n\
# players seen: 27 shaft avg n1, Big Poppa 94, JM L0rdBeAK, LordBuRns23, dr.teivos, lg acrid, pog 川, t0urizt\n"
    );
    assert_eq!(r.demos_with_player, 0);
    let ex = scenefinder_core::scan_demo(&files[0].path, &Default::default()).unwrap();
    assert_eq!(ex.inflated_bytes, 92_321_732);
    assert!(ex.truncated_gzip);
}

#[test]
fn golden_discover_names() {
    let Some(dir) = samples() else { return };
    let files = list_demos_many(&[dir.join("WIPEOUT-2026_06_12-22_03_40.rbr")]).unwrap();
    let result = discover_players(&files, None, &RunControl::default());
    let names: Vec<&str> = result.players.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "27 shaft avg n1",
            "Big Poppa 94",
            "JM L0rdBeAK",
            "LordBuRns23",
            "dr.teivos",
            "lg acrid",
            "pog 川",
            "t0urizt"
        ]
    );
}
