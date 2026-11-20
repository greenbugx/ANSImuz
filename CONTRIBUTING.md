# Contributing to ANSImuz

Thank you for your interest in contributing to ANSImuz! We welcome bug reports, feature suggestions, documentation improvements, and code contributions.

Please review this guide before submitting a pull request or opening an issue.

---

## Code of Conduct

All contributors and participants are expected to adhere to our [Code of Conduct](CODE_OF_CONDUCT.md). Please treat others with respect and keep our community welcoming and collaborative.

---

## Supported Operating Systems & Prerequisites

ANSImuz supports:
- **Linux** (x86_64): Primary platform (tested on Arch Linux, Fedora). Uses Unix-domain sockets for MPV IPC.
- **Windows** (x86_64): Native Windows 10/11 support via PowerShell installer (`scripts/install_windows.ps1`), native `mpv`/`yt-dlp`, and cross-platform IPC over Windows Named Pipes (`\\.\pipe\...`). Also supported through `WSL2`.

Ensure the following tools and development libraries are installed on your system:

### Required Toolchains & Libraries
- **Rust toolchain**: 1.85+ (compatible with the Rust 2024 edition).
- **Python**: 3.10 or higher with development headers (`python3-dev` / `python3-devel`) for PyO3 linking.
- **C Compiler & System Libraries**: `gcc` / `clang`, `glibc`, and OpenSSL development headers (`libssl-dev` / `openssl-devel`).
- **[mpv](https://mpv.io/)**: Version 0.35+ recommended, with JSON IPC enabled (Unix-domain sockets on Linux, Named Pipes on Windows).
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)**: For stream URL resolution and media extraction.

---

## Setting Up Your Development Environment

### Automated Setup (Recommended)

Run the installer for your operating system. It verifies and installs all prerequisites (including `mpv` and `yt-dlp`), creates the Python virtual environment (`.venv`), installs Python packages, and compiles the project:

**Linux:**
```bash
./scripts/install_linux.sh
```

**Windows (PowerShell):**
```powershell
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass
.\scripts\install_windows.ps1
```

---

### Manual Setup

1. **Clone the repository**:
   ```bash
   git clone https://github.com/greenbugx/ANSImuz.git
   cd ANSImuz
   ```

2. **Create and activate a Python virtual environment**:

   **Linux**:
   ```bash
   python -m venv .venv
   source .venv/bin/activate
   ```

   **Windows (PowerShell)**:
   ```powershell
   python -m venv .venv
   .\.venv\Scripts\Activate.ps1
   ```

   **Windows (Command Prompt)**:
   ```cmd
   python -m venv .venv
   .\.venv\Scripts\activate.bat
   ```

3. **Install Python dependencies**:
   ```bash
   pip install --upgrade pip
   pip install -r requirements.txt
   ```

4. **Verify the build and tests**:
   ```bash
   cargo fmt --check
   cargo check --all-targets
   cargo clippy --all-targets -- -D warnings
   cargo test
   python -m unittest discover -s backend/tests -p "test_*.py"
   ```

5. **Run ANSImuz locally**:
   ```bash
   cargo run
   ```

> [!IMPORTANT]
> Because ANSImuz links dynamically against the Python runtime and uses Python libraries (`ytmusicapi`, `spotifyscraper`, etc.) via PyO3, you **must activate the virtual environment (`.venv`)** in your shell before running `cargo run`, `cargo test`, or compiled binaries during development.

---

## Source Builds vs. Packaged Installs

- **Source Builds (`cargo run` / `cargo build`)**:
  When developing from source, Python dependencies run inside the project-local `.venv`. The application dynamically resolves `backend/` and `assets/` directly from your repository root.
- **Packaged Installs (Arch Linux & Fedora COPR)**:
  System packages install the compiled binary to `/usr/bin/ansimuz` and static runtime resources (`backend/` and `assets/`) to `/usr/share/ansimuz/`. They link against system Python modules (`python-ytmusicapi`, `python-spotifyscraper`, etc.) and require no virtual environment.
