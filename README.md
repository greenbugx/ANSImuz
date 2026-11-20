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

- **Multi-Source Streaming**: Search and stream high-quality music directly from YouTube Music and Spotify.
- **Synchronized Karaoke Lyrics**: Real-time line and syllable-synchronized lyrics with duet alignment and background vocal streams.
- **Queue Management**: Add songs, remove them, or reorder upcoming tracks with intuitive keyboard shortcuts.
- **Session History**: Jump back to previously played songs from your current listening session.
- **Terminal Album Artwork**: High-resolution album covers rendered directly in your terminal using adaptive block art.
- **Adaptive Terminal Layout**: Dynamic interface that resizes gracefully to fit any terminal size.
- **Color Themes**: Dynamic UI color palettes sampled from album art or classic retro ANSI palettes.
- **Playback Modes**: Shuffle and repeat modes (Repeat Off, Repeat All, Repeat One).
- **Quick Command Palette**: Access all commands and settings instantly with `:`.
- **Discord Rich Presence (RPC)**: Share live track info, album art, elapsed progress, and stream links on Discord.
- **Offline Caching**: Automatically caches audio streams, album art, and lyrics for instant offline replay.

---

## Synchronized Lyrics

ANSImuz features an end-to-end synchronized lyrics experience powered by a tiered retrieval pipeline:

- **Word & Syllable Timing**: Progressive karaoke highlighting that highlights individual syllables and words in sync with the audio.
- **Multi-Vocal & Duet Support**:
  - **Lead Vocalist**: Left-aligned with active indicator (`▶`).
  - **Duet / Secondary Vocalist**: Right-aligned with active indicator (`◀`).
  - **Background Vocals**: Rendered in dim italics with a secondary indicator (`▷`).
- **Flexible Display**: Press **`L`** to open the lyrics view alongside the player on wide screens, or as a full-height overlay on compact terminals.
- **Independent Scrolling**:
  - Automatically follows active lyrics during playback.
  - Browse ahead or look back with **`k`** / **`j`** or **`PageUp`** / **`PageDown`** without interrupting playback. A `[MANUAL SCROLL]` indicator appears.
  - Press **`c`** at any time to re-center the viewport on the active singing line and resume auto-scroll.
- **Multi-Tier Retrieval**: Seamlessly queries synchronized lyric databases (including Apple Music TTML, QQ Music, and community LRCs), gracefully falling back to plain text lyrics when time-codes are unavailable.
- **Automatic Caching**: Lyrics are saved locally to disk so subsequent replays load instantly, even offline.

---

## Discord Rich Presence (RPC)

> [!WARNING]
> **Discord Rich Presence** is currently *experimental* and may not fully meet user expectations. It may behave unexpectedly, fail to work in certain situations, or contain bugs. Features and behavior are subject to change as development continues.

ANSImuz can display your live playback activity on Discord:

- **Rich Status**: Shows the track title, artist name, and album with a live elapsed/total progress bar.
- **Album Artwork**: Automatically uploads album art to secure image hosts so high-resolution covers display on Discord, with fallback to the retro ANSImuz logo.
- **Interactive Buttons**: Clickable buttons linking to public track pages on YouTube Music or Spotify, plus a "Get ANSImuz" button.
- **Seamless & Non-Blocking**: Runs in the background without stuttering audio playback or slowing down the terminal UI.

### Enabling & Configuration

Toggle Discord RPC inside ANSImuz:
1. Press **`S`** to open the **Settings** screen.
2. Toggle **Discord RPC** (`ON` / `OFF`).

For advanced options, configure the `[discord]` section in `~/.config/ansimuz/settings.toml` (or `%APPDATA%\ansimuz\settings.toml` on Windows):

```toml
[discord]
enabled = false          # Enable Discord Rich Presence (default: false)
artwork_enabled = true   # Upload and display album covers on Discord (default: true)

[discord.artwork]
primary_host = "imgdb"   # Primary image host: "imgdb", "catbox", or "litterbox" (default: "imgdb")
fallback_host = "catbox" # Fallback provider if primary host fails (default: "catbox")
cache_max_size_mb = 250  # Maximum disk cache for hosted artwork (default: 250 MB)
max_download_size_mb = 8 # Maximum download size for raw artwork (default: 8 MB)
image_size = 1024        # Normalized artwork dimensions in pixels (default: 1024)
litterbox_expiry = "72h" # Retention period when using Litterbox: "1h", "12h", "24h", "72h" (default: "72h")
```

