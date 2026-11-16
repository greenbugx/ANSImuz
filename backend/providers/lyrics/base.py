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
    Parses a timestamp in seconds (float/int), mm:ss[.xx], hh:mm:ss[.xx],
    hh:mm:ss:frames, or unit strings (e.g. '19.764s', '19764ms') into integer
    milliseconds. Returns None if input is invalid or missing.
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
            # hh:mm:ss:frames
            hrs = float(parts[0])
            mins = float(parts[1])
            secs = float(parts[2])
            frames = float(parts[3])
            return max(
                0,
                int(round((hrs * 3600.0 + mins * 60.0 + secs + frames / 30.0) * 1000.0)),
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


def get_xml_attr(elem: ET.Element, name: str) -> str | None:
    """Retrieves an XML element attribute matching the local name, ignoring namespaces."""
    if name in elem.attrib:
        return elem.attrib[name]
    for k, v in elem.attrib.items():
        if k.split("}")[-1] == name:
            return v
    return None


def parse_ttml(ttml_text: str) -> tuple[str, list[dict[str, Any]]]:
    """
    Parses a TTML XML document containing timed lyrics lines and spans.
    Handles XML namespaces, timing formats, paragraph lines, word timings,
    and syllable timings where present.
    Returns (sync_type, lines) where sync_type is 'syllable', 'word', 'line', or 'none'.
    """
    if not ttml_text or not ttml_text.strip():
        return "none", []

    try:
        root = ET.fromstring(ttml_text.strip())
    except ET.ParseError:
        return "none", []

    lines: list[dict[str, Any]] = []
    has_line_sync = False
    has_word_sync = False
    has_syllable_sync = False

    for elem in root.iter():
        tag = elem.tag.split("}")[-1] if "}" in elem.tag else elem.tag
        if tag != "p":
            continue

        p_begin = parse_timestamp(get_xml_attr(elem, "begin"))
        p_dur = parse_timestamp(get_xml_attr(elem, "dur"))
        p_end = parse_timestamp(get_xml_attr(elem, "end"))
        if p_end is None and p_begin is not None and p_dur is not None:
            p_end = p_begin + p_dur
        if p_begin is not None:
            has_line_sync = True

        raw_line_text = "".join(elem.itertext()).strip()
        full_text = re.sub(r"\s+", " ", raw_line_text)

        direct_spans = [
            child for child in elem if child.tag.split("}")[-1] == "span"
        ]
        all_spans = [
            child
            for child in elem.iter()
            if child != elem and child.tag.split("}")[-1] == "span"
        ]

        words: list[dict[str, Any]] = []

        if not all_spans:
            # Paragraph without timed inner spans: extract words with missing timestamps
            for tok in full_text.split():
                words.append({
                    "text": tok,
                    "start_ms": None,
                    "end_ms": None,
                    "syllables": [],
                })
        else:
            # Check for nested span hierarchy: <span word><span syl/>...</span>
            has_nested = any(
                any(c.tag.split("}")[-1] == "span" for c in s) for s in direct_spans
            )
            if has_nested:
                for s in direct_spans:
                    w_text = "".join(s.itertext()).strip()
                    if not w_text:
                        continue
                    w_begin = parse_timestamp(get_xml_attr(s, "begin")) or p_begin
                    w_end = parse_timestamp(get_xml_attr(s, "end")) or p_end
                    if w_begin is not None:
                        has_word_sync = True

                    nested = [c for c in s if c.tag.split("}")[-1] == "span"]
                    syls: list[dict[str, Any]] = []
                    if nested:
                        for ns in nested:
                            ns_text = "".join(ns.itertext()).strip()
                            if ns_text:
                                ns_begin = (
                                    parse_timestamp(get_xml_attr(ns, "begin")) or w_begin
                                )
                                ns_end = (
                                    parse_timestamp(get_xml_attr(ns, "end")) or w_end
                                )
                                syls.append({
                                    "text": ns_text,
                                    "start_ms": ns_begin,
                                    "end_ms": ns_end,
                                })
                                has_syllable_sync = True
                        if syls:
                            w_text = "".join(ns["text"] for ns in syls)

                    words.append({
                        "text": w_text,
                        "start_ms": w_begin,
                        "end_ms": w_end,
                        "syllables": syls,
                    })
            else:
                # Sibling spans model (consecutive spans grouped by whitespace tail/text)
                current_syllables: list[dict[str, Any]] = []
                for s in direct_spans:
                    s_text = s.text or ""
                    s_tail = s.tail or ""
                    s_begin = parse_timestamp(get_xml_attr(s, "begin"))
                    s_end = parse_timestamp(get_xml_attr(s, "end"))
                    if s_begin is not None:
                        has_word_sync = True

                    current_syllables.append({
                        "text": s_text,
                        "start_ms": s_begin if s_begin is not None else p_begin,
                        "end_ms": s_end if s_end is not None else p_end,
                        "tail": s_tail,
                    })

                    has_space = bool(
                        s_tail and any(c.isspace() for c in s_tail)
                    ) or s_text.endswith(" ")
                    if has_space or s == direct_spans[-1]:
                        combined_text = "".join(
                            syl["text"] for syl in current_syllables
                        ).strip()
                        if combined_text:
                            w_start = current_syllables[0]["start_ms"]
                            w_end = current_syllables[-1]["end_ms"]
                            syls = []
                            if len(current_syllables) > 1:
                                has_syllable_sync = True
                                syls = [
                                    {
                                        "text": syl["text"].strip(),
                                        "start_ms": syl["start_ms"],
                                        "end_ms": syl["end_ms"],
                                    }
                                    for syl in current_syllables
                                    if syl["text"].strip()
                                ]
                            words.append({
                                "text": combined_text,
                                "start_ms": w_start,
                                "end_ms": w_end,
                                "syllables": syls,
                            })
                        current_syllables = []

        if p_begin is None and words and words[0]["start_ms"] is not None:
            p_begin = words[0]["start_ms"]
        if p_end is None and words and words[-1]["end_ms"] is not None:
            p_end = words[-1]["end_ms"]

        if words:
            full_text = " ".join(w["text"] for w in words if w.get("text"))
        else:
            raw_line_text = "".join(elem.itertext()).strip()
            full_text = re.sub(r"\s+", " ", raw_line_text)

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

    if has_syllable_sync:
        sync_type = "syllable"
    elif has_word_sync:
        sync_type = "word"
    elif has_line_sync:
        sync_type = "line"
    else:
        sync_type = "none"

    return sync_type, lines


def parse_lrc(lrc_text: str) -> tuple[str, list[dict[str, Any]]]:
    """
    Parses LRC text (supports standard LRC, enhanced LRC with <mm:ss.xx> word tags,
    syllable groupings, multi-timestamp lines, and unsynchronized plain text).
    Returns (sync_type, lines) where sync_type is 'syllable', 'word', 'line', or 'none'.
    """
    if not lrc_text or not lrc_text.strip():
        return "none", []

    offset_ms = 0
    raw_lines = lrc_text.splitlines()
    timed_lines: list[dict[str, Any]] = []
    plain_lines: list[str] = []
    has_word_sync = False
    has_syllable_sync = False

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
            plain_lines.append(line)
            continue

        remainder = time_tag_pattern.sub("", line).strip()
        word_matches = list(word_tag_pattern.finditer(remainder))

        words: list[dict[str, Any]] = []
        clean_text = ""
        line_end_ms: int | None = None

        if word_matches:
            has_word_sync = True
            tokens: list[dict[str, Any]] = []
            for i, match in enumerate(word_matches):
                ts = parse_timestamp(match.group(1))
                if ts is not None:
                    ts = max(0, ts + offset_ms)
                txt = match.group(2)
                next_ts = None
                if i + 1 < len(word_matches):
                    next_ts = parse_timestamp(word_matches[i + 1].group(1))
                    if next_ts is not None:
                        next_ts = max(0, next_ts + offset_ms)
                tokens.append({"start_ms": ts, "end_ms": next_ts, "raw": txt})

            curr_syls: list[dict[str, Any]] = []
            for idx, tok in enumerate(tokens):
                raw = tok["raw"]
                # Closing timestamp with empty text at end of line provides line_end_ms
                if not raw.strip() and idx == len(tokens) - 1:
                    line_end_ms = tok["start_ms"]
                    continue

                curr_syls.append(tok)
                is_last_content = (idx == len(tokens) - 1) or (
                    idx == len(tokens) - 2 and not tokens[-1]["raw"].strip()
                )

                if raw.endswith(" ") or raw.endswith("\t") or is_last_content:
                    combined = "".join(s["raw"] for s in curr_syls).strip()
                    if combined:
                        w_start = curr_syls[0]["start_ms"]
                        w_end = curr_syls[-1]["end_ms"]
                        syls = []
                        if len(curr_syls) > 1:
                            has_syllable_sync = True
                            syls = [
                                {
                                    "text": s["raw"].strip(),
                                    "start_ms": s["start_ms"],
                                    "end_ms": s["end_ms"],
                                }
                                for s in curr_syls
                                if s["raw"].strip()
                            ]
                        words.append({
                            "text": combined,
                            "start_ms": w_start,
                            "end_ms": w_end,
                            "syllables": syls,
                        })
                    curr_syls = []

            clean_text = re.sub(r"<[^>]+>", "", remainder).strip()
            clean_text = re.sub(r"\s+", " ", clean_text)
            if line_end_ms is None and words:
                line_end_ms = words[-1]["end_ms"]
        else:
            clean_text = remainder
            for tok in clean_text.split():
                words.append({
                    "text": tok,
                    "start_ms": None,
                    "end_ms": None,
                    "syllables": [],
                })

        for tag in tags:
            tag_ms = parse_timestamp(tag)
            if tag_ms is not None:
                tag_ms = max(0, tag_ms + offset_ms)
            timed_lines.append({
                "text": clean_text,
                "start_ms": tag_ms,
                "end_ms": line_end_ms,
                "words": list(words),
            })

    if timed_lines:
        timed_lines.sort(
            key=lambda item: item["start_ms"] if item["start_ms"] is not None else 0
        )
        for i in range(len(timed_lines) - 1):
            if timed_lines[i]["end_ms"] is None and timed_lines[i + 1]["start_ms"] is not None:
                timed_lines[i]["end_ms"] = timed_lines[i + 1]["start_ms"]

        if has_syllable_sync:
            sync_type = "syllable"
        elif has_word_sync:
            sync_type = "word"
        else:
            sync_type = "line"
        return sync_type, timed_lines

    # Plain unsynchronized lyrics
    parsed_plain: list[dict[str, Any]] = []
    for pl in plain_lines:
        w_list = [
            {"text": tok, "start_ms": None, "end_ms": None, "syllables": []}
            for tok in pl.split()
        ]
        parsed_plain.append({
            "text": pl,
            "start_ms": None,
            "end_ms": None,
            "words": w_list,
        })
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