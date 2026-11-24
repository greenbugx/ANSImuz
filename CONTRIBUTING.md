# Contributing to ANSImuz

Thank you for your interest in contributing to ANSImuz! We welcome bug reports, feature suggestions, documentation improvements, and code contributions.

Please review this guide before submitting a pull request or opening an issue.

---

## Code of Conduct

All contributors and participants are expected to adhere to our [Code of Conduct](CODE_OF_CONDUCT.md). Please treat others with respect and keep our community welcoming and collaborative.

---

## Supported Operating Systems & Prerequisites

ANSImuz supports:
- **Linux** (x86_64): Primary platform (tested on Arch Linux, Fedora, Debian/Ubuntu).
- **Windows** (x86_64): Windows 10/11 support via native libmpv or PowerShell installer (`scripts/install_windows.ps1`). Also supported through WSL2.

Ensure the following tools and development libraries are installed on your system:

### Required Toolchains & Libraries
- **Rust toolchain**: 1.85+ (compatible with the Rust 2024 edition).
- **Python**: 3.10 or higher with development headers (`python3-dev` / `python3-devel`) for PyO3 linking.
- **C Compiler & Build Tools**:
  - Linux: `gcc` / `clang`, OpenSSL development headers (`libssl-dev` / `openssl-devel`), and `pkg-config`.
  - Windows: MSVC toolchain (C++ Build Tools with MSVC and Windows SDK) and `7z` (7-Zip on system PATH).
