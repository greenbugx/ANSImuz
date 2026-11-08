<div align="center">
  <img src="assets/logo_nobg.png" alt="ANSImuz Logo" width="320" />
  <p>A lightweight terminal music player with retro 8-bit ANSI vibes</p>
  <p>
    <a href="https://github.com/greenbugx/ANSImuz/actions/workflows/ci.yml"><img src="https://github.com/greenbugx/ANSImuz/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
    <a href="https://copr.fedorainfracloud.org/coprs/greenbugx/ansimuz/package/ansimuz/"><img src="https://copr.fedorainfracloud.org/coprs/greenbugx/ansimuz/package/ansimuz/status_image/last_build.png" alt="COPR build status" /></a> <br>
    <a href="https://github.com/greenbugx/ANSImuz/releases/latest"><img src="https://img.shields.io/github/v/release/greenbugx/ANSImuz" alt="Latest version" /></a>
    <a href="https://hub.xerj.org/contributors/greenbugx"><img src="https://hub.xerj.org/contributors/greenbugx/badge.svg" alt="XERJ Corpus Hub contributor" /></a><br><br>
    <a href="https://sourceforge.net/projects/ansimuz/files/latest/download"><img alt="Download ANSImuz" src="https://a.fsdn.com/con/app/sf-download-button" width=276 height=48 srcset="https://a.fsdn.com/con/app/sf-download-button?button_size=2x 2x"></a>
  </p>
</div>

> [!IMPORTANT]
> **Disclaimer:** This project is not affiliated with, endorsed by, or in any way officially connected to Youtube or Spotify. It is intended solely for educational purposes only.

