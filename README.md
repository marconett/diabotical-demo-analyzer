# Diabotical Demo Analyzer

A desktop app that finds a player's standout moments ("scenes") in Diabotical demo files: windows of a few seconds in which one player dealt a lot of damage and/or scored several frags, optionally only those that end with the player's round-winning frag. It is a GUI for the `scene-finder` command of the dbt-netcode toolkit and produces the same results and the same text output.

Rust core + Tauri 2 shell, Vue 3 front end. Built for large collections: a folder of ~1000 demos (13 GB) scans in the time it takes to gunzip it in parallel, and extracted data is cached so every later run with different players or thresholds is instant.

## Usage

1. Pick a demo file (`.rbr` client recording, `.srd` server recording) or a folder with the buttons, or drop it anywhere onto the window. Folders are searched recursively.
2. The player list is filled from the start of every demo. Pick names from the suggestions or type any name (a player who joined mid-match may only show up after a full scan cached the demo).
3. Set the criteria, exactly as on the command line:
   - **Min. damage** (`-dmg`) and **Min. frags** (`-frags`): thresholds within the window. At least one of them, or **Round win**, is required.
   - **Combine** (`-c and|or`): whether both thresholds must hold (default) or either, only relevant when both are set.
   - **Time window** (`-t`): window length in seconds; required with a damage or frag threshold.
   - **Round win** (`-win`): the scene must end with the player's round-winning frag (the round may close up to 1 s after the window). Alone, it lists every round-winning frag the player scored.
4. Run. Results are grouped per demo; "Copy as text" copies the exact command-line output.

Cache location: the app data directory (`~/Library/Application Support/de.marconett.dbt-fragfinder/extract-cache` on macOS). Entries are keyed by absolute path, size and modification time, so a changed file is rescanned automatically. "Clear cache" removes them.

## Building

Prerequisites: Rust (stable), Node 22+, and the [Tauri 2 system dependencies](https://tauri.app/start/prerequisites/) for your platform.

```bash
npm install
npm run tauri dev      # development build with hot reload
npm run tauri build    # release bundle in src-tauri/target/release/bundle
```

The core lives in `crates/scenefinder-core` and has no Tauri dependency. It also builds a small command-line tool that mirrors the reference `scene-finder` CLI, useful for scripting and benchmarking:

```bash
cargo run --release -p scenefinder-core --features cli -- ~/demos -p PlayerName -dmg 200 -frags 2 -c or -t 5
cargo run --release -p scenefinder-core --features cli -- --discover ~/demos          # list player names
cargo run --release -p scenefinder-core --features cli -- --bench --no-cache ~/demos -p x -dmg 1 -t 5
```

Tests: `cargo test --workspace --features scenefinder-core/cli`. The golden tests compare against the reference tool's output on sample demos and are skipped when `../dbt-netcode/samples` (or `$SCENEFINDER_SAMPLES`) is absent.

## How it works

Each demo is read once through a streaming gzip inflate; the container framing is walked without decoding messages, and only the handful of message types the search needs (damage totals, kill feed, team assignment, round score, player names) are decoded. Truncated recordings, which are common, are read up to the point where they stop. The per-demo extract (~50 KB) is what gets cached; the scene search itself runs over these extracts in microseconds.

Demos are processed in parallel across all cores, largest first. Cancelling stops every worker within a chunk.

## Notes

- `.srd` server recordings are supported per the format description but have not been exercised against real files.
- Output matches the reference Python implementation byte-for-byte on the sample demos, including its choice among equally good windows for round-winning scenes (which in Python follows set iteration order; the core reproduces that order).

## License

MIT
