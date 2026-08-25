# Diabotical Demo Analyzer

A desktop app that finds a player's standout moments ("scenes") in Diabotical demo files: windows of a few seconds in which one player dealt a lot of damage and/or scored several frags, optionally only those that end with the player's round-winning frag.

Built for large collections: a folder of ~1000 demos (14 GB) scanned in ~9s on my NVME.\
Extracted data is cached so every later run with different parameters is instant.

![Diabotical Demo Analyzer showing three scenes found across 934 demos](screenshot.png)

## Where to find your replays

Diabotical writes replays to the following folders. Point the app at this folder (or drop it onto the window) to scan all of them:

| Platform | Folder |
|---|---|
| Windows | `C:\Users\<you>\AppData\Roaming\Diabotical\Replays\` (paste `%APPDATA%\Diabotical\Replays` into Explorer) |
| Linux (Heroic, Wine/Proton prefix) | `~/Games/Heroic/Prefixes/default/Diabotical/pfx/drive_c/users/steamuser/AppData/Roaming/Diabotical/Replays/` |

## Usage

1. Pick a demo file (`.rbr` client recording, `.srd` server recording) or a folder, or drop it anywhere onto the window. Folders are searched recursively.
2. Pick player names you want to look up (a player who joined mid-match may only show up after a full scan cached the demo).
3. Set the criteria, exactly as on the command line:
   - **Min. damage** (`-dmg`) and **Min. frags** (`-frags`): thresholds within the time window. At least one of them, or **Round win**, is required.
   - **Combine** (`-c and|or`): whether both thresholds must hold or either, only relevant when both are set.
   - **Time window** (`-t`): window length in seconds; required with a damage or frag threshold.
   - **Round win** (`-win`): the scene must end with the player's round-winning frag.
4. Run.
5. Use "Copy command" to copy a console command into the clipboard and paste it into the Diabotical console to watch the scene.

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

Tests: `cargo test --workspace --features scenefinder-core/cli`.

## How it works

Each demo is read once through a streaming gzip inflate; the container framing is walked without decoding messages, and only the handful of message types the search needs (damage totals, kill feed, team assignment, round score, player names) are decoded. Truncated recordings, which are common, are read up to the point where they stop. The per-demo extract (~50 KB) is what gets cached; the scene search itself runs over these extracts in microseconds.

Demos are processed in parallel across all cores, largest first. Cancelling stops every worker within a chunk.