- **Windows Portable Release**:
  Bundles `ansimuz.exe`, visual assets, backend bridge, and an isolated embedded Python 3.14 runtime with all site-packages included in `Lib/site-packages`.

### Runtime Resource Directories (`data_dir`)

ANSImuz locates its embedded Python bridge (`backend/`) and visual assets (`assets/`) via `crate::paths::data_dir()`. The path is resolved in the following priority order:

1. `ANSIMUZ_DATA_DIR` environment variable set at runtime (explicit override).
2. The current working directory (development from project root).
3. The executable's parent directories (e.g. running from `target/debug/` or `target/release/`).
4. `ANSIMUZ_DATA_DIR` configured at compile time via `option_env!("ANSIMUZ_DATA_DIR")`.
5. `/usr/share/ansimuz` (standard system package install path).
6. Fallback to current directory (`.`).

---

## Cache Storage & Lifecycle

Runtime cache directories are stored under `$XDG_CACHE_HOME/ansimuz` (or `~/.cache/ansimuz` on Linux, `%LOCALAPPDATA%\ansimuz` on Windows):

- `audio/`: Cached audio stream segments and prefetch dumps.
- `artwork/`: Cached terminal album artwork thumbnails (`.img`).
- `lyrics/`: Cached synchronized and plain lyrics (`index.json` + `entries/*.json`).
- `discord/artwork/`: Cached Discord artwork images and upload records (`index.json`).

### Cache Safeguards & Architecture
- **Atomic Writes**: All cache entries are written to temporary `.part` files and renamed atomically upon completion.
- **Stale Cleanup**: Any incomplete `.part` files leftover from unexpected process kills or crashes are automatically cleaned up on application startup.
- **Bounded Disk Quota**: Cache directories are automatically pruned to configured thresholds (e.g. 512 MB for audio, 250 MB for Discord artwork, max byte quota for lyrics) on an LRU / least-recently-accessed basis.
- **Epoch Invalidation**: Triggering **Clear Caches** in Settings (`S` → Clear Caches) atomically clears cache directories and increments a cache epoch, guaranteeing in-flight requests discard their results rather than repopulating cleared storage.

---

## Project Architecture

The codebase is organized into clean, modular boundaries:

