from __future__ import annotations

import html
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

    agent_map: dict[str, int] = {}
    agent_order: list[str] = []

    # Pre-populate agent map from <ttm:agent> metadata declarations if present
    for elem in root.iter():
        tag = elem.tag.split("}")[-1] if "}" in elem.tag else elem.tag
        if tag == "agent":
            agent_id = get_xml_attr(elem, "id")
            if agent_id and agent_id not in agent_map:
                agent_map[agent_id] = len(agent_order)
                agent_order.append(agent_id)

    for elem in root.iter():
        tag = elem.tag.split("}")[-1] if "}" in elem.tag else elem.tag
        if tag != "p":
            continue

        agent_ref = get_xml_attr(elem, "agent")
        if agent_ref and agent_ref not in agent_map:
            agent_map[agent_ref] = len(agent_order)
            agent_order.append(agent_ref)

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
            "_agent": agent_ref,
        })

    # Assign integer singer index (0 = primary/left, 1 = secondary/right)
    # If 0 or 1 agent across all lines, all lines are singer 0
    multiple_singers = len(agent_order) > 1
    for line in lines:
        a_ref = line.pop("_agent", None)
        if multiple_singers and a_ref in agent_map:
            line["singer"] = agent_map[a_ref]
        else:
            line["singer"] = 0

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
                "singer": 0,
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
            "singer": 0,
        })
    return "none", parsed_plain


def clean_title_for_search(title: str) -> str:
    """
    Cleans metadata, audio/video tags, and version suffixes from a title string
    to improve lyrics search hit rates.
    """
    if not title:
        return ""
    # Strip parenthesized or bracketed video/metadata tags
    cleaned = re.sub(
        r"\s*[\(\[](?:(?:official\s*)?(?:music\s*video|lyric\s*video|video|audio|visualizer)|lyrics?|hd|uhd|4k|remastered|explicit)[\)\]]",
        "",
        title,
        flags=re.IGNORECASE,
    )
    # Strip trailing dash markers like " - Official Video"
    cleaned = re.sub(
        r"\s*-\s*(?:(?:official\s*)?(?:music\s*video|lyric\s*video|video|audio|visualizer)|lyrics?|hd|uhd|4k|remastered)\s*$",
        "",
        cleaned,
        flags=re.IGNORECASE,
    )
    return cleaned.strip()


