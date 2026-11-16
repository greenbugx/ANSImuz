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

Ensure the following tools are installed on your system:

- **Rust toolchain**: 1.85+ (compatible with the Rust 2024 edition).
- **Python**: 3.10 or higher (with development headers for PyO3 linking).
- **[mpv](https://mpv.io/)**: Version 0.35+ recommended, with JSON IPC enabled (Unix-domain sockets on Linux, Named Pipes on Windows).
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)**: For stream resolution and media extraction.
- **OpenSSL / libssl-dev**: Required for native network operations.
---

## Setting Up Your Environment

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
   ```
5. **Run ANSImuz locally**:
   ```bash
   cargo run
   ```

---

## Project Architecture

The codebase is organized into modular boundaries:

```
ANSImuz/
├── src/
│   ├── app/          # Application loop, state machine, and orchestration
│   │   ├── runner.rs     # Main application event loop and terminal interaction
│   │   ├── playback.rs   # Playback lifecycle, queue sequencing, history, and resolution
│   │   ├── search.rs     # Search result handling, refetching, and pagination
│   │   ├── events.rs     # Raw key event handling and input routing
│   │   ├── commands.rs   # Command palette actions and dispatch
│   │   ├── state.rs      # Authoritative PlaybackState and application state
│   │   └── mod.rs        # App module re-exports
│   ├── artwork/      # Image decoding, bounded memory cache, resize cache, renderer
│   ├── config/       # Settings persistence (TOML), platform cache dirs, atomic writes
│   ├── discord/      # Discord Rich Presence subsystem
│   │   ├── activity.rs   # Listening activity formatting, timestamps, buttons, UTF-8 truncation
│   │   ├── client.rs     # Low-level Discord IPC transport client wrapper
│   │   ├── worker.rs     # Background worker actor, connection lifecycle & exponential backoff
│   │   └── artwork/      # Discord artwork resolution, normalization, and upload pipeline
│   │       ├── types.rs      # ArtworkRequest/Result, byte-level format detection, MIME types
│   │       ├── downloader.rs # Async HTTPS download, aspect ratio preserving resize & bounds check
│   │       ├── uploader.rs   # Multipart uploaders (Imgdb, Catbox, Litterbox), SSRF validation
│   │       ├── cache.rs      # Persistent disk cache (index.json, images/), LRU pruning
│   │       ├── manager.rs    # Orchestrator, request coalescing, provider cooldown matrix
│   │       └── worker.rs     # Async artwork worker thread & channel forwarder
│   ├── music/        # Track model, identity policy, backend PyO3 bridge execution
│   ├── playback/     # MPV IPC process supervision, cross-platform transport (ipc.rs), audio prefetch
│   │   ├── events.rs     # PlayerEvent enum (Position, Ended, Failed)
│   │   ├── ipc.rs        # Cross-platform local IPC endpoints & streams (interprocess)
│   │   ├── mpv.rs        # AudioPlayer implementation, observer loop, stream dumping
│   │   ├── repeat.rs     # Repeat mode sequencing
│   │   └── mod.rs        # Playback module exports
│   ├── terminal/     # Terminal raw mode setup and clean teardown
│   ├── ui/           # Ratatui components (queue_list, track_list), screens, theme
│   ├── paths.rs      # Runtime data directory resolution (backend/ and assets/)
│   ├── lib.rs        # Library interface for integration tests
│   └── main.rs       # Application entry point (cleans stale caches, boots TUI)
├── backend/
│   ├── bridge.py     # PyO3 integration boundary
│   ├── exceptions.py # Structured provider exception hierarchy
│   ├── artwork/      # Python thumbnail utilities and prewarming
│   ├── providers/    # Music provider implementations (ytmusic, spotify)
│   └── tests/        # Python backend unit and smoke tests
├── assets/           # Logo and animation frames
├── scripts/          # Linux and Windows installer scripts
├── packaging/
│   ├── arch/         # PKGBUILD for Arch Linux
│   └── rpm/          # RPM specs for Fedora (COPR)
├── vendor/
│   └── presenceforge/ # Vendored cross-platform Discord IPC crate (Unix domain sockets & Windows named pipes)
├── tests/            # Integration test suites:
│   ├── queue.rs              # Queue manipulation and focus interactions
│   ├── queue_playback.rs     # Queue priority, repeat modes, failure continuation
│   ├── resolution.rs         # Generation tracking, slot identity, cancellation
│   ├── lifecycle.rs          # Explicit Ended/Failed events, shutdown, track switching
│   ├── memory_session.rs     # Long-session cache bounds, memory leakage, cache clearing
│   ├── artwork.rs            # Disk cache persistence, corruption handling
│   └── discord_connection.rs # Discord client connection lifecycle and reconnect contracts
└── .github/          # CI workflows
```

---

## Code Guidelines & Best Practices
- **Non-blocking TUI**: Never block the main TUI rendering thread. Long-running tasks (such as network requests or stream resolutions) must execute asynchronously in background threads and communicate via channels.
- **Queue & Playback Separation**: Search results (`app.tracks`) are strictly for track discovery. Playback sequencing (`Next`, `Previous`, `Shuffle`, `Repeat`) operates exclusively on the playback queue (`app.queue`) and session history (`app.history`), never cycling through search results.
- **Queue Deduplication**: Use `app.enqueue()` or `app.queue_contains()` to prevent duplicate tracks from being added to the queue.
- **Unicode Safety**: Never slice strings by raw byte offsets when displaying text in the TUI. Use character-based bounds or the project's `truncate` helper (`crate::ui::theme::truncate`) to prevent panics on multibyte characters.
- **Runtime Data Paths**: Locate `backend/` and `assets/` only through `crate::paths::data_dir()`. Never use `env!("CARGO_MANIFEST_DIR")` or other compile-time paths for files needed at runtime, because that bakes the build directory into release binaries and breaks packaged installs. Use `option_env!` only for optional build-time overrides such as `ANSIMUZ_DATA_DIR`.
- **Cross-Platform IPC**: MPV communication uses `IpcEndpoint` and `IpcStream` (`crate::playback::ipc`). On Unix, endpoints use filesystem paths (`.sock`); on Windows, endpoints use named pipes (`\\.\pipe\...`). Never invoke filesystem existence checks or deletions (`Path::exists()`, `fs::remove_file()`) on Windows named pipes.
- **Observer Thread Concurrency & Reaping**: The observer decouples periodic query writing from continuous response reading to support platforms (such as Windows named pipes) that lack socket-level read timeouts. Background writer threads must always be wrapped in a RAII guard (`WriterThreadGuard`) and joined when the observer exits, and `AudioPlayer::stop()` must join the observer thread.
- **Test Concurrency & State Isolation**: Tests that mutate shared global caches (such as `RESIZE_CACHE` or `art_cache()`) must acquire `crate::artwork::ARTWORK_TEST_LOCK` to prevent race conditions during parallel test execution. Self-contained data structure tests (like `ResizeCache`) must instantiate local, isolated instances.
- **Discord Isolation**: Discord RPC must never block the main TUI rendering thread or audio playback loop. All Discord activity formatting and IPC transmission run in `DiscordWorker`, while network downloads, image resizing, and uploads run in `ArtworkWorker`.
- **Discord Playback Synchronization & Stale Event Rejection**: Player generation counters ensure rapid track skipping, cancellations, or out-of-order player events never publish stale presence to Discord. Presence must be cleared immediately when playback finishes, halts on error, or is manually stopped.
- **Artwork Security & Format Validation**: Never infer image format from file extensions or URLs. Use `NormalizedImageFormat::from_bytes` to validate magic bytes. Reject private IPs, localhost, and non-HTTPS destinations before initiating upload requests.
- **Style & Consistency**: Keep code formatting consistent. Avoid unnecessary abstractions or dead code.
- **Comments**: Keep comments minimal and focused strictly on explaining non-obvious requirements.

---

## Discord Rich Presence (RPC) Architecture

The Discord Rich Presence subsystem (`src/discord/`) integrates real-time playback state with Discord's desktop client over local Inter-Process Communication (IPC).

### Subsystem Structure

1. **Activity Builder (`src/discord/activity.rs`)**:
   - Constructs Discord `Activity` payloads (`ActivityType::Listening`).
   - Details: Track title; State: Artist + Album (or ANSImuz fallback).
   - Timestamps: Live start and end timestamps calculated for playing tracks; timestamps omitted when paused.
   - Interactive Buttons: "Listen on YouTube Music" or "Listen on Spotify" (pointing to public canonical URLs) and "Get ANSImuz". Temporary streaming URLs are strictly rejected.
   - Truncation: Safe UTF-8 grapheme truncation preventing multibyte panic or Discord API rejection.

2. **IPC Client & Transport (`src/discord/client.rs` & `vendor/presenceforge/`)**:
   - Thin wrapper around `presenceforge::DiscordIpcClient`.
   - Cross-platform local IPC: Unix-domain sockets (`/run/user/.../discord-ipc-0`) on Linux; Windows Named Pipes (`\\.\pipe\discord-ipc-0`) on Windows.
   - Offline safety: If Discord is closed or unreachable, calls return `DiscordError::NotConnected` safely without blocking the application.

3. **Background Worker Actor (`src/discord/worker.rs`)**:
   - Supervised background thread receiving `DiscordCommand` events (`TrackChanged`, `PositionChanged`, `Paused`, `Resumed`, `Clear`).
   - Auto-reconnection with exponential backoff: 10s → 20s → 40s → capped at 60s. Backoff resets immediately upon successful handshake.
   - Idempotent shutdown and graceful channel draining on application exit.

4. **Artwork Pipeline (`src/discord/artwork/`)**:
   - **Format Detection (`types.rs`)**: Magic byte inspection for JPEG, PNG, WebP, GIF, and AVIF. Rejects corrupted, empty, or unsupported formats (BMP, SVG).
   - **Downloader (`downloader.rs`)**: Asynchronous HTTPS downloader with dimension bounds checks ($10\times 10$ to $4096\times 4096$) and aspect-ratio preserving downscaling.
   - **Uploader (`uploader.rs`)**: Multi-host multipart uploaders for Imgdb, Catbox, and Litterbox. Validates returned URLs, enforces SSRF protections (blocking localhost, RFC-1918 private IPs, and credentialed URLs), and parses `Retry-After` headers.
   - **Cache (`cache.rs`)**: Persistent disk cache (`index.json` + `images/` directory) with SHA-256 content deduplication, LRU pruning to `cache_max_size_mb`, and file preservation across expiration cycles.
   - **Manager (`manager.rs`)**: Orchestrates in-flight request coalescing, fallback chains (`Imgdb` → `Catbox` → `Litterbox` → default logo), and provider error cooldown matrices (5xx, timeouts, malformed responses apply cooldowns; invalid inputs do not penalize hosts).
   - **Worker (`worker.rs`)**: Decoupled async worker thread and response forwarder.

---

## Packaging (Arch Linux)

The `PKGBUILD` in `packaging/arch/` builds the latest `main` as `ansimuz-git`. It installs the binary to `/usr/bin/ansimuz` and `backend/` and `assets/` to `/usr/share/ansimuz/`, and builds with `ANSIMUZ_DATA_DIR=/usr/share/ansimuz`.

Update it when you:

- add, remove, or rename a Python or system dependency (keep `depends` in sync with `requirements.txt`; `python-spotifyscraper` comes from the AUR)
- change how runtime data is located, or add new top-level runtime directories that must be installed
- change the license or add files that should be installed alongside the binary

To test a change to the PKGBUILD:

```bash
cd packaging/arch
makepkg -sf
namcap PKGBUILD
sudo pacman -U ansimuz-git-*.pkg.tar.zst
cd /tmp && ansimuz
```

Run the installed binary from a directory outside the repository to confirm search, playback, and animation frames work without the source tree. `makepkg` should not print `Package contains reference to $srcdir`. Warnings from `namcap` about `providers.*` and `artwork.*` Python modules are expected; they are the app's own modules, not missing dependencies. Do not commit `src/`, `pkg/`, or built packages.

---

## Packaging (Fedora / COPR)

`packaging/rpm/` contains two RPM specs, built in the [`greenbugx/ansimuz`](https://copr.fedorainfracloud.org/coprs/greenbugx/ansimuz/) COPR project:

- `ansimuz.spec` builds ANSImuz from a **pinned commit** (`%global commit`) matching the release version. It installs the binary to `/usr/bin/ansimuz` and `backend/` and `assets/` to `/usr/share/ansimuz/`, and builds with `ANSIMUZ_DATA_DIR=/usr/share/ansimuz`.
- `python-spotifyscraper.spec` packages `spotifyscraper`, which is not in the Fedora repositories. `ansimuz` requires it as `python3dist(spotifyscraper)`.

Update the specs when you:

- add, remove, or rename a Python or system dependency (keep `Requires` in sync with `requirements.txt`)
- change how runtime data is located, or add new top-level runtime directories that must be installed
- change the license or add files that should be installed alongside the binary

To move `ansimuz.spec` to a newer commit or release:

```bash
cd packaging/rpm
NEW=$(git rev-parse HEAD)
sed -i "s/^%global commit .*/%global commit $NEW/" ansimuz.spec
```

Then update `Version: <new_version>`, add a `%changelog` entry, and reset `Release` to `1%{?dist}`.

To test a spec before uploading to COPR:

```bash
rm -f ~/rpmbuild/SRPMS/ansimuz-*.src.rpm    # mock rejects a glob that matches several SRPMs
spectool -g -R ansimuz.spec
rpmbuild -bs ansimuz.spec
mock -r fedora-rawhide-x86_64 --enable-network rebuild ~/rpmbuild/SRPMS/ansimuz-*.src.rpm
```

Notes for maintainers:

- The COPR project needs **"Enable internet access during builds"** turned on, because `ansimuz.spec` runs `cargo fetch`.
- Build `python-spotifyscraper` before `ansimuz`, and build both for **every Fedora release enabled in the project**. Packages depend on that release's Python version, so a release missing a build fails to install with `nothing provides python(abi) = ...`.
- Do not commit `*.rpm`, `*.src.rpm`, or source tarballs.


---

## Packaging (Windows Portable Release)

ANSImuz provides a portable Windows distribution with an embedded Python runtime (`ansimuz-windows-v<version>-x64.rar`) for Windows 10/11. Users do not need a separate Python installation. MPV and yt-dlp must be installed and available in `PATH`.

### Bundle Structure

```text
ansimuz-windows-v<version>-x64/
├── ansimuz.exe              # Release binary compiled with MSVC
├── assets/                  # Logos and visualizer animation frames
│   ├── frames/
│   ├── logo_nobg.png 
│   └── logo.png      
├── backend/                 # PyO3 Python bridge and providers
│   ├── bridge.py
│   ├── exceptions.py
│   ├── artwork/
│   └── providers/
├── python314.dll            # Python dynamic library (matching PyO3 build version)
├── python3.dll              # Python stable ABI stub
├── python314.zip            # Standard library archive from embeddable package
├── python314._pth           # Module search path configuration
├── *.pyd                    # Native C extension modules from embeddable package (_socket, _ssl, etc.)
└── Lib/
    └── site-packages/       # Organized dependencies (requests, ytmusicapi, spotifyscraper, pillow)