```text
ANSImuz/
├── src/
│   ├── app/              # Application loop, state machine, and orchestration
│   │   ├── runner.rs         # Main event loop and terminal drawing
│   │   ├── playback.rs       # Playback lifecycle, queue sequencing, history, and resolution
│   │   ├── search.rs         # Search result handling, refetching, and pagination
│   │   ├── events.rs         # Raw key event handling and input routing
│   │   ├── commands.rs       # Command palette actions and dispatch
│   │   ├── state.rs          # Authoritative PlaybackState and application state
│   │   └── mod.rs            # App module re-exports
│   ├── artwork/          # Terminal image decoding, memory cache, resize cache, renderer
│   ├── config/           # Settings persistence (TOML), platform cache dirs, atomic writes
│   ├── discord/          # Discord Rich Presence subsystem
│   │   ├── activity.rs       # Activity builder, timestamps, buttons, UTF-8 truncation
│   │   ├── client.rs         # Low-level Discord IPC transport wrapper
│   │   ├── worker.rs         # Background worker actor, auto-reconnect backoff
│   │   └── artwork/          # Discord artwork resolution, normalization, and upload pipeline
│   ├── lyrics/           # Lyrics engine subsystem
│   │   ├── types.rs          # Data structures (Lyrics, LyricLine, LyricWord, LyricSyllable)
│   │   ├── worker.rs         # Dedicated lyrics worker thread, request deduplication & retry
│   │   ├── cache.rs          # Persistent disk cache (index.json, entries/), LRU pruning
│   │   ├── synchronizer.rs   # Real-time MPV playback synchronizer, duet/vocal separation
│   │   ├── provider.rs       # Bridge to Python lyrics retrieval pipeline
│   │   ├── model.rs          # Serialized lyrics models and converter helpers
│   │   └── mod.rs            # Lyrics module exports
│   ├── music/            # Track model, identity policy, backend PyO3 bridge execution
│   ├── playback/         # MPV IPC process supervision, cross-platform transport (ipc.rs)
│   │   ├── events.rs         # PlayerEvent enum (Position, Ended, Failed)
│   │   ├── ipc.rs            # Cross-platform local IPC endpoints & streams (interprocess)
│   │   ├── mpv.rs            # AudioPlayer implementation, observer loop, stream dumping
│   │   ├── repeat.rs         # Repeat mode sequencing
│   │   └── mod.rs            # Playback module exports
│   ├── terminal/         # Terminal raw mode setup and clean teardown
│   ├── ui/               # Ratatui components, screens, and themes
│   │   ├── screens/
│   │   │   ├── lyrics.rs         # Karaoke display, duet alignment, zero-alloc word views
│   │   │   ├── player.rs         # Active playback display and visualizer
│   │   │   ├── settings.rs       # In-app settings configuration
│   │   │   └── search.rs         # Track search list and queue view
│   │   └── theme.rs          # ANSI themes, colors, and truncation helpers
│   ├── paths.rs          # Runtime data directory resolution (backend/ and assets/)
│   ├── lib.rs            # Library interface for integration tests
│   └── main.rs           # Application entry point (cleans stale caches, boots TUI)
├── backend/
│   ├── bridge.py         # PyO3 integration boundary
│   ├── exceptions.py     # Structured provider exception hierarchy
│   ├── artwork/          # Python thumbnail utilities and prewarming
│   ├── providers/        # Music and lyrics provider implementations
│   │   ├── ytmusic/          # YouTube Music API integration
│   │   ├── spotify/          # Spotify scraper integration
│   │   └── lyrics/           # Multi-tier lyrics retrieval pipeline
│   │       ├── base.py           # Base provider ABC and timing constants
│   │       ├── lrc_red.py        # lrc.red provider (TTML & LRC)
│   │       ├── lyrics_plus.py    # LyricsPlus mirror racing provider
│   │       ├── lrclib.py         # LRCLIB REST API provider
│   │       ├── megalobiz.py      # Megalobiz community LRC provider
│   │       ├── better_lyrics.py  # BetterLyrics Apple Music TTML / Portato QRC
│   │       ├── genius.py         # Genius web scraper unsynced lyrics fallback
│   │       ├── matching.py       # Track similarity and candidate ranking
│   │       ├── normalization.py  # Response payload formatting
│   │       ├── timestamps.py     # Timestamp parsers and millisecond converters
│   │       ├── parsers/          # Specialized format parsers
│   │       │   ├── ttml.py           # Apple Music TTML with syllable & offset support
│   │       │   ├── lrc.py            # Multi-timestamp and offset-aware LRC parser
│   │       │   ├── qrc.py            # Portato QQ Music QRC parser
│   │       │   └── lyricsfile.py     # LRCLIB YAML lyricsfile parser
│   │       └── __init__.py       # Provider registry, health tracker, and get_lyrics
│   └── tests/            # Python unit and mock test suites
├── assets/               # Logo and visualizer animation frames
├── scripts/              # Linux and Windows installer scripts
├── packaging/            # Packaging specs (Arch Linux PKGBUILD, Fedora RPM specs)
├── vendor/               # Vendored crates (presenceforge)
└── tests/                # Rust integration test suites
```

---

## Code Guidelines & Best Practices