def parse_qrc(qrc_text: str) -> tuple[str, list[dict[str, Any]]]:
    """
    Parses QQ Music QRC karaoke lyrics text (or XML with LyricContent attribute).
    Returns (sync_type, lines) where sync_type is 'syllable', 'word', 'line', or 'none'.
    """
    if not qrc_text or not qrc_text.strip():
        return "none", []

    # If wrapped in XML e.g. <Lyric_1 ... LyricContent="..."/>, extract content
    content_match = re.search(r'LyricContent="([^"]*)"', qrc_text)
    if not content_match:
        # Check multiline attribute match
        content_match = re.search(r'LyricContent="([\s\S]*?)"(?:\s*/>|\s*>)', qrc_text)
    raw_lyrics = content_match.group(1) if content_match else qrc_text
    raw_lyrics = html.unescape(raw_lyrics)

    line_pattern = re.compile(r"^\s*\[(\d+),(\d+)\](.*)$")
    word_pattern = re.compile(r"([^()]+)\((\d+),(\d+)\)")

    lines: list[dict[str, Any]] = []
    has_word_sync = False
    has_syllable_sync = False

    for raw_line in raw_lyrics.splitlines():
        line_match = line_pattern.match(raw_line)
        if not line_match:
            continue
        start_ms = int(line_match.group(1))
        dur_ms = int(line_match.group(2))
        line_end_ms = start_ms + dur_ms if dur_ms > 0 else None
        body = line_match.group(3)

        matches = word_pattern.findall(body)
        tokens: list[dict[str, Any]] = []
        for txt, s_str, d_str in matches:
            w_s = int(s_str)
            w_d = int(d_str)
            tokens.append({"raw": txt, "start_ms": w_s, "end_ms": w_s + w_d})

        words: list[dict[str, Any]] = []
        curr_syls: list[dict[str, Any]] = []
        for idx, tok in enumerate(tokens):
            raw = tok["raw"]
            if not raw.strip():
                if curr_syls:
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
                continue

            curr_syls.append(tok)
            if raw.endswith(" ") or raw.endswith("\t") or idx == len(tokens) - 1:
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

        if words:
            has_word_sync = True
            full_text = " ".join(w["text"] for w in words if w.get("text"))
        else:
            clean = re.sub(r"\(\d+,\d+\)", "", body).strip()
            full_text = re.sub(r"\s+", " ", clean)

        # Check background vocals
        b_trim = body.strip()
        t_trim = full_text.strip()
        is_bg = (
            (t_trim.startswith("(") and t_trim.endswith(")"))
            or (t_trim.startswith("（") and t_trim.endswith("）"))
            or (b_trim.startswith("(") and b_trim.endswith(")"))
            or (b_trim.startswith("（") and b_trim.endswith("）"))
        )
        if is_bg and not (t_trim.startswith("(") and t_trim.endswith(")")) and not (t_trim.startswith("（") and t_trim.endswith("）")):
            full_text = f"({full_text})"

        lines.append({
            "start_ms": start_ms,
            "end_ms": line_end_ms,
            "text": full_text,
            "words": words,
            "is_background": is_bg,
            "singer": 0,
        })

    if not lines:
        return "none", []

    lines.sort(key=lambda item: item["start_ms"] if item["start_ms"] is not None else 0)
    for i in range(len(lines) - 1):
        if lines[i]["end_ms"] is None and lines[i + 1]["start_ms"] is not None:
            lines[i]["end_ms"] = lines[i + 1]["start_ms"]

    if has_syllable_sync:
        sync_type = "syllable"
    elif has_word_sync:
        sync_type = "word"
    else:
        sync_type = "line"

    return sync_type, lines


