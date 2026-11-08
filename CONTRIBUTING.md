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
- **Windows** (x86_64): Native Windows 10/11 support via PowerShell installer (`scripts/install_windows.ps1`), native `mpv`/`yt-dlp`, and cross-platform IPC over Windows Named Pipes (`\\.\pipe\...`) and also supported through `WSL2`.

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
   ```bash
   python -m venv .venv
   source .venv/bin/activate
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
├── tests/            # Integration test suites:
│   ├── queue.rs          # Queue manipulation and focus interactions
│   ├── queue_playback.rs # Queue priority, repeat modes, failure continuation
│   ├── resolution.rs     # Generation tracking, slot identity, cancellation
│   ├── lifecycle.rs      # Explicit Ended/Failed events, shutdown, track switching
│   ├── memory_session.rs # Long-session cache bounds, memory leakage, cache clearing
│   └── artwork.rs        # Disk cache persistence, corruption handling
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
- **Style & Consistency**: Keep code formatting consistent. Avoid unnecessary abstractions or dead code.
- **Comments**: Keep comments minimal and focused strictly on explaining non-obvious requirements.

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

- `ansimuz.spec` builds ANSImuz from a **pinned commit** (`%global commit`). The version uses a caret snapshot, e.g. `0.1.0^20261031git0abb051`. It installs the binary to `/usr/bin/ansimuz` and `backend/` and `assets/` to `/usr/share/ansimuz/`, and builds with `ANSIMUZ_DATA_DIR=/usr/share/ansimuz`.
- `python-spotifyscraper.spec` packages `spotifyscraper`, which is not in the Fedora repositories. `ansimuz` requires it as `python3dist(spotifyscraper)`.

Update the specs when you:

- add, remove, or rename a Python or system dependency (keep `Requires` in sync with `requirements.txt`)
- change how runtime data is located, or add new top-level runtime directories that must be installed
- change the license or add files that should be installed alongside the binary

To move `ansimuz.spec` to a newer upstream commit:

```bash
cd packaging/rpm
NEW=$(git ls-remote https://github.com/greenbugx/ANSImuz HEAD | cut -f1)
sed -i "s/^%global commit .*/%global commit $NEW/; s/^%global commitdate .*/%global commitdate $(date +%Y%m%d)/" ansimuz.spec
```

Then add a `%changelog` entry with the new version, and reset `Release` to `1`.

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

# Check Python syntax and run backend tests
python -m py_compile backend/bridge.py backend/exceptions.py backend/artwork/thumbnails.py backend/providers/spotify/spotify.py backend/providers/ytmusic/ytmusic.py
python -m unittest discover -s backend/tests -p "test_*.py" -v
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