- **Non-blocking TUI**: Never block the main TUI rendering thread. Long-running tasks (network fetches, PyO3 queries, stream resolutions) must execute asynchronously on background worker threads and communicate via channels.
- **Queue & Playback Separation**: Search results (`app.tracks`) are strictly for track discovery. Playback sequencing (`Next`, `Previous`, `Shuffle`, `Repeat`) operates exclusively on the playback queue (`app.queue`) and session history (`app.history`), never cycling through search results.
- **Queue Deduplication**: Use `app.enqueue()` or `app.queue_contains()` to prevent duplicate tracks from being enqueued.
- **Unicode Safety**: Never slice strings by raw byte offsets when displaying text in the TUI. Use character-based bounds or the project's `truncate` helper (`crate::ui::theme::truncate`) to prevent panics on multibyte characters.
- **Runtime Data Paths**: Locate `backend/` and `assets/` only through `crate::paths::data_dir()`. Never use `env!("CARGO_MANIFEST_DIR")` or other compile-time paths for runtime assets, because that bakes the build directory into release binaries and breaks packaged installs. Use `option_env!` only for optional build-time overrides such as `ANSIMUZ_DATA_DIR`.
- **Cross-Platform IPC**: MPV communication uses `IpcEndpoint` and `IpcStream` (`crate::playback::ipc`). On Unix, endpoints use filesystem paths (`.sock`); on Windows, endpoints use named pipes (`\\.\pipe\...`). Never invoke filesystem existence checks or deletions (`Path::exists()`, `fs::remove_file()`) on Windows named pipes.
- **Observer Thread Concurrency & Reaping**: Decouple periodic query writing from continuous response reading. Background writer threads must always be wrapped in a RAII guard (`WriterThreadGuard`) and joined when the observer exits, and `AudioPlayer::stop()` must join the observer thread.
- **Test Concurrency & State Isolation**: Tests that mutate shared global caches (such as `RESIZE_CACHE`, `art_cache()`, or `lyrics::cache`) must acquire their respective test locks (`ARTWORK_TEST_LOCK` or `LYRICS_CACHE_TEST_LOCK`) to prevent race conditions during parallel test runs. Self-contained data structure tests must instantiate local, isolated instances.

---

## Subsystem Architecture Deep Dives

### 1. Synchronized Lyrics Subsystem

The lyrics subsystem provides real-time word and syllable-synchronized lyrics with automatic fallback across 6 distinct services.

#### A. Python Retrieval Pipeline (`backend/providers/lyrics/`)
- **Tiered Provider Hierarchy**:
  1. `lrc.red`: Apple Music TTML with syllable timing and duet channels, with LRC fallback.
  2. `LyricsPlus`: Aggregated Apple Music, QQ Music, and Musixmatch syllable timings via bounded concurrent mirror racing.
  3. `LRCLIB`: Official database with Lyricsfile word sync & LRC line sync.
  4. `Megalobiz`: Community-synchronized LRC sheets via official REST API.
  5. `BetterLyrics`: Apple Music TTML syllable-timed & Portato QQ Music QRC fallback.
  6. `Genius`: Web scraper fallback providing unsynchronized plain lyrics with stanzas preserved.
- **Provider Registry (`LyricsProviderRegistry`)**: Thread-safe registry eliminating duplicated global singletons while supporting lazy instantiation and unit test mocking.
- **Resilient Health & Circuit Breakers (`ProviderHealthTracker`)**: Automatically tracks failures and enforces temporary backoffs (60s+ for HTTP 429, 30s+ for persistent network errors) to avoid blocking the retrieval chain.
- **Intra-Provider Deadline Budgeting**: Dynamically caps each provider's `self.timeout` to the remaining global retrieval deadline budget (with a safety floor of 1.0s and guaranteed restoration in a `finally` block), ensuring slow sequential queries never exceed the overall timeout.
- **Cooperative Cancellation**: Checks `CURRENT_CANCEL_TOKEN` before each provider and allows in-flight HTTP requests to terminate early when a track is skipped.
- **Specialized Parsers (`backend/providers/lyrics/parsers/`)**:
  - `ttml.py`: Preserves zero-millisecond timestamps (`is not None` vs falsy checks), handles container offset inheritance (`<body>`/`<div>`), supports parent-relative timings, and rejects invalid framerates.
  - `lrc.py`: Correctly handles multi-timestamp lines (`[00:10.00][00:25.00]`), global offset tags (`[offset:+/-ms]`), and empty instrumental lines.
  - `qrc.py` and `lyricsfile.py`: High-precision syllable and word timing parsers.

