# Orpheus

A minimal, lightweight desktop music player written in Rust (`egui`/`eframe` GUI, `rodio` audio).

## Features

- Pick a local music directory (remembered between launches).
- Reads title, artist, album and track number tags; tracks are grouped by album and ordered by track number (untagged files are kept, listed by file name under "Unknown Album").
- Album grid with cover art (embedded artwork, or a `cover.jpg`/`cover.png`/`folder.jpg`/`folder.png` file when the folder holds a single album); select an album to see its tracks and play it.
- Vinyl playback view: a record with the album art as its label, turning at 33 1/3 rpm while playing, and a tonearm that lowers onto the record and tracks progress. Title and artist are shown below; the record stands still when paused or stopped. Opens when you play from an album; the Record/Library button switches views.
- Current track, progress and duration.
- Play, pause, resume, stop, previous, next, volume.
- Plays an album in order and stops after its last track; previous/next stay within the album.
- Dark theme, no network access, no telemetry.

## Supported platforms

Linux, Windows and macOS. CI currently runs on Linux only.

## Supported audio formats

MP3, WAV, FLAC and OGG (Vorbis). Only the selected directory is scanned, not its subdirectories.

## Prerequisites

- Stable Rust toolchain (`rustup`).
- Linux: ALSA development headers and `pkg-config`
  (Debian/Ubuntu: `sudo apt install libasound2-dev pkg-config`).

## Build

```sh
cargo build --release
```

## Run

```sh
cargo run --release
```

Type or paste the path of your music directory and press **Load** (or Enter).
Settings are stored as `config.toml` in the OS configuration directory.

## Test

```sh
cargo test --all-targets
```

Tests need no audio hardware or music files.
