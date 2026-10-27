# ANSImuz

A lightweight terminal music player with retro 8-bit ANSI vibes, built with Rust, Ratatui, Python, and [`ytmusicapi`](https://github.com/sigma67/ytmusicapi).

## Features

- Search and play music from YouTube Music
- Render album artwork directly in the terminal
- Artwork caching and prewarming
- Shuffle and repeat playback modes
- Playback controls and volume adjustment
- Configurable artwork rendering and color palette

## Requirements

- Rust toolchain (made in `rustc 1.99.1`)
- Python 3.10+
- [mpv](https://mpv.io/)
- [yt-dlp](https://github.com/yt-dlp/yt-dlp)

## Installation

Clone the repository:

```bash
git clone https://github.com/greenbugx/ANSImuz.git
cd ANSImuz
```

Create a python virtual environment:

```bash
python -m venv .venv
```

Install the Python dependencies:

```bash
pip3 install -r requirements.txt
```

Build and run:

```bash
cargo fmt --check
cargo check
cargo test
cargo run
```

## Controls

| Key | Action |
|---|---|
| `/` | Search |
| `↑` / `↓` | Select track |
| `Enter` | Play selected track |
| `Space` | Pause / resume |
| `←` / `→` | Previous / next track |
| `X` | Toggle shuffle |
| `R` | Cycle repeat mode |
| `+` / `-` | Adjust volume |
| `S` | Open settings |
| `Q` | Quit |

## License

See [LICENSE](LICENSE) for license information.