def parse_lyrics_plus(response: dict[str, Any]) -> tuple[str, list[dict[str, Any]]]:
    """
    Parses a LyricsPlus v2 response dictionary into normalized lines and sync type.
    Handles syllable-level timings, word groupings, background vocals, and duet agents.
    """
    if not isinstance(response, dict):
        return "none", []

    lyrics_list = response.get("lyrics")
    if not isinstance(lyrics_list, list) or not lyrics_list:
        return "none", []

    metadata = response.get("metadata") or {}
    agents = metadata.get("agents") if isinstance(metadata, dict) else {}
    bg_singers: set[str] = set()
    if isinstance(agents, dict):
        for agent_id, agent_info in agents.items():
            if isinstance(agent_info, dict):
                a_type = str(agent_info.get("type") or "").lower()
                if a_type in ("background", "bg"):
                    bg_singers.add(str(agent_id))
                    alias = agent_info.get("alias")
                    if alias:
                        bg_singers.add(str(alias))

    lines: list[dict[str, Any]] = []
    has_syllable_sync = False
    has_word_sync = False

    for raw_line in lyrics_list:
        if not isinstance(raw_line, dict):
            continue
        time_ms = raw_line.get("time")
        dur_ms = raw_line.get("duration")
        if time_ms is None:
            continue

        syllables = raw_line.get("syllabus")
        words: list[dict[str, Any]] = []
        curr_syls: list[dict[str, Any]] = []

        if isinstance(syllables, list) and syllables:
            for idx, syl in enumerate(syllables):
                if not isinstance(syl, dict):
                    continue
                raw_text = syl.get("text") or ""
                s_time = syl.get("time")
                s_dur = syl.get("duration") or 0
                if s_time is None:
                    continue
                s_end = s_time + s_dur
                curr_syls.append({
                    "text": raw_text,
                    "start_ms": s_time,
                    "end_ms": s_end,
                })
                is_last = (idx == len(syllables) - 1)
                if raw_text.endswith(" ") or raw_text.endswith("\t") or is_last:
                    combined = "".join(s["text"] for s in curr_syls).strip()
                    if combined:
                        w_start = curr_syls[0]["start_ms"]
                        w_end = curr_syls[-1]["end_ms"]
                        syl_list: list[dict[str, Any]] = []
                        if len(curr_syls) > 1:
                            has_syllable_sync = True
                            syl_list = [
                                {
                                    "text": s["text"].strip(),
                                    "start_ms": s["start_ms"],
                                    "end_ms": s["end_ms"],
                                }
                                for s in curr_syls
                                if s["text"].strip()
                            ]
                        words.append({
                            "text": combined,
                            "start_ms": w_start,
                            "end_ms": w_end,
                            "syllables": syl_list,
                        })
                    curr_syls = []

        if words:
            has_word_sync = True
            full_text = " ".join(w["text"] for w in words if w.get("text"))
            line_start = min(time_ms, words[0]["start_ms"]) if words[0]["start_ms"] is not None else time_ms
            line_end = words[-1]["end_ms"]
        else:
            raw_line_text = str(raw_line.get("text") or "").strip()
            full_text = raw_line_text
            line_start = time_ms
            line_end = time_ms + dur_ms if dur_ms and dur_ms > 0 else None
            words = [
                {"text": tok, "start_ms": None, "end_ms": None, "syllables": []}
                for tok in full_text.split()
            ]

        elem = raw_line.get("element")
        singer_name: str | None = None
        says_opposite = False
        is_bg = False

        if isinstance(elem, dict):
            s_val = elem.get("singer")
            if s_val is not None:
                singer_name = str(s_val).strip()
            v_type = elem.get("vocal_type") or elem.get("type") or elem.get("role")
            if isinstance(v_type, str) and v_type.lower() in ("background", "bg"):
                is_bg = True
        elif isinstance(elem, list):
            for tag in elem:
                if isinstance(tag, str):
                    t_lower = tag.lower()
                    if t_lower in ("opposite", "right"):
                        says_opposite = True
                    elif t_lower in ("background", "bg"):
                        is_bg = True

        if singer_name and singer_name in bg_singers:
            is_bg = True

        t_trim = full_text.strip()
        if (t_trim.startswith("(") and t_trim.endswith(")")) or (
            t_trim.startswith("（") and t_trim.endswith("）")
        ):
            is_bg = True
        elif is_bg:
            full_text = f"({full_text})"

        lines.append({
            "start_ms": line_start,
            "end_ms": line_end,
            "text": full_text,
            "words": words,
            "is_background": is_bg,
            "_singer_name": singer_name,
            "_says_opposite": says_opposite,
        })

    if not lines:
        return "none", []

    distinct_singers: list[str] = []
    for l in lines:
        s_name = l.get("_singer_name")
        if s_name and s_name not in distinct_singers:
            distinct_singers.append(s_name)

    has_multi = len(distinct_singers) > 1
    for l in lines:
        s_name = l.pop("_singer_name", None)
        says_opp = l.pop("_says_opposite", False)
        if says_opp:
            l["singer"] = 1
        elif has_multi and s_name in distinct_singers:
            l["singer"] = distinct_singers.index(s_name)
        else:
            l["singer"] = 0

    lines.sort(key=lambda item: item["start_ms"] if item["start_ms"] is not None else 0)
    for i in range(len(lines) - 1):
        if lines[i]["end_ms"] is None and lines[i + 1]["start_ms"] is not None:
            lines[i]["end_ms"] = lines[i + 1]["start_ms"]

    if has_syllable_sync:
        sync_type = "syllable"
    elif has_word_sync:
        sync_type = "word"
    elif lines:
        sync_type = "line"
    else:
        sync_type = "none"

    return sync_type, lines



