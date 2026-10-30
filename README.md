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

- Search and play music from YouTube Music and Spotify
- Manage an upcoming queue: add songs, remove them, or reorder the list
- Session history so you can jump back to previously played songs
- Album artwork displayed directly in your terminal
- Adaptive layout that fits any terminal size
- Dynamic color themes based on album art
- Shuffle and repeat modes (Repeat One / Repeat All)
- Full playback controls with keyboard shortcuts
- Quick command palette and built-in help screen

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

### Automated Setup (Recommended)

Run the installer for your operating system. It installs all prerequisites (including `mpv` and `yt-dlp`), creates the Python environment, installs dependencies, and compiles the release binary:

**Linux:**
```bash
./scripts/install_linux.sh
```

**Windows (PowerShell):**
```powershell
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass
.\scripts\install_windows.ps1
```

### Manual Setup
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