ANSImuz is built with Rust, Ratatui, Python, [`ytmusicapi`](https://github.com/sigma67/ytmusicapi), [`spotifyscraper`](https://github.com/AliAkhtari78/SpotifyScraper), and [`interprocess`](https://github.com/kotauskas/interprocess).

## Features

- Search and play music from YouTube Music and Spotify
- Manage an upcoming queue: add songs, remove them, or reorder the list
- Session history so you can jump back to previously played songs
- Album artwork displayed directly in your terminal
- Adaptive layout that fits any terminal size
- Dynamic color themes based on album art
- Shuffle and repeat modes (Repeat One / Repeat All)
- Full playback controls with keyboard shortcuts
- Quick command palette

## Supported Operating Systems

- **Linux** (x86_64): Primary platform. Tested on Arch Linux, Fedora, and generic systemd/musl environments. Communicates with `mpv` via Unix-domain sockets.
- **Windows** (x86_64): Native Windows 10/11 support via PowerShell installer (`scripts/install_windows.ps1`), native `mpv`/`yt-dlp`, and cross-platform IPC over Windows Named Pipes (`\\.\pipe\...`) and also supported through `WSL2`.

## Requirements

### External Dependencies
- **[mpv](https://mpv.io/)** (0.35+ recommended): Audio playback engine and JSON IPC communication (Unix-domain sockets on Linux, Named Pipes on Windows). Must be available in system `PATH`.
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)**: Audio stream URL resolution and media stream extraction. Must be available in system `PATH`.

### Build & Runtime Toolchain
- **Rust Toolchain**: 1.85+ (compatible with the Rust 2024 edition).
- **Python**: 3.10 or higher with development headers (required for PyO3 embedding).
- **C Compiler & System Libraries**: `gcc` / `clang`, `glibc`, and OpenSSL (`libssl-dev` / `openssl-devel`).

## Installation

### Windows (Portable Release)

1. Download `ansimuz-windows-x64.zip` from the [Latest Release](https://github.com/greenbugx/ANSImuz/releases/latest).
2. Extract the archive to any folder.
3. Ensure [`mpv`](https://mpv.io/) and [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) are installed and accessible in your `PATH`:

   ```powershell
   winget install shinchiro.mpv yt-dlp.yt-dlp
   ```
4. Run `ansimuz.exe` directly from the extracted folder.

### Windows (PowerShell Automated Installer)

If building and installing from source, run the automated installer script in PowerShell. It verifies and installs all prerequisites (including `mpv`, `yt-dlp`, Python, and Rust via `winget`), creates the `.venv` virtual environment, installs Python packages, and compiles the release binary:

```powershell
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass
.\scripts\install_windows.ps1
```

To launch ANSImuz after running the installer:

```powershell
# Activate the virtual environment
.\.venv\Scripts\Activate.ps1

# Run the release binary
.\target\release\ansimuz.exe
```
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

### Fedora (COPR)

```bash
sudo dnf copr enable greenbugx/ansimuz
sudo dnf install ansimuz
```

This also installs `python3-spotifyscraper` from the same COPR repository. The package installs the binary to `/usr/bin/ansimuz` and runtime data to `/usr/share/ansimuz`.

> [!NOTE]
> Packages are built per Fedora release and depend on that release's Python version. If `dnf` fails with `nothing provides python(abi) = ...`, your Fedora release is not built in the COPR yet. Please open an issue and include the output of `grep VERSION_ID /etc/os-release`.

To remove it:

```bash
sudo dnf remove ansimuz
sudo dnf copr remove greenbugx/ansimuz
```
## Source Builds vs. Packaged Installs

- **Packaged Installs (Arch Linux PKGBUILD / Fedora COPR)**:
  System packages install the compiled binary to `/usr/bin/ansimuz` and static runtime resources (`backend/` and `assets/`) to `/usr/share/ansimuz/`. They link against system Python modules (`python-ytmusicapi`, `python-spotifyscraper`, etc.) and require no virtual environment activation.
- **Source Builds (`cargo run` / `cargo build`)**:
  When building from source, Python dependencies run within a project-local virtual environment (`.venv`). The `.venv` must be activated in your terminal session before starting `cargo run`. The application automatically resolves `backend/` and `assets/` directly from your repository working directory.

## Development Setup

> [!NOTE]
> These setups are intended for **development**. ANSImuz uses a Python virtual environment (`.venv`) for its backend when built from source, so you must activate it before running `cargo run` or the compiled binary.
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

### Runtime Resource Directories (`data_dir`)

ANSImuz locates its embedded Python bridge (`backend/`) and visual assets (`assets/`) via `crate::paths::data_dir()`. The data directory is resolved in the following priority order:

1. `ANSIMUZ_DATA_DIR` environment variable set at runtime (explicit user override).
2. The current working directory (development from project root).
3. The executable's parent directories (e.g. running from `target/debug/` or `target/release/`).
4. `ANSIMUZ_DATA_DIR` configured at compile time via `option_env!("ANSIMUZ_DATA_DIR")`.
5. `/usr/share/ansimuz` (standard system package install path).
6. Fallback to current directory (`.`).

### Cache Storage & Lifecycle

Runtime cache directories are stored under `$XDG_CACHE_HOME/ansimuz` (or `~/.cache/ansimuz`):
- `audio/`: Cached audio streams and prefetch dumps.
- `artwork/`: Cached album artwork thumbnails (`.img`).

**Cache Safeguards:**
- **Atomic Writes**: Writes use temporary `.part` files renamed atomically upon completion.
- **Stale Cleanup**: Any incomplete `.part` files leftover from unexpected process kills or crashes are automatically cleaned up on application startup.
- **Bounded Disk Quota**: Cache directories are automatically pruned to a maximum threshold (512 MB per cache type) on a least-recently-modified basis.
- **Epoch Invalidation**: Triggering **Clear Caches** in Settings (`S` → Clear Caches) atomically increments a cache epoch, guaranteeing in-flight downloads discard their results rather than repopulating cleared storage.

### Network Requirements

- **Online Operations**:
  - Music searching (querying YouTube Music or Spotify APIs).
  - Spotify track resolution (finding YouTube Music stream equivalents).
  - Audio streaming and background prefetch (`yt-dlp` stream resolution and `mpv` network playback).
  - Album artwork fetching (`reqwest` streaming image downloads).
- **Offline Capabilities**:
  - Playing previously cached audio streams (`audio_cache = true` in Settings).
  - Viewing cached album artwork.
  - Viewing the animated fallback visualizer (uses bundled local frames).
  - Navigating the local queue and session history.
  - Running offline unit tests and mock-based integration suites.

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
| `?` | Nav | Toggle About overlay |
| `Esc` | Any | Clear search query / close overlay / cancel resolution |
| `Q` | Nav | Quit ANSImuz |

## License

See [LICENSE](LICENSE) for license information.