#### B. Rust Lyrics Engine (`src/lyrics/`)
- **Background Worker Actor (`worker.rs`)**:
  - Runs in an isolated background thread receiving `LyricsCommand::Fetch`, `Clear`, and `Shutdown`.
  - Dispatches requests using an atomic `generation` counter. Stale results from superseded tracks are dropped automatically.
  - Deduplicates repeated `Fetch` requests for the active track while tracking transient failures (`last_track_failed`), explicitly allowing retries after a failure without getting stuck in `Failed`.
- **Persistent Disk Cache (`cache.rs`)**:
  - SHA-256 content-hashed files stored under `$XDG_CACHE_HOME/ansimuz/lyrics/entries/`.
  - In-memory index (`index.json`) tracking LRU access and byte size.
  - Self-healing recovery: Automatically rebuilds or resets if the index file is corrupted or version-mismatched.
  - LRU pruning: Evicts oldest entries when total cache size exceeds configured bounds.
- **State-Preserving Synchronizer (`synchronizer.rs`)**:
  - Receives live playback position updates from MPV.
  - Handles seeking forwards and backwards, pause/resume, and repeat-one loops without losing position.
  - Enforces global chronological ordering across multi-vocal tracks (lead vocals, duets, and background channels).
  - Explicit failure reset: Distinguishes `PlayerEvent::Ended` from `PlayerEvent::Failed`, immediately resetting sync on errors instead of marking songs as completed.
- **Terminal UI (`src/ui/screens/lyrics.rs`)**:
  - Multi-vocal layout: Lead vocals left-aligned (`▶`), secondary duet singer right-aligned (`◀`), and background vocals styled in dim italics (`▷`).
  - Zero-allocation rendering: Uses borrowed `WordItem` and `WordSpan` slices rather than cloning word vectors every frame.
  - Explicit scroll policy: Viewport automatically follows the active lyric. When manual scroll is triggered (`k`/`j`/`PageUp`/`PageDown`), an indicator badge is shown; pressing `c` re-centers and resumes auto-scroll.
  - Transition guard: `resolve_sync_point` verifies track and generation identity to prevent flashing stale active lyrics across song changes.

---

### 2. Discord Rich Presence (RPC) Subsystem

The Discord Rich Presence subsystem (`src/discord/`) integrates real-time playback state with Discord's desktop client over local Inter-Process Communication (IPC).

1. **Activity Builder (`src/discord/activity.rs`)**:
   - Constructs Discord `Activity` payloads (`ActivityType::Listening`).
   - Timestamps: Live start and end timestamps calculated for playing tracks; timestamps omitted when paused.
   - Interactive Buttons: "Listen on YouTube Music" or "Listen on Spotify" (pointing to public canonical URLs) and "Get ANSImuz". Temporary streaming URLs are strictly rejected.
   - Truncation: Safe UTF-8 grapheme truncation preventing multibyte panic or Discord API rejection.
   
2. **IPC Client & Transport (`src/discord/client.rs` & `vendor/presenceforge/`)**:
   - Cross-platform local IPC: Unix-domain sockets (`/run/user/.../discord-ipc-0`) on Linux; Windows Named Pipes (`\\.\pipe\discord-ipc-0`) on Windows.
   - Offline safety: If Discord is closed or unreachable, calls return `DiscordError::NotConnected` safely without blocking the application.

