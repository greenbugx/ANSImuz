<div align="center">
  <img src="assets/logo_nobg.png" alt="ANSImuz Logo" width="320" />
  <p>A lightweight terminal music player with retro 8-bit ANSI vibes</p>
  <p>
    <a href="https://hub.xerj.org/contributors/greenbugx"><img src="https://hub.xerj.org/contributors/greenbugx/badge.svg" alt="XERJ Corpus Hub contributor" /></a>
    <a href="https://github.com/greenbugx/ANSImuz/actions/workflows/ci.yml"><img src="https://github.com/greenbugx/ANSImuz/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  </p>
</div>

> [!IMPORTANT]
> **Disclaimer:** This project is not affiliated with, endorsed by, or in any way officially connected to Youtube or Spotify. It is intended solely for educational purposes only.

ANSImuz is built with Rust, Ratatui, Python, [`ytmusicapi`](https://github.com/sigma67/ytmusicapi), and [`spotifyscraper`](https://github.com/AliAkhtari78/SpotifyScraper).

## Features

- **Dual Music Providers**: Search and stream audio seamlessly from YouTube Music & Spotify with non-blocking stream resolution.
- **Spotify-Style Queue & History**: Full playback queue and session history. `Next` (`→`) and `Previous` (`←`) navigation strictly consumes the queue and history without bleeding into search results.
- **Queue Duplicate Prevention**: Guarantees distinct queue entries with duplicate detection across Spotify IDs, stream URLs, and normalized title/artist pairs.
- **Focus-Aware Navigation**: Instant `Tab` toggling between Search Results and Queue with independent selection cursors.
- **Interactive Queue Operations**: Enqueue highlighted tracks (`A`), remove entries (`Delete`), and reorder upcoming songs (`Alt+↑ / Alt+↓`).
- **Adaptive Multi-Panel Layout**: Seamlessly adapts across terminal sizes (3-panel widescreen $\ge$ 120 cols, progressive reveal 100–119 cols, 2-panel list swap 75–99 cols, and compact stacked mode 40–74 cols).
- **Terminal Album Artwork**: High-resolution artwork rendering supporting Kitty, Sixel, iTerm2, and Unicode half-blocks with background prewarming and disk caching.
- **Dynamic Palette Theming**: Real-time extraction of dominant artwork colors applied dynamically across TUI borders and widgets.
- **Playback Engine**: Queue shuffle, repeat modes (Off, All, One), volume adjustment, and robust MPV process supervision.
- **Searchable Overlays**: Command Palette (`:`) and Keybinds Help Overlay (`?`).
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

| Key | Context | Action |
|---|---|---|
| `Tab` | Nav | Switch focus between Search Results and Queue |
| `↑` / `↓` | Nav | Select track in focused list |
| `Enter` | Nav | Play selected track from focused list |
| `A` | Nav (Search focused) | Add highlighted search track to queue |
| `Delete` | Nav (Queue focused) | Remove highlighted track from queue |
| `Alt+↑` / `Alt+↓` | Nav (Queue focused) | Move highlighted queue track up / down |
| `Space` | Any | Play / Pause playback |
| `←` | Any | Play previously played track (from session history) |
| `→` | Any | Play next track in queue |
| `X` | Any | Toggle queue shuffle |
| `R` | Any | Cycle repeat mode (Off → All → One) |
| `+` / `-` | Any | Adjust volume (±5%) |
| `/` | Nav | Focus search bar |
| `S` | Nav | Open settings screen |
| `:` | Nav | Open command palette |
| `?` | Nav | Toggle help overlay |
| `Esc` | Any | Clear search query / close overlay / cancel resolution |
| `Q` / `Ctrl+C` | Nav | Quit ANSImuz |
## License

See [LICENSE](LICENSE) for license information.