- **libmpv Development Libraries**:
  Native playback links directly to the C library API without spawning an external player process.
  - Linux: The shared library headers are required for compiling (`libmpv-dev` on Debian/Ubuntu, `mpv-libs-devel` on Fedora, or `mpv` on Arch Linux).
  - Windows: The dev archive and MSVC import library are set up automatically using `.\scripts\setup_mpv_windows.ps1`.
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)**: For stream URL resolution and media extraction.
- **[mpv](https://mpv.io/)** (CLI binary): Only needed as an optional fallback when compiling with `--no-default-features`.

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

4. **Set up libmpv development libraries**:

   **Windows (PowerShell)**:
   ```powershell
   .\scripts\setup_mpv_windows.ps1
   ```
   This downloads the pinned libmpv developer package (`native/mpv/WINDOWS_LIBMPV`), verifies its SHA256 checksum, places the C headers and dynamic library in `native/mpv/`, and generates the MSVC import library `native/mpv/lib/mpv.lib`.

   **Linux (Debian / Ubuntu)**:
   ```bash
   sudo apt install -y libssl-dev libmpv-dev pkg-config
   ```

   **Linux (Fedora)**:
   ```bash
   sudo dnf install -y openssl-devel mpv-libs-devel pkg-config
   ```

   **Linux (Arch Linux)**:
   ```bash
   sudo pacman -S --needed openssl mpv pkgconf
   ```

5. **Verify the build and tests**:
   ```bash
   cargo fmt --check
   cargo check --all-targets
   cargo check --no-default-features --all-targets
   cargo clippy --all-targets -- -D warnings
   cargo clippy --no-default-features --all-targets -- -D warnings
   cargo test
   python -m unittest discover -s backend/tests -p "test_*.py"
   ```

6. **Run ANSImuz locally**:
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
├── native/
│   └── mpv/
│       ├── include/          # Native C headers
│       ├── lib/              # libmpv-2.dll, mpv.lib import library, mpv.def
│       ├── src/              # C bridge implementation
│       ├── tests/            # C bridge test suite
│       └── WINDOWS_LIBMPV    # Pinned archive URL and SHA256 checksum
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
│   ├── playback/         # Audio playback subsystem
│   │   ├── events.rs         # PlayerEvent enum (Position, Ended, Failed)
│   │   ├── native_mpv.rs     # Safe Rust FFI wrapper around ansimuz_mpv_bridge
│   │   ├── native_backend.rs # NativeBackend implementation and stream prefetching
│   │   ├── native_adapter.rs # Zero-allocation event translator for native mpv events
│   │   ├── mpv.rs            # Fallback CLI player (CliPlayer) via IPC
│   │   ├── ipc.rs            # Cross-platform local IPC endpoints and streams (interprocess)
│   │   ├── player.rs         # Unified AudioPlayer facade with automatic backend selection
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
- **Cross-Platform IPC**: MPV communication uses `IpcEndpoint` and `IpcStream` (`crate::playback::ipc`). On Unix, endpoints use filesystem paths (`.sock`), and on Windows, endpoints use named pipes (`\\.\pipe\...`). Never invoke filesystem existence checks or deletions (`Path::exists()`, `fs::remove_file()`) on Windows named pipes.
- **Native libmpv Safety**: Never allocate dynamic heap structures or parse recursive property nodes across FFI in high-frequency event loops. All track transitions must use direct O(1) state correlation. Always disable internal script engines (`load-scripts=no`, `osc=no`, `input-default-bindings=no`) to ensure libmpv never corrupts terminal TUI drawing.
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
  - Explicit scroll policy: Viewport automatically follows the active lyric. When manual scroll is triggered (`k`/`j`/`PageUp`/`PageDown`), an indicator badge is shown, and pressing `c` re-centers and resumes auto-scroll.
  - Transition guard: `resolve_sync_point` verifies track and generation identity to prevent flashing stale active lyrics across song changes.

---

### 2. Discord Rich Presence (RPC) Subsystem

The Discord Rich Presence subsystem (`src/discord/`) integrates real-time playback state with Discord's desktop client over local Inter-Process Communication (IPC).

1. **Activity Builder (`src/discord/activity.rs`)**:
   - Constructs Discord `Activity` payloads (`ActivityType::Listening`).
   - Timestamps: Live start and end timestamps calculated for playing tracks, with timestamps omitted when paused.
   - Interactive Buttons: "Listen on YouTube Music" or "Listen on Spotify" (pointing to public canonical URLs) and "Get ANSImuz". Temporary streaming URLs are strictly rejected.
   - Truncation: Safe UTF-8 grapheme truncation preventing multibyte panic or Discord API rejection.
   
2. **IPC Client & Transport (`src/discord/client.rs` & `vendor/presenceforge/`)**:
   - Cross-platform local IPC: Unix-domain sockets (`/run/user/.../discord-ipc-0`) on Linux, and Windows Named Pipes (`\\.\pipe\discord-ipc-0`) on Windows.
   - Offline safety: If Discord is closed or unreachable, calls return `DiscordError::NotConnected` safely without blocking the application.

3. **Background Worker Actor (`src/discord/worker.rs`)**:
   - Supervised background thread receiving `DiscordCommand` events.
   - Auto-reconnection with exponential backoff: 10s → 20s → 40s → capped at 60s. Backoff resets immediately upon successful handshake.
   
4. **Artwork Pipeline (`src/discord/artwork/`)**:
   - Format detection, dimensions checks, multipart uploaders (Imgdb, Catbox, Litterbox), SSRF validation, and persistent disk caching with LRU pruning.

---

### 3. Playback Subsystem Architecture (Native libmpv & CLI Fallback)

The playback subsystem (`src/playback/` and `native/mpv/`) manages audio playback, stream decoding, volume control, track transition lifecycles, and background stream prefetching.

#### A. Dual-Backend Strategy
ANSImuz supports two interchangeable playback engines behind a unified facade (`AudioPlayer` in `src/playback/player.rs`):
- **Native libmpv Backend (`NativeBackend` in `src/playback/native_backend.rs`)**:
  Enabled by default via the `native-mpv` Cargo feature. It links directly against `libmpv-2.dll` on Windows and `-lmpv` on Linux through an isolated C bridge. It eliminates process supervision overhead, removes external CLI binary requirements, and provides instantaneous track replacement. The command-line `mpv` executable is never invoked on either Windows or Linux.
- **CLI Fallback Backend (`CliPlayer` in `src/playback/mpv.rs`)**:
  Used when compiled with `--no-default-features`, or when runtime native initialization fails. It spawns an external `mpv` process and controls it via cross-platform JSON IPC (Unix domain sockets on Linux, Named Pipes on Windows).

#### B. The Native C Bridge (`native/mpv/src/mpv_bridge.c` and `native/mpv/include/ansimuz_mpv.h`)
- **ABI Isolation**: The C bridge encapsulates `mpv_handle` within an opaque pointer (`ansimuz_mpv`). This insulates the Rust FFI layer from upstream libmpv client header drift.
- **Zero-Allocation Track Correlation**:
  Rather than allocating heap ring buffers or traversing recursive playlist node trees, the bridge tracks `current_load_id`, `active_load_id`, `current_entry_id`, and `previous_entry_id` in O(1) storage.
  When a new track loads via `loadfile` replace mode, libmpv emits `MPV_EVENT_END_FILE` for the displaced track. The bridge correlates the displaced track's entry ID with `previous_entry_id` and tags it with `previous_load_id`. The Rust translator drops the displaced stop event without disrupting active track playback.
- **TUI Display & Playback Isolation**:
  Terminal music players are sensitive to background engine logs corrupting terminal cells or stale engine state across transitions. During `ansimuz_mpv_create`, the bridge explicitly sets:
  - `terminal=no` and `msg-level=all=no`: Silences console and stderr logging.
  - `load-scripts=no`: Stops user/external Lua/JS scripts from loading while keeping built-in hooks isolated.
  - `osc=no`, `input-default-bindings=no`, and `input-vo-keyboard=no`: Disables on-screen controllers and keyboard intercepts.
  - `reset-on-next-file=pause`: Clears pause mode automatically whenever a replacement track opens (matched in `NativeBackend::play` and `Translator::set_paused`).
  - `ao` fallback (`wasapi,null` on Windows, `auto,null` on Linux): Respects `ANSIMUZ_MPV_AO` (via both `getenv` and Windows `GetEnvironmentVariableA`) and falls back to `null` audio on headless environments.
  - Selective `ytdl` and client fallback: Enables `ytdl` for YouTube URLs while disabling it for direct/local targets in `ansimuz_mpv_loadfile`, and passes the `YTDL_PLAYER_CLIENTS` chain to `ytdl-raw-options` via `MPV_FORMAT_NODE_MAP` so commas inside `extractor-args` are preserved.
- **Discrete Property Observation**:
  Registers discrete integer reply tags for `time-pos`, `duration`, `pause`, `volume`, and `mute`. Property change notifications are dispatched without string lookups.
- **Rust Event Translation (`src/playback/native_adapter.rs`)**:
  The `Translator` struct maps raw C event structs into high-level `PlayerEvent` variants (`Position`, `Ended`, `Failed`). It drops events belonging to superseded load IDs and filters high-frequency position ticks.

#### C. Headless Stream Prefetching
To ensure gapless transitions between queued tracks, ANSImuz prefetches upcoming songs into the local disk cache:
- When a queued song is scheduled for prefetch, `NativeBackend::prefetch_later` invokes `ansimuz_mpv_dump_stream`.
- A dedicated secondary headless `mpv_handle` is initialized with `stream-dump`, `network-timeout=30`, and the same selective `ytdl` and `MPV_FORMAT_NODE_MAP` YouTube client fallback chain.
- The dump loop polls in a background thread while querying Rust's `AtomicBool` cancellation flag safely through an `extern "C"` callback (`ansimuz_mpv_cancel_fn`), avoiding unsynchronized cross-language pointer reads.
- Prefetching enforces CLI-equivalent safety bounds (`64 MiB` maximum dump size, `100 KiB` minimum cached size, and a `600s` timeout), terminating `mpv` via `mpv_terminate_destroy` and deleting incomplete `.part` files on cancellation or limit violations.
- Once completed, the file is atomically renamed into the audio cache directory (`~/.cache/ansimuz/audio/` or `%LOCALAPPDATA%\ansimuz\audio`). When the track plays, it loads instantly from disk.

#### D. Build Automation and DLL Deployment (`build.rs`)
- `build.rs` compiles `native/mpv/src/mpv_bridge.c` using the `cc` crate and links against `mpv.lib` (Windows) or `-lmpv` (Linux).
- On Windows, `build.rs` automatically detects the compiled output directory (`target/debug` or `target/release`) and copies `libmpv-2.dll` directly next to `ansimuz.exe`. Developers and users can run `cargo run` without manually modifying system PATH or copying DLLs.

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
5. Copy `ansimuz.exe`, `libmpv-2.dll`, `backend\`, and `assets\` next to `python3XX.dll`.
6. Package into `ansimuz-windows-v<version>-x64.rar`.

---

## Validation Before Submitting

Before submitting a pull request, verify that all local checks pass cleanly:

```bash
# 1. Check code formatting
cargo fmt --check

# 2. Check compilation across both feature sets
cargo check --all-targets
cargo check --no-default-features --all-targets

# 3. Run Clippy lints across both feature sets (warnings treated as errors)
cargo clippy --all-targets -- -D warnings
cargo clippy --no-default-features --all-targets -- -D warnings

# 4. Run all Rust unit and integration tests
cargo test

# 5. Run native libmpv smoke and playback tests (headless with null audio output)
# Linux:
ANSIMUZ_MPV_AO=null cargo test --test native_smoke -- --test-threads=1
ANSIMUZ_MPV_AO=null cargo test --test queue_playback
cargo test --lib playback

# Windows PowerShell:
$env:ANSIMUZ_MPV_AO="null"
$nativeLib = (Resolve-Path "native/mpv/lib").Path
$env:PATH = [string]::Join([System.IO.Path]::PathSeparator, @($nativeLib, $env:PATH))
cargo test --test native_smoke -- --test-threads=1
cargo test --test queue_playback
cargo test --lib playback

# 6. Run subsystem-specific test suites
cargo test lyrics                   # All 76 lyrics unit and integration tests
cargo test --lib discord::          # Discord activity, worker, and cache tests
cargo test --lib playback::         # Playback engine and IPC transport tests
cargo test --test queue             # Queue operations and cursor focus
cargo test --test queue_playback    # Queue priority, repeat modes, error recovery
cargo test --test resolution        # Async track resolution, stale generation rejection
cargo test --test lifecycle         # Explicit Ended/Failed lifecycle events, track switching
cargo test --test memory_session    # Bounded cache capacities, memory stability
cargo test --test artwork           # Disk caching, format detection, corruption recovery

# 7. Verify Python syntax and run backend unit tests
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
