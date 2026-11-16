from __future__ import annotations

import math
import re
import unicodedata
import xml.etree.ElementTree as ET
from abc import ABC, abstractmethod
from difflib import SequenceMatcher
from typing import Any

# Default API User-Agent identifying the app and contact according to API TOS
DEFAULT_USER_AGENT = (
    "ANSImuz/0.1.3 (+https://github.com/greenbugx/ANSImuz; contact: greenbugx@proton.me)"
)
DEFAULT_ATTRIBUTION = "Lyrics provided by lrc.red"
DEFAULT_TIMEOUT = 15.0


def parse_timestamp(ts: str | float | int | None) -> int | None:
    """
    Parses a timestamp in seconds (float/int), mm:ss[.xx], or hh:mm:ss[.xx]
    into milliseconds (int). Returns None if input is invalid or missing.
    """
    if ts is None:
        return None
    if isinstance(ts, (int, float)):
        return max(0, int(round(float(ts) * 1000.0)))

    ts_str = str(ts).strip()
    if not ts_str:
        return None

    # Strip any enclosing brackets or angle tags if passed accidentally
    ts_str = ts_str.lstrip("[<").rstrip("]>")

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


def normalize_text(text: str) -> str:
    """Normalizes text for fuzzy matching by removing accents, punctuation, and markers."""
    text = unicodedata.normalize("NFKD", text or "")
    text = "".join(char for char in text if not unicodedata.combining(char))
    text = text.lower()
    text = re.sub(
        r"\b(official(?:\s+music)?\s+video|official\s+audio|lyrics?\s+video|lyrics|visualizer|topic|hd|uhd|4k)\b",
        " ",
        text,
    )
    return re.sub(r"[^a-z0-9]+", " ", text).strip()


def compute_similarity(expected: str, actual: str) -> float:
    """Calculates weighted similarity ratio between two strings using token coverage and SequenceMatcher."""
    norm_expected = normalize_text(expected)
    norm_actual = normalize_text(actual)
    if not norm_expected or not norm_actual:
        return 0.0

    expected_tokens = set(norm_expected.split())
    actual_tokens = set(norm_actual.split())
    if not expected_tokens:
        return 0.0

    coverage = len(expected_tokens & actual_tokens) / len(expected_tokens)
    sequence = SequenceMatcher(None, norm_expected, norm_actual).ratio()
    return 0.65 * coverage + 0.35 * sequence


def extract_title(track: dict[str, Any]) -> str:
    """Extracts track title from various dict formats."""
    return str(track.get("title") or track.get("name") or "").strip()


def extract_artists(track: dict[str, Any]) -> list[str]:
    """Extracts list of artist names from track dict."""
    artists = track.get("artists")
    if isinstance(artists, list):
        names = []
        for item in artists:
            if isinstance(item, str) and item.strip():
                names.append(item.strip())
            elif isinstance(item, dict) and item.get("name"):
                names.append(str(item["name"]).strip())
        if names:
            return names

    artist_str = track.get("artist") or track.get("author")
    if isinstance(artist_str, str) and artist_str.strip():
        # Split on comma if present, or return as single item
        return [a.strip() for a in artist_str.split(",") if a.strip()]

    return []


def extract_album(track: dict[str, Any]) -> str:
    """Extracts album name from track dict."""
    album = track.get("album")
    if isinstance(album, str):
        return album.strip()
    if isinstance(album, dict):
        return str(album.get("name") or "").strip()
    return ""


def extract_duration_seconds(track: dict[str, Any]) -> float | None:
    """Extracts duration in seconds as a float."""
    dur_sec = track.get("duration_seconds")
    if isinstance(dur_sec, (int, float)) and dur_sec > 0:
        return float(dur_sec)

    dur_ms = track.get("duration_ms")
    if isinstance(dur_ms, (int, float)) and dur_ms > 0:
        return float(dur_ms) / 1000.0

    dur = track.get("duration")
    if isinstance(dur, (int, float)) and dur > 0:
        return float(dur)
    if isinstance(dur, str) and dur.strip():
        try:
            parts = [float(p) for p in dur.split(":")]
            total = 0.0
            for p in parts:
                total = total * 60.0 + p
            return total if total > 0 else None
        except (ValueError, TypeError):
            return None

    return None


def extract_isrc(track: dict[str, Any]) -> str | None:
    """Extracts ISRC from track dict if available."""
    isrc = track.get("isrc")
    if isinstance(isrc, str) and isrc.strip():
        return isrc.strip()

    ext_ids = track.get("external_ids")
    if isinstance(ext_ids, dict):
        ext_isrc = ext_ids.get("isrc")
        if isinstance(ext_isrc, str) and ext_isrc.strip():
            return ext_isrc.strip()

    return None


