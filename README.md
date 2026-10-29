# ANSImuz

[![XERJ Corpus Hub contributor](https://hub.xerj.org/contributors/greenbugx/badge.svg)](https://hub.xerj.org/contributors/greenbugx)

> [!IMPORTANT]
> **Disclaimer:** This project is not affiliated with, endorsed by, or in any way officially connected to Youtube or Spotify. It is intended solely for educational purposes only.

A lightweight terminal music player with retro 8-bit ANSI vibes, built with Rust, Ratatui, Python, [`ytmusicapi`](https://github.com/sigma67/ytmusicapi) and [`spotifyscraper`](https://github.com/AliAkhtari78/SpotifyScraper)

## Features

- Search and play music from the selected provider (YT Music & Spotify)
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
| `:` | Commands |
| `?` | Help |
| `Q` | Quit |

## License

See [LICENSE](LICENSE) for license information.