```

### Packaging Steps

1. **Build the release binary**:
   
   ```powershell
   cargo build --release
   ```

2. **Download the matching Python Windows Embeddable Package**:

   Download `python-3.x.x-embed-amd64.zip` from [python.org](https://www.python.org/downloads/windows/) matching your build toolchain's Python version. Extract its contents into the staging directory.

3. **Install Python dependencies into the organized `Lib\site-packages` subfolder**:

   Run this command from your **ANSImuz repository root** using your development environment's `pip` (pip is not included in the embeddable `python.exe`):

   ```powershell
   # From the repository root using pip:
   pip install --target "$StagingDir\Lib\site-packages" -r requirements.txt

   # Or using your virtual environment Python:
   .\.venv\Scripts\python.exe -m pip install --target "$StagingDir\Lib\site-packages" -r requirements.txt
   ```
4. **Append `Lib\site-packages` to the `._pth` file**:

   Open the `._pth` file in the staging directory (e.g. `python314._pth`) and add:
   
   ```text
   Lib\site-packages
   ```
   
   *(Note: ANSImuz also auto-detects `Lib/site-packages` at startup).*

5. **Copy assets and binaries**:

   Copy `target\release\ansimuz.exe`, `backend\`, and `assets\` into the staging directory next to `python3XX.dll`.

6. **Create the RAR archive**:
   
   Create the release archive using WinRAR or `rar.exe` CLI:

   ```powershell
   rar a -ep1 -r "ansimuz-windows-v<version>-x64.rar" "$StagingDir\*"
   ```

### Verifying the Portable Release

To verify the portable bundle before publishing:
1. Extract the `.rar` archive on a clean Windows machine (or in a sandbox/VM) without Python registered in system `PATH`.
2. Ensure `mpv` and `yt-dlp` are installed in `PATH` (e.g. `winget install --id shinchiro.mpv -e` and `winget install --id yt-dlp.yt-dlp -e`).
3. Run `ansimuz.exe` from the extracted directory and confirm search, playback, artwork decoding, and animation frames initialize properly.
---

## Release Workflow

ANSImuz produces releases through a combination of automated CI artifact generation and manual package distribution:

### 1. GitHub Actions (Automated Package Generation)

When a release tag is pushed (e.g. `v0.1.3`) or triggered manually via `workflow_dispatch` on [`.github/workflows/release.yml`](.github/workflows/release.yml):
- **Validation**: Verifies that the tag format matches `Cargo.toml`, checks that `packaging/rpm/ansimuz.spec` references an ancestor commit of the tag, and verifies spec dependencies.
- **Arch Linux Package (`arch-package`)**: Compiles the release binary in an `archlinux:latest` container and packages `ansimuz-git-*.pkg.tar.zst`.
- **Fedora Source RPM (`fedora-srpm`)**: Uses `spectool` and `rpmbuild -bs` in a `fedora:45` container to build `ansimuz-*.src.rpm`.

### 2. Manual Fedora COPR Publishing

1. Download the generated `ansimuz-*.src.rpm` from the GitHub Actions run artifacts.
2. Upload the SRPM to the [`greenbugx/ansimuz`](https://copr.fedorainfracloud.org/coprs/greenbugx/ansimuz/) COPR project (via the web UI or `copr-cli build greenbugx/ansimuz ansimuz-*.src.rpm`).
3. Wait for COPR chroots (`fedora-43`, `fedora-44`, `fedora-45`, `fedora-rawhide`) to finish building.
4. Download the compiled binary RPM packages (`ansimuz-*.rpm`).

*(Note: `python-spotifyscraper` is maintained in the same COPR repository and does not need to be rebuilt unless its upstream package changes).*

### 3. Windows Portable Bundle Creation

Follow the [Packaging (Windows Portable Release)](#packaging-windows-portable-release) procedure to create `ansimuz-windows-v<version>-x64.rar`.

### 4. Manual GitHub Release

1. Create a GitHub Release corresponding to the version tag (e.g. `v0.1.x`).
2. Generate SHA256 checksums across all distribution assets:

```bash
   sha256sum ansimuz-git-*.pkg.tar.zst ansimuz-*.rpm ansimuz-windows-*.rar > SHA256SUMS