def parse_lyricsfile(yaml_text: str) -> tuple[str, list[dict[str, Any]]] | None:
    """
    Parses an LRCLIB Lyricsfile YAML document into lines with word-level timings.
    Returns (sync_type, lines) or None if parsing fails or no lines are found.
    """
    if not yaml_text or "lines:\n" not in yaml_text:
        return None

    lines_part = yaml_text.split("lines:\n", 1)[-1]
    raw_lines = lines_part.splitlines()

    lines: list[dict[str, Any]] = []
    curr_line: dict[str, Any] | None = None
    curr_words: list[dict[str, Any]] = []
    curr_word: dict[str, Any] | None = None
    in_words = False
    has_word_sync = False

    def finish_word() -> None:
        nonlocal curr_word
        if curr_word and curr_word.get("text"):
            curr_words.append(curr_word)
        curr_word = None

    def finish_line() -> None:
        nonlocal curr_line, curr_words, in_words, has_word_sync
        finish_word()
        if curr_line is not None:
            if curr_words:
                has_word_sync = True
                for i in range(len(curr_words) - 1):
                    if curr_words[i].get("end_ms") is None and curr_words[i + 1].get("start_ms") is not None:
                        curr_words[i]["end_ms"] = curr_words[i + 1]["start_ms"]
                if curr_words[-1].get("end_ms") is None:
                    curr_words[-1]["end_ms"] = curr_line.get("end_ms")
                curr_line["words"] = curr_words
            else:
                curr_line["words"] = [
                    {"text": tok, "start_ms": None, "end_ms": None, "syllables": []}
                    for tok in curr_line.get("text", "").split()
                ]
            t_trim = curr_line.get("text", "").strip()
            if (t_trim.startswith("(") and t_trim.endswith(")")) or (
                t_trim.startswith("（") and t_trim.endswith("）")
            ):
                curr_line["is_background"] = True
            lines.append(curr_line)
        curr_line = None
        curr_words = []
        in_words = False

    for raw in raw_lines:
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue

        if raw.startswith("- text:") or (raw.startswith("- ") and "text:" in raw):
            finish_line()
            text_val = raw.split("text:", 1)[1].strip()
            if (text_val.startswith("'") and text_val.endswith("'")) or (text_val.startswith('"') and text_val.endswith('"')):
                text_val = text_val[1:-1].replace("''", "'")
            curr_line = {
                "text": text_val,
                "start_ms": None,
                "end_ms": None,
                "words": [],
                "is_background": False,
                "singer": 0,
            }
            in_words = False
        elif curr_line is not None:
            if raw.strip().startswith("words:"):
                in_words = True
            elif in_words and raw.strip().startswith("- text:"):
                finish_word()
                w_text = raw.split("text:", 1)[1].strip()
                if (w_text.startswith("'") and w_text.endswith("'")) or (w_text.startswith('"') and w_text.endswith('"')):
                    w_text = w_text[1:-1].replace("''", "'")
                curr_word = {
                    "text": w_text.strip(),
                    "start_ms": None,
                    "end_ms": None,
                    "syllables": [],
                }
            elif in_words and curr_word is not None and "start_ms:" in raw:
                try:
                    curr_word["start_ms"] = int(raw.split("start_ms:", 1)[1].strip())
                except ValueError:
                    pass
            elif in_words and curr_word is not None and "end_ms:" in raw:
                try:
                    curr_word["end_ms"] = int(raw.split("end_ms:", 1)[1].strip())
                except ValueError:
                    pass
            elif not in_words and "start_ms:" in raw:
                try:
                    curr_line["start_ms"] = int(raw.split("start_ms:", 1)[1].strip())
                except ValueError:
                    pass
            elif not in_words and "end_ms:" in raw:
                try:
                    curr_line["end_ms"] = int(raw.split("end_ms:", 1)[1].strip())
                except ValueError:
                    pass

    finish_line()
    if not lines:
        return None

    lines.sort(key=lambda item: item["start_ms"] if item["start_ms"] is not None else 0)
    for i in range(len(lines) - 1):
        if lines[i]["end_ms"] is None and lines[i + 1]["start_ms"] is not None:
            lines[i]["end_ms"] = lines[i + 1]["start_ms"]

    sync_type = "word" if has_word_sync else ("line" if any(l["start_ms"] is not None for l in lines) else "none")
    return sync_type, lines




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