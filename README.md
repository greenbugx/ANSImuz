# ANSImuz

[![XERJ Corpus Hub contributor](https://hub.xerj.org/contributors/greenbugx/badge.svg)](https://hub.xerj.org/contributors/greenbugx)
[![CI](https://github.com/greenbugx/ANSImuz/actions/workflows/ci.yml/badge.svg)](https://github.com/greenbugx/ANSImuz/actions/workflows/ci.yml)

> [!IMPORTANT]
> **Disclaimer:** This project is not affiliated with, endorsed by, or in any way officially connected to Youtube or Spotify. It is intended solely for educational purposes only.

A lightweight terminal music player with retro 8-bit ANSI vibes, built with Rust, Ratatui, Python, [`ytmusicapi`](https://github.com/sigma67/ytmusicapi) and [`spotifyscraper`](https://github.com/AliAkhtari78/SpotifyScraper)

## Features

- Search and play music from the selected provider (YT Music & Spotify)
- Responsive multi-panel layout (Search Results, Player, and Queue)
- Searchable Command Palette (`:`) and Keybinds Help Overlay (`?`)
- Render album artwork directly in the terminal (with Kitty, Sixel, iTerm2, and Unicode half-blocks)
- Artwork caching and background prewarming
- Shuffle and repeat playback modes
- Playback controls and volume adjustment
- Dynamic artwork color palette theming across the entire interface
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

# macOS/Linux
source .venv/bin/activate

# Windows PowerShell
.venv\Scripts\Activate.ps1

# Windows Command Prompt
.venv\Scripts\activate.bat
```

Install the Python dependencies:

```bash
pip3 install -r requirements.txt
```

Build and run:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run
```

## Controls
| Key | Action |
|---|---|
| `/` | Focus search bar |
| `↑` / `↓` | Select track |
| `Enter` | Play selected track |
| `Space` | Pause / resume |
| `←` / `→` | Previous / next track |
| `X` | Toggle shuffle |
| `R` | Cycle repeat mode |
| `+` / `-` | Adjust volume |
| `S` | Open settings |
| `:` | Commands |
| `?` | Help |
| `Q` | Quit |
## License

See [LICENSE](LICENSE) for license information.
