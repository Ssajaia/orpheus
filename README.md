# Orpheus

A minimal, lightweight desktop music player written in Rust (`egui`/`eframe` GUI, `rodio` audio).

## Features

- Pick a local music directory (remembered between launches).
- Reads title, artist, album and track number tags; tracks are grouped by album and ordered by track number (untagged files are kept, listed by file name under "Unknown Album").
- Track list, current track, progress and duration.
- Play, pause, resume, stop, previous, next, volume.
- Automatically advances to the next track and stops after the last one.
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
