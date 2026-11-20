from __future__ import annotations

import re
from typing import Any

from providers.lyrics.timestamps import parse_timestamp

TIME_TAG_PATTERN = re.compile(r"\[((?:\d{1,2}:)?\d{1,3}:\d{2}(?:\.\d{1,3})?)\]")
WORD_TAG_PATTERN = re.compile(r"<((?:\d{1,2}:)?\d{1,3}:\d{2}(?:\.\d{1,3})?)>([^<]*)")
METADATA_TAG_PATTERN = re.compile(r"^\[\s*[a-zA-Z_]+\s*:[^\]]*\]")
OFFSET_TAG_PATTERN = re.compile(r"^\[offset:\s*([+-]?\d+)\s*\]", re.IGNORECASE)


def parse_lrc(lrc_text: str) -> tuple[str, list[dict[str, Any]]]:
    """
    Parses LRC text supporting standard LRC, enhanced LRC with <mm:ss.xx> word tags,
    syllable groupings, multi-timestamp lines, offset tags, empty instrumental lines,
    and unsynchronized plain text with preserved stanza breaks.

    Returns (sync_type, lines) where sync_type is 'syllable', 'word', 'line', or 'none'.
    """
    if not lrc_text or not lrc_text.strip():
        return "none", []

    raw_lines = lrc_text.splitlines()

    # First pass: extract global offset tag if present
    offset_ms = 0
    for raw in raw_lines:
        offset_match = OFFSET_TAG_PATTERN.match(raw.strip())
        if offset_match:
            try:
                offset_ms = int(offset_match.group(1))
            except ValueError:
                pass

    timed_lines: list[dict[str, Any]] = []
    plain_lines: list[str] = []
    has_word_sync = False
    has_syllable_sync = False

    for raw_line in raw_lines:
        line = raw_line.strip()
        if not line:
            # Preserve stanza separation for plain text lines
            plain_lines.append("")
            continue

        # Ignore metadata tags [ar:...], [ti:...], [offset:...], etc.
        if METADATA_TAG_PATTERN.match(line):
            continue

        tags = TIME_TAG_PATTERN.findall(line)
        if not tags:
            plain_lines.append(line)
            continue

        remainder = TIME_TAG_PATTERN.sub("", line).strip()
        word_matches = list(WORD_TAG_PATTERN.finditer(remainder))

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
            if clean_text:
                for tok in clean_text.split():
                    words.append({
                        "text": tok,
                        "start_ms": None,
                        "end_ms": None,
                        "syllables": [],
                    })

        # Multi-timestamp line handling: each tag generates a distinct timed line
        for tag in tags:
            tag_ms = parse_timestamp(tag)
            if tag_ms is not None:
                tag_ms = max(0, tag_ms + offset_ms)
            timed_lines.append({
                "text": clean_text,
                "start_ms": tag_ms,
                "end_ms": line_end_ms,
                "words": [dict(w) for w in words],
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

    # Plain unsynchronized lyrics with stanza break preservation
    parsed_plain: list[dict[str, Any]] = []
    last_was_blank = True
    for pl in plain_lines:
        if not pl:
            if not last_was_blank and parsed_plain:
                parsed_plain.append({
                    "text": "",
                    "start_ms": None,
                    "end_ms": None,
                    "words": [],
                    "singer": 0,
                })
                last_was_blank = True
            continue

        last_was_blank = False
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

    # Strip any trailing empty lines
    while parsed_plain and parsed_plain[-1]["text"] == "":
        parsed_plain.pop()

    return "none", parsed_plain