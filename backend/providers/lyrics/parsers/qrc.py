from __future__ import annotations

import html
import re
from typing import Any


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