---

## Supported Operating Systems

- **Linux** (x86_64): Primary platform. Tested on Arch Linux, Fedora, and Debian/Ubuntu derivatives.
- **Windows** (x86_64): Windows 10 and 11 supported via standalone portable release, native `mpv`/`yt-dlp`, and Windows Named Pipes. Also runs via WSL2.

---

ANSImuz relies on two standard command-line tools for media playback and streaming:

1. **[mpv](https://mpv.io/)** (0.35+ recommended): Audio playback engine.
2. **[yt-dlp](https://github.com/yt-dlp/yt-dlp)**: Audio stream resolution.

Ensure both tools are installed and accessible in your system `PATH`:

- **Windows (winget)**:
  ```powershell
  winget install --id shinchiro.mpv -e
  winget install --id yt-dlp.yt-dlp -e
  ```
- **Arch Linux**:
  ```bash
  sudo pacman -S mpv yt-dlp
  ```
- **Fedora**:
  ```bash
  sudo dnf install mpv yt-dlp
  ```

---

## Installation

### Windows (Portable Release)

The Windows portable release bundles all necessary runtime files and dependencies. No Python setup is needed.

1. Download `ansimuz-windows-v<version>-x64.rar` from the [Latest Release](https://github.com/greenbugx/ANSImuz/releases/latest).
2. Extract the archive to any folder (using Windows 11 built-in extraction, WinRAR, or 7-Zip).
3. Ensure [`mpv`](https://mpv.io/) and [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) are installed in your `PATH` (see [Prerequisites](#external-prerequisites)).
4. Launch `ansimuz.exe`.

### Arch Linux (AUR)

A `PKGBUILD` is available for Arch Linux:

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

To remove:
```bash
sudo dnf remove ansimuz
sudo dnf copr remove greenbugx/ansimuz
```

---

> [!NOTE]
> **Building from Source?**
> If you are a developer or prefer building ANSImuz directly from source code, please refer to [CONTRIBUTING.md](CONTRIBUTING.md) for full toolchain requirements, build commands, and development setup instructions.

---

## Controls

### Navigation & Playback Controls

| Key | Context | Action |
|---|---|---|
| `Tab` | Navigation | Switch focus between Search Results and Queue |
| `↑` / `↓` | Navigation | Select track in focused list |
| `Enter` | Navigation | Play selected track from focused list |
| `A` | Navigation (Search) | Add highlighted search track to queue |
| `Delete` | Navigation (Queue) | Remove highlighted track from queue |
| `Alt+↑` / `Alt+↓` | Navigation (Queue) | Move highlighted queue track up / down |
| `Space` | Global | Play / Pause playback |
| `←` | Global | Play previously played track (session history) |
| `→` | Global | Play next track in queue |
| `X` | Global | Toggle queue shuffle |
| `R` | Global | Cycle repeat mode (Off → All → One) |
| `+` / `-` | Global | Adjust volume (±5%) |
| `/` | Navigation | Focus search bar |
| `L` | Global | Toggle synchronized lyrics view |
| `S` | Global | Open settings screen |
| `:` | Global | Open command palette |
| `?` | Global | Toggle About overlay |
| `Esc` | Global | Clear search / close overlay / cancel resolution |
| `Q` | Navigation | Quit ANSImuz |

### Lyrics Screen Controls

When the lyrics screen is open:

| Key | Action |
|---|---|
| `L` / `Esc` | Close lyrics view |
| `k` / `↑` | Scroll lyrics up (activates manual scroll mode) |
| `j` / `↓` | Scroll lyrics down (activates manual scroll mode) |
| `PageUp` | Scroll up one viewport page |
| `PageDown` | Scroll down one viewport page |
| `c` | Re-center viewport on active line and resume auto-scroll |
| `Space`, `←`, `→`, `+`, `-` | Standard playback controls remain fully active |

---

## Cache Storage

ANSImuz stores caches under `$XDG_CACHE_HOME/ansimuz` (Linux) or `%LOCALAPPDATA%\ansimuz` (Windows):

- `audio/`: Stream audio chunks for offline playback.
- `artwork/`: Cached terminal album artwork thumbnails.
- `lyrics/`: Cached synchronized and plain lyrics.
- `discord/artwork/`: Cached Discord artwork images and upload records.

You can safely clear all caches at any time from within ANSImuz by opening **Settings** (**`S`**) and selecting **Clear Caches**.

---

## License

See [LICENSE](LICENSE) for license information.