```

3. Attach the release assets to the GitHub Release:
   - Arch Linux package (`ansimuz-git-*.pkg.tar.zst`)
   - Fedora binary RPM packages (`ansimuz-*.rpm`)
   - Windows portable archive (`ansimuz-windows-v<version>-x64.rar`)
   - `SHA256SUMS` checksum verification file

## Validation Before Submitting

Before submitting a pull request, verify that all local checks pass cleanly:

```bash
# Check formatting
cargo fmt --check

# Check compilation across all targets
cargo check --all-targets

# Run Clippy lints (warnings treated as errors)
cargo clippy --all-targets -- -D warnings

# Run all unit and integration tests
cargo test

# Run a specific integration test target:
cargo test --test queue             # Queue operations and cursor focus
cargo test --test queue_playback    # Queue priority, repeat modes, error recovery
cargo test --test resolution        # Async track resolution, stale generation rejection
cargo test --test lifecycle         # Explicit Ended/Failed lifecycle events, track switching
cargo test --test memory_session    # Bounded cache capacities, long session memory stability
cargo test --test artwork           # Disk caching, file format detection, corruption recovery
cargo test --test discord_connection # Discord IPC client connection contracts
cargo test --lib discord::          # Discord activity, worker, uploader, and cache unit tests

# Check Python syntax and run backend tests
python -m py_compile backend/bridge.py backend/exceptions.py backend/artwork/thumbnails.py backend/providers/spotify/spotify.py backend/providers/ytmusic/ytmusic.py
python -m unittest discover -s backend/tests -p "test_*.py" -v
```

### Testing Discord Rich Presence (RPC) & IPC

ANSImuz tests all 117 Discord RPC integration behaviors (activity formatting, worker reconnection backoff, byte format detection, SSRF protection, multi-provider fallbacks, cache invalidation, and playback lifecycle synchronization) offline using mock recorders and isolated temporary directories:

```bash
# Run all Discord-specific unit and integration tests:
cargo test --lib discord::
cargo test --test discord_connection

