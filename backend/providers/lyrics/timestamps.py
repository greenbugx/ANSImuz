from __future__ import annotations

DEFAULT_FRAME_RATE = 30.0


def parse_timestamp(
    ts: str | float | int | None,
    frame_rate: float | None = DEFAULT_FRAME_RATE,
) -> int | None:
    """
    Parses a timestamp in seconds (float/int), mm:ss[.xx], hh:mm:ss[.xx],
    hh:mm:ss:frames, or unit strings (e.g. '19.764s', '19764ms') into integer
    milliseconds.

    For four-component frame formats (hh:mm:ss:frames), frame_rate information
    is required. If frame_rate is None or <= 0, frame formats are rejected.

    Returns None if input is invalid or missing.
    """
    if ts is None:
        return None
    if isinstance(ts, (int, float)):
        return max(0, int(round(float(ts) * 1000.0)))

    ts_str = str(ts).strip()
    if not ts_str:
        return None

    # Strip any enclosing brackets or angle tags if passed accidentally
    ts_str = ts_str.lstrip("[<").rstrip("]>").strip()
    if not ts_str:
        return None

    # Handle explicit millisecond units
    if ts_str.endswith("ms"):
        try:
            return max(0, int(round(float(ts_str[:-2]))))
        except ValueError:
            return None

    # Handle explicit seconds units
    if ts_str.endswith("s"):
        ts_str = ts_str[:-1].strip()

    parts = ts_str.split(":")
    try:
        if len(parts) == 1:
            val = float(parts[0])
            return max(0, int(round(val * 1000.0)))
        elif len(parts) == 2:
            mins = float(parts[0])
            secs = float(parts[1])
            return max(0, int(round((mins * 60.0 + secs) * 1000.0)))
        elif len(parts) == 3:
            hrs = float(parts[0])
            mins = float(parts[1])
            secs = float(parts[2])
            return max(0, int(round((hrs * 3600.0 + mins * 60.0 + secs) * 1000.0)))
        elif len(parts) == 4:
            # hh:mm:ss:frames requires valid frame-rate information
            if frame_rate is None or frame_rate <= 0:
                return None
            hrs = float(parts[0])
            mins = float(parts[1])
            secs = float(parts[2])
            frames = float(parts[3])
            return max(
                0,
                int(round((hrs * 3600.0 + mins * 60.0 + secs + frames / float(frame_rate)) * 1000.0)),
            )
        return None
    except (ValueError, TypeError):
        return None


def format_timestamp(ms: int | None) -> str:
    """Formats milliseconds into mm:ss.xx format."""
    if ms is None:
        return "--:--"
    ms = max(0, ms)
    total_secs = ms / 1000.0
    mins = int(total_secs // 60)
    secs = total_secs % 60
    return f"{mins:02d}:{secs:05.2f}"

