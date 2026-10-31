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

### Arch Linux (PKGBUILD)

> [!WARNING]
> Review any AUR package before installing it, including `python-spotifyscraper`.

A `PKGBUILD` is available in [`packaging/arch`](packaging/arch). It builds the latest `main` and installs ANSImuz system-wide (binary in `/usr/bin`, runtime data in `/usr/share/ansimuz`).

`python-spotifyscraper` is an AUR package, so install it first:

```bash
paru -S python-spotifyscraper    # or yay -S, or build it manually from the AUR
git clone https://github.com/greenbugx/ANSImuz.git
cd ANSImuz/packaging/arch
makepkg -si
```

## Development Setup

> [!NOTE]
> These setups are intended for **development**. ANSImuz uses a Python virtual environment (`.venv`) for its backend, so you must activate it in every new terminal session before running `cargo run` or the compiled binary. Otherwise the backend (search, streaming, artwork) will not work.
>
> On Arch Linux, the [PKGBUILD](#arch-linux-pkgbuild) installs ANSImuz system-wide and needs no virtual environment.

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

### Environment Variables

- `ANSIMUZ_DATA_DIR`: Path to application runtime data (`backend/` and `assets/`). Packagers can configure this at compile time (`ANSIMUZ_DATA_DIR=/usr/share/ansimuz cargo build --release`) or users can override it at runtime. Defaults to `/usr/share/ansimuz` when installed, or the local project directory during development.

The data directory is resolved in this order:

1. `ANSIMUZ_DATA_DIR` set at runtime (used as-is)
2. `ANSIMUZ_DATA_DIR` set at compile time (if it contains `backend/` and `assets/`)
3. `/usr/share/ansimuz` (if it contains `backend/` and `assets/`)
4. The current working directory (if it contains `backend/` and `assets/`)
5. The executable's directory and its parents, e.g. `target/release/` (if one contains `backend/` and `assets/`)

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