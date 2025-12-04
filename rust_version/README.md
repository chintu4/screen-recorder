# Rust Screen Recorder

A cross-platform GUI screen recorder built with Rust, `egui`, and `ffmpeg`.

## Features
- **Minimal GUI:** Easy to use interface.
- **Record Screen:** Captures the primary monitor or custom regions.
- **Audio Recording:** Supports recording from default audio input (ALSA on Linux).
- **Formats:** Saves as MP4 (H.264) or WebM (VP9).
- **Controls:** Start, Stop, Pause, Resume.

## Prerequisites

### Linux
You need `ffmpeg` installed on your system.
```bash
sudo apt install ffmpeg
```

You also need build dependencies for `egui`:
```bash
sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
```

## Running
```bash
cd rust_version
cargo run --release
```

## Development
This project uses:
- `egui` / `eframe` for the GUI.
- `std::process::Command` to spawn `ffmpeg` for recording.
