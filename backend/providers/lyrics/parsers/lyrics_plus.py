from __future__ import annotations

from typing import Any


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

