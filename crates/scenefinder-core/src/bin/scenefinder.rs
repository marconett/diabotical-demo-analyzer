//! Command-line front end with the same output as the reference tool, plus
//! `--discover` (list player names) and `--bench` (timings).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::time::Instant;

use scenefinder_core::{
    discover_players, list_demos_many, run, Cache, Condition, Params, RunControl,
};

const USAGE: &str = "usage: scenefinder [--no-cache] [--cache-dir DIR] [--bench] [--discover] \
<demo or folder>... -p NAME [-p NAME ...] [-dmg N] [-weapon ID] [-speed N] \
[-speed-for SECONDS] [-accuracy PERCENT] [-frags N] [-c and|or] [-win] [-t SECONDS]";

fn main() -> ExitCode {
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut params = Params {
        players: Vec::new(),
        damage: None,
        frags: None,
        weapon: None,
        speed: None,
        speed_duration: None,
        accuracy: None,
        condition: Condition::And,
        win: false,
        window: None,
        rule_query: None,
    };
    let mut use_cache = true;
    let mut cache_dir: Option<PathBuf> = None;
    let mut bench = false;
    let mut discover = false;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |what: &str| {
            args.next().unwrap_or_else(|| {
                eprintln!("{what} needs a value\n{USAGE}");
                std::process::exit(2)
            })
        };
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "-p" | "--player_name" => params.players.push(value("-p")),
            "-dmg" | "--damage_threshold" => params.damage = Some(parse(&value("-dmg"), "-dmg")),
            "-weapon" => params.weapon = Some(parse(&value("-weapon"), "-weapon")),
            "-speed" => params.speed = Some(parse(&value("-speed"), "-speed")),
            "-speed-for" => params.speed_duration = Some(parse(&value("-speed-for"), "-speed-for")),
            "-accuracy" => params.accuracy = Some(parse(&value("-accuracy"), "-accuracy")),
            "-frags" => params.frags = Some(parse(&value("-frags"), "-frags")),
            "-c" | "--condition" => {
                params.condition = match value("-c").to_ascii_uppercase().as_str() {
                    "AND" => Condition::And,
                    "OR" => Condition::Or,
                    other => {
                        eprintln!("-c must be and|or, got {other:?}");
                        return ExitCode::from(2);
                    }
                }
            }
            "-win" => params.win = true,
            "-t" | "--time_window" => params.window = Some(parse(&value("-t"), "-t")),
            "--no-cache" => use_cache = false,
            "--cache-dir" => cache_dir = Some(PathBuf::from(value("--cache-dir"))),
            "--bench" => bench = true,
            "--discover" => discover = true,
            s if s.starts_with('-') => {
                eprintln!("unknown option {s}\n{USAGE}");
                return ExitCode::from(2);
            }
            s => inputs.push(PathBuf::from(s)),
        }
    }
    if inputs.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    let files = match list_demos_many(&inputs) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let cache = if use_cache {
        let dir = cache_dir.unwrap_or_else(|| std::env::temp_dir().join("scenefinder-cache"));
        match Cache::open(&dir) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("cannot open cache {}: {e}", dir.display());
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    let ctl = RunControl::default();
    let total_bytes: u64 = files.iter().map(|f| f.size).sum();

    if discover {
        let t0 = Instant::now();
        let result = discover_players(&files, cache.as_ref(), &ctl);
        for p in &result.players {
            println!("{:>5}  {}", p.demos, p.name);
        }
        for e in &result.errors {
            eprintln!("# {e}");
        }
        if bench {
            report_bench("discover", t0, &files.len(), total_bytes, &ctl);
        }
        return ExitCode::SUCCESS;
    }

    if let Err(e) = params.validate() {
        eprintln!("{e}");
        return ExitCode::from(2);
    }
    let t0 = Instant::now();
    let report = match run(&files, &params, cache.as_ref(), &ctl) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    print!("{}", report.text);
    if bench {
        for d in &report.demos {
            eprintln!(
                "# {:>7} ms  {}{}",
                d.scan_ms,
                d.file_name,
                if d.from_cache { "  (cache)" } else { "" }
            );
        }
        report_bench("run", t0, &files.len(), total_bytes, &ctl);
    }
    if report.demos_with_player > 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn parse<T: std::str::FromStr>(s: &str, what: &str) -> T {
    s.parse().unwrap_or_else(|_| {
        eprintln!("invalid value for {what}: {s:?}");
        std::process::exit(2)
    })
}

fn report_bench(what: &str, t0: Instant, files: &usize, total_bytes: u64, ctl: &RunControl) {
    let secs = t0.elapsed().as_secs_f64();
    let read = ctl.bytes_done.load(Ordering::Relaxed);
    eprintln!(
        "# {what}: {files} file(s), {:.1} MB on disk, {:.1} MB read, {:.2} s, {:.0} MB/s read, {} cache hit(s), threads {}",
        total_bytes as f64 / 1e6,
        read as f64 / 1e6,
        secs,
        read as f64 / 1e6 / secs.max(1e-9),
        ctl.cache_hits.load(Ordering::Relaxed),
        rayon::current_num_threads()
    );
}