# Run with an isolated cache directory if running in restricted environments:
# Linux/macOS:
XDG_CACHE_HOME=/tmp/ansimuz_test_cache cargo test --lib discord::
# Windows (PowerShell):
$env:XDG_CACHE_HOME='C:\Users\<user>\AppData\Local\Temp\ansimuz_test_cache'; cargo test --lib discord::
```

### Testing MPV Process Supervision & IPC

ANSImuz separates player orchestration from system processes using `AudioPlayer::new_mock()`, allowing all test suites to run deterministically offline without requiring audio hardware.

To test real MPV process lifecycle management, graceful shutdown, SIGKILL timeouts, and JSON IPC parsing:

```bash
# Run MPV supervision and cross-platform IPC transport unit tests:
cargo test --lib playback::

# Smoke test live audio playback and streaming (requires internet and active audio device):
cargo run
```

### Network Access in Tests

- **Offline Tests**: All Rust unit tests (`cargo test --lib`), integration suites (`cargo test --test <name>`), and Python unit tests run 100% offline using mock drivers.
- **Online Operations**: Live searching, streaming, and artwork prefetching during `cargo run` require an active internet connection.
---

## Continuous Integration (CI)

Our GitHub Actions pipeline automatically checks every push and pull request targeting `main` across multiple platforms:

- **Linux** (`ubuntu-latest`): Formatting, type checks, Clippy, unit and integration test suites, and build.
- **Windows** (`windows-latest`): Python tests, format check, compilation, Clippy, verbose tests, and release build on native MSVC toolchain.

CI automatically skips runs when only documentation (`*.md`), `LICENSE`, or `.gitignore` files are changed.

## Submitting Pull Requests

1. Create a descriptive feature branch:
   ```bash
   git checkout -b feature/my-new-feature
   ```
2. Make concise, focused commits with meaningful commit messages.
3. Push to your fork and submit a Pull Request targeting the `main` branch.
4. Ensure all CI checks pass. Provide a clear description of the problem, the solution, and how the changes were verified.