3. **Background Worker Actor (`src/discord/worker.rs`)**:
   - Supervised background thread receiving `DiscordCommand` events.
   - Auto-reconnection with exponential backoff: 10s → 20s → 40s → capped at 60s. Backoff resets immediately upon successful handshake.
   
4. **Artwork Pipeline (`src/discord/artwork/`)**:
   - Format detection, dimensions checks, multipart uploaders (Imgdb, Catbox, Litterbox), SSRF validation, and persistent disk caching with LRU pruning.

---

## Packaging

### Arch Linux (`packaging/arch/`)
The `PKGBUILD` builds the latest `main` as `ansimuz-git`. It installs the binary to `/usr/bin/ansimuz` and `backend/` and `assets/` to `/usr/share/ansimuz/` with `ANSIMUZ_DATA_DIR=/usr/share/ansimuz`.

To test:
```bash
cd packaging/arch
makepkg -sf
namcap PKGBUILD
sudo pacman -U ansimuz-git-*.pkg.tar.zst
cd /tmp && ansimuz
```

### Fedora / COPR (`packaging/rpm/`)
`packaging/rpm/` contains two RPM specs built in the [`greenbugx/ansimuz`](https://copr.fedorainfracloud.org/coprs/greenbugx/ansimuz/) COPR project:
- `ansimuz.spec`: Builds ANSImuz from a pinned commit.
- `python-spotifyscraper.spec`: Packages `spotifyscraper` for Fedora.

To test locally with `mock`:
```bash
cd packaging/rpm
spectool -g -R ansimuz.spec
rpmbuild -bs ansimuz.spec
mock -r fedora-rawhide-x86_64 --enable-network rebuild ~/rpmbuild/SRPMS/ansimuz-*.src.rpm
```

### Windows Portable Release
The Windows portable release embeds its own Python 3.14 runtime so users do not need Python installed:
1. Compile release binary: `cargo build --release`.
2. Extract the Python Windows Embeddable package into a staging directory.
3. Install dependencies into `Lib\site-packages` using `pip install --target "$StagingDir\Lib\site-packages" -r requirements.txt`.
4. Append `Lib\site-packages` to the `._pth` file.
5. Copy `ansimuz.exe`, `backend\`, and `assets\` next to `python3XX.dll`.
6. Package into `ansimuz-windows-v<version>-x64.rar`.

---

## Validation Before Submitting

Before submitting a pull request, verify that all local checks pass cleanly:

```bash
# 1. Check code formatting
cargo fmt --check

# 2. Check compilation across all targets
cargo check --all-targets

# 3. Run Clippy lints (warnings treated as errors)
cargo clippy --all-targets -- -D warnings

# 4. Run all Rust unit and integration tests
cargo test

# 5. Run subsystem-specific test suites:
cargo test lyrics                   # All 76 lyrics unit and integration tests
cargo test --lib discord::          # Discord activity, worker, and cache tests
cargo test --lib playback::         # MPV process supervision and IPC transport tests
cargo test --test queue             # Queue operations and cursor focus
cargo test --test queue_playback    # Queue priority, repeat modes, error recovery
cargo test --test resolution        # Async track resolution, stale generation rejection
cargo test --test lifecycle         # Explicit Ended/Failed lifecycle events, track switching
cargo test --test memory_session    # Bounded cache capacities, memory stability
cargo test --test artwork           # Disk caching, format detection, corruption recovery

# 6. Verify Python syntax and run backend unit tests
python -m py_compile backend/bridge.py backend/exceptions.py backend/providers/lyrics/*.py backend/providers/lyrics/parsers/*.py
python -m unittest discover -s backend/tests -p "test_*.py" -v
```

---

## Submitting Pull Requests

1. Create a descriptive feature branch:
   ```bash
   git checkout -b feature/my-new-feature
   ```
2. Make concise, focused commits with meaningful commit messages.
3. Push to your fork and submit a Pull Request targeting the `main` branch.
4. Ensure all CI checks pass. Provide a clear description of the problem, the solution, and how the changes were verified.
