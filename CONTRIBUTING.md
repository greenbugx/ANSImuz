# Contributing to ANSImuz

Thank you for your interest in contributing to ANSImuz! We welcome bug reports, feature suggestions, documentation improvements, and code contributions.

Please review this guide before submitting a pull request or opening an issue.

---

## Code of Conduct

All contributors and participants are expected to adhere to our [Code of Conduct](CODE_OF_CONDUCT.md). Please treat others with respect and keep our community welcoming and collaborative.

---

## Development Prerequisites

Ensure the following tools are installed on your system:

- **Rust toolchain**: 1.85+ (compatible with the Rust 2024 edition).
- **Python**: 3.10 or higher (with development headers for PyO3 linking).
- **[mpv](https://mpv.io/)**: For audio playback.
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)**: For stream resolution and media fetching.
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
   cargo fmt --check
   cargo check --all-targets
   cargo clippy --all-targets -- -D warnings
   cargo test

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
│   ├── app/          # State management, keyboard event routing, and main loop
│   ├── artwork/      # Image decoding, thumbnail cache, palette extraction, and renderer
│   ├── config/       # Settings persistence and platform-aware cache paths
│   ├── music/        # Track models, search interfaces, and lazy resolution
│   ├── playback/     # MPV IPC process supervision, volume, and playback controls
│   ├── terminal/     # Terminal raw mode setup and clean teardown
│   ├── ui/           # Ratatui components, screens (player, settings, help, command palette), theme
│   ├── lib.rs        # Library interface for integration tests
│   └── main.rs       # Minimal application entry point
├── backend/
│   ├── bridge.py     # PyO3 integration boundary
│   ├── artwork/      # Python thumbnail utilities and prewarming
│   └── providers/    # Music provider implementations (ytmusic, spotify)
├── tests/            # Integration test suites
└── .github/          # CI workflows
```

---

## Code Guidelines & Best Practices

- **Non-blocking TUI**: Never block the main TUI rendering thread. Long-running tasks (such as network requests or stream resolutions) must execute asynchronously in background threads and communicate via channels.
- **Unicode Safety**: Never slice strings by raw byte offsets when displaying text in the TUI. Use character-based bounds or the project's `truncate` helper (`crate::ui::theme::truncate`) to prevent panics on multibyte characters.
- **Style & Consistency**: Keep code formatting consistent. Avoid unnecessary abstractions or dead code.
- **Comments**: Keep comments minimal and focused strictly on explaining non-obvious requirements.

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

# Run the test suite
cargo test

# Check Python syntax
python -m py_compile backend/bridge.py backend/artwork/thumbnails.py backend/providers/spotify/spotify.py backend/providers/ytmusic/ytmusic.py

---

## Continuous Integration (CI)

Our GitHub Actions pipeline automatically checks every push and pull request targeting `main`:

- **Formatting**: `cargo fmt --check`
- **Type & Target Checks**: `cargo check --all-targets`
- **Linter**: `cargo clippy --all-targets -- -D warnings`
- **Build & Tests**: `cargo build --verbose` and `cargo test --verbose`
- **Python Syntax**: Bytecode compilation (`py_compile`) across all backend modules

CI automatically skips runs when only documentation (`*.md`), `LICENSE`, or `.gitignore` files are changed.

## Submitting Pull Requests

1. Create a descriptive feature branch:
   ```bash
   git checkout -b feature/my-new-feature
   ```
2. Make concise, focused commits with meaningful commit messages.
3. Push to your fork and submit a Pull Request targeting the `main` branch.
4. Ensure all CI checks pass. Provide a clear description of the problem, the solution, and how the changes were verified.
