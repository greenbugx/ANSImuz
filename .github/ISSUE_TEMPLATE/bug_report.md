---
name: Bug report
about: Report a bug, crash, or unexpected behavior in ANSImuz
title: '[BUG] '
labels: 'bug'
assignees: ''

---

## Describe the Bug
A clear and concise description of what the bug is.

## Steps to Reproduce
Steps to reproduce the behavior:
1. Start ANSImuz (`cargo run`)
2. Press `/` and search for `'...'`
3. Press `...` (e.g. `Enter` to play, `A` to queue, `Tab` to switch focus)
4. Observe the issue

## Expected Behavior
A clear and concise description of what you expected to happen.

## Actual Behavior / Status Message
What actually occurred (e.g. status line text such as `PLAYER ERROR: ...` or `RESOLVE ERROR: ...`).

## Error Logs / Backtrace
If the application crashed or panicked, run with `RUST_BACKTRACE=1` and paste the backtrace here:

```text
<paste backtrace or terminal error here>
```

## Screenshots / Terminal Recordings
If this is a visual or layout glitch, please include a screenshot of your terminal window (including terminal size if relevant).

## Environment Information
Please provide your environment details:
- **OS / Distribution**: [e.g. Arch Linux, Ubuntu 24.04, macOS Sequoia 15.0]
- **Terminal Emulator**: [e.g. Kitty, Alacritty, WezTerm, Ghostty, Foot, iTerm2]
- **Terminal Dimensions (Cols x Rows)**: [e.g. 140x35, 80x24]
- **Active Music Provider**: [e.g. YouTube Music / Spotify]
- **`mpv` Version** (`mpv --version`):
- **`yt-dlp` Version** (`yt-dlp --version`):
- **Rust Version** (`rustc --version`):
- **Python Version** (`python3 --version`):

## Additional Context
Add any other context about the problem here (e.g. specific songs/queries, custom settings, network proxies).