def parse_ttml(ttml_text: str) -> tuple[str, list[dict[str, Any]]]:
    """
    Parses a TTML XML document containing timed lyrics lines and spans.
    Returns (sync_type, lines) where sync_type is 'word', 'line', or 'none'.
    """
    if not ttml_text or not ttml_text.strip():
        return "none", []

    try:
        root = ET.fromstring(ttml_text.strip())
    except ET.ParseError:
        return "none", []

    lines: list[dict[str, Any]] = []
    has_word_sync = False
    has_line_sync = False

    for elem in root.iter():
        tag = elem.tag.split("}")[-1] if "}" in elem.tag else elem.tag
        if tag != "p":
            continue

        p_begin = parse_timestamp(elem.attrib.get("begin"))
        p_end = parse_timestamp(elem.attrib.get("end"))
        if p_begin is not None:
            has_line_sync = True

        words: list[dict[str, Any]] = []
        # Find all direct or nested <span> children
        for child in elem:
            c_tag = child.tag.split("}")[-1] if "}" in child.tag else child.tag
            if c_tag == "span":
                s_begin = parse_timestamp(child.attrib.get("begin"))
                s_end = parse_timestamp(child.attrib.get("end"))
                s_text = child.text or ""
                if s_begin is not None or s_text:
                    words.append({
                        "start_ms": s_begin if s_begin is not None else p_begin,
                        "end_ms": s_end if s_end is not None else p_end,
                        "text": s_text,
                    })
                    if s_begin is not None:
                        has_word_sync = True

        full_text = "".join(elem.itertext()).strip()
        lines.append({
            "start_ms": p_begin,
            "end_ms": p_end,
            "text": full_text,
            "words": words,
        })

    lines.sort(key=lambda item: item["start_ms"] if item["start_ms"] is not None else 0)

    # Fill in end_ms from next line's start_ms if missing
    for i in range(len(lines) - 1):
        if lines[i]["end_ms"] is None and lines[i + 1]["start_ms"] is not None:
            lines[i]["end_ms"] = lines[i + 1]["start_ms"]

    sync_type = "word" if has_word_sync else ("line" if has_line_sync else "none")
    return sync_type, lines


def parse_lrc(lrc_text: str) -> tuple[str, list[dict[str, Any]]]:
    """
    Parses LRC text (supports standard LRC, enhanced LRC with <mm:ss.xx> word tags,
    multi-timestamp lines, and unsynchronized plain text).
    Returns (sync_type, lines) where sync_type is 'word', 'line', or 'none'.
    """
    if not lrc_text or not lrc_text.strip():
        return "none", []

    offset_ms = 0
    raw_lines = lrc_text.splitlines()
    timed_lines: list[dict[str, Any]] = []
    plain_lines: list[str] = []
    has_word_sync = False

    time_tag_pattern = re.compile(r"\[(\d{1,2}:\d{2}(?:\.\d{1,3})?)\]")
    word_tag_pattern = re.compile(r"<(\d{1,2}:\d{2}(?:\.\d{1,3})?)>([^<]*)")

    for raw_line in raw_lines:
        line = raw_line.strip()
        if not line:
            continue

        # Check for offset header e.g. [offset:+500] or [offset:-200]
        offset_match = re.match(r"^\[offset:\s*([+-]?\d+)\s*\]", line, re.IGNORECASE)
        if offset_match:
            try:
                offset_ms = int(offset_match.group(1))
            except ValueError:
                pass
            continue

        # Ignore other metadata ID tags like [ti:Title], [ar:Artist], etc.
        if re.match(r"^\[[a-zA-Z]+:[^\]]*\]", line):
            continue

        tags = time_tag_pattern.findall(line)
        if not tags:
            # Plain line without leading timestamp
            plain_lines.append(line)
            continue

        # Extract remainder after stripping leading timestamp tags
        remainder = time_tag_pattern.sub("", line).strip()

        # Check for word-level sync tags: <00:01.23>word
        word_matches = list(word_tag_pattern.finditer(remainder))
        words: list[dict[str, Any]] = []
        if word_matches:
            for i, match in enumerate(word_matches):
                w_start = parse_timestamp(match.group(1))
                if w_start is not None:
                    w_start = max(0, w_start + offset_ms)
                w_text = match.group(2)
                w_end = None
                if i + 1 < len(word_matches):
                    w_next_start = parse_timestamp(word_matches[i + 1].group(1))
                    if w_next_start is not None:
                        w_end = max(0, w_next_start + offset_ms)

                word_clean = w_text.strip()
                if word_clean:
                    words.append({
                        "start_ms": w_start,
                        "end_ms": w_end,
                        "text": word_clean,
                    })
                    if w_start is not None:
                        has_word_sync = True

            clean_text = re.sub(r"<[^>]+>", "", remainder).strip()
        else:
            clean_text = remainder

        for tag in tags:
            tag_ms = parse_timestamp(tag)
            if tag_ms is not None:
                tag_ms = max(0, tag_ms + offset_ms)
            timed_lines.append({
                "start_ms": tag_ms,
                "end_ms": None,
                "text": clean_text,
                "words": list(words),
            })

    if timed_lines:
        timed_lines.sort(key=lambda item: item["start_ms"] if item["start_ms"] is not None else 0)
        # Calculate line end_ms based on next line's start_ms
        for i in range(len(timed_lines) - 1):
            if timed_lines[i]["end_ms"] is None and timed_lines[i + 1]["start_ms"] is not None:
                timed_lines[i]["end_ms"] = timed_lines[i + 1]["start_ms"]
        sync_type = "word" if has_word_sync else "line"
        return sync_type, timed_lines

    # If no timestamps were found at all, treat as unsynchronized plain lyrics
    parsed_plain = [
        {"start_ms": None, "end_ms": None, "text": pl, "words": []}
        for pl in plain_lines
        if pl
    ]
    return "none", parsed_plain


class LyricsProvider(ABC):
    """Abstract base class for all lyrics providers."""

    @property
    @abstractmethod
    def name(self) -> str:
        """Unique provider identifier."""
        ...

    @abstractmethod
    def get_lyrics(self, track: dict[str, Any]) -> dict[str, Any] | None:
        """
        Accepts a track dictionary and returns its lyrics in a normalized format,
        or None if no lyrics could be found.
        """
        ...