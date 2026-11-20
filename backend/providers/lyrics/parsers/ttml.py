from __future__ import annotations

import re
import xml.etree.ElementTree as ET
from typing import Any

from providers.lyrics.normalization import get_xml_attr
from providers.lyrics.timestamps import parse_timestamp


def parse_ttml(ttml_text: str) -> tuple[str, list[dict[str, Any]]]:
    """
    Parses a TTML XML document containing timed lyrics lines and spans.
    Handles XML namespaces, frame-rates, container offsets, relative span timing,
    zero-millisecond timestamps, paragraph lines, word timings, syllable timings,
    and multi-singer agent assignments.

    Returns (sync_type, lines) where sync_type is 'syllable', 'word', 'line', or 'none'.
    """
    if not ttml_text or not ttml_text.strip():
        return "none", []

    try:
        root = ET.fromstring(ttml_text.strip())
    except ET.ParseError:
        return "none", []

    # Extract document-level frame rate if specified in ttp:frameRate
    doc_frame_rate: float | None = 30.0
    raw_frame_rate = get_xml_attr(root, "frameRate")
    if raw_frame_rate:
        try:
            parsed_fr = float(raw_frame_rate)
            if parsed_fr > 0:
                doc_frame_rate = parsed_fr
        except (ValueError, TypeError):
            pass

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

    # Process body / div container offsets
    body_elem = None
    for child in root.iter():
        tag = child.tag.split("}")[-1] if "}" in child.tag else child.tag
        if tag == "body":
            body_elem = child
            break

    body_begin = 0
    if body_elem is not None:
        parsed_body_begin = parse_timestamp(get_xml_attr(body_elem, "begin"), frame_rate=doc_frame_rate)
        if parsed_body_begin is not None:
            body_begin = parsed_body_begin

    for elem in root.iter():
        tag = elem.tag.split("}")[-1] if "}" in elem.tag else elem.tag
        if tag != "p":
            continue

        agent_ref = get_xml_attr(elem, "agent")
        if agent_ref and agent_ref not in agent_map:
            agent_map[agent_ref] = len(agent_order)
            agent_order.append(agent_ref)

        raw_p_begin = get_xml_attr(elem, "begin")
        raw_p_dur = get_xml_attr(elem, "dur")
        raw_p_end = get_xml_attr(elem, "end")

        parsed_p_begin = parse_timestamp(raw_p_begin, frame_rate=doc_frame_rate)
        parsed_p_dur = parse_timestamp(raw_p_dur, frame_rate=doc_frame_rate)
        parsed_p_end = parse_timestamp(raw_p_end, frame_rate=doc_frame_rate)

        # Inherit container offset if body has offset
        p_begin = parsed_p_begin + body_begin if parsed_p_begin is not None else None
        p_end = parsed_p_end + body_begin if parsed_p_end is not None else None

        if p_end is None and p_begin is not None and parsed_p_dur is not None:
            p_end = p_begin + parsed_p_dur

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
            # Paragraph without timed inner spans
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

                    raw_w_begin = get_xml_attr(s, "begin")
                    raw_w_end = get_xml_attr(s, "end")
                    parsed_w_begin = parse_timestamp(raw_w_begin, frame_rate=doc_frame_rate)
                    parsed_w_end = parse_timestamp(raw_w_end, frame_rate=doc_frame_rate)

                    # Compute span timing supporting explicit relative (+1s) or parent-relative offsets
                    if parsed_w_begin is not None:
                        if raw_w_begin and raw_w_begin.strip().startswith("+"):
                            w_begin = (p_begin or 0) + parsed_w_begin
                        elif p_begin is not None and parsed_w_begin < p_begin:
                            # Relative offset from p_begin
                            w_begin = p_begin + parsed_w_begin
                        else:
                            w_begin = parsed_w_begin + body_begin
                    else:
                        w_begin = p_begin

                    if parsed_w_end is not None:
                        if raw_w_end and raw_w_end.strip().startswith("+"):
                            w_end = (p_begin or 0) + parsed_w_end
                        elif p_begin is not None and parsed_w_end < p_begin:
                            w_end = p_begin + parsed_w_end
                        else:
                            w_end = parsed_w_end + body_begin
                    else:
                        w_end = p_end

                    if w_begin is not None:
                        has_word_sync = True

                    nested = [c for c in s if c.tag.split("}")[-1] == "span"]
                    syls: list[dict[str, Any]] = []
                    if nested:
                        for ns in nested:
                            ns_text = "".join(ns.itertext()).strip()
                            if ns_text:
                                raw_ns_begin = get_xml_attr(ns, "begin")
                                raw_ns_end = get_xml_attr(ns, "end")
                                parsed_ns_begin = parse_timestamp(raw_ns_begin, frame_rate=doc_frame_rate)
                                parsed_ns_end = parse_timestamp(raw_ns_end, frame_rate=doc_frame_rate)

                                if parsed_ns_begin is not None:
                                    if raw_ns_begin and raw_ns_begin.strip().startswith("+"):
                                        ns_begin = (w_begin or 0) + parsed_ns_begin
                                    elif w_begin is not None and parsed_ns_begin < w_begin:
                                        ns_begin = w_begin + parsed_ns_begin
                                    else:
                                        ns_begin = parsed_ns_begin + body_begin
                                else:
                                    ns_begin = w_begin

                                if parsed_ns_end is not None:
                                    if raw_ns_end and raw_ns_end.strip().startswith("+"):
                                        ns_end = (w_begin or 0) + parsed_ns_end
                                    elif w_begin is not None and parsed_ns_end < w_begin:
                                        ns_end = w_begin + parsed_ns_end
                                    else:
                                        ns_end = parsed_ns_end + body_begin
                                else:
                                    ns_end = w_end

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
                    raw_s_begin = get_xml_attr(s, "begin")
                    raw_s_end = get_xml_attr(s, "end")
                    parsed_s_begin = parse_timestamp(raw_s_begin, frame_rate=doc_frame_rate)
                    parsed_s_end = parse_timestamp(raw_s_end, frame_rate=doc_frame_rate)

                    if parsed_s_begin is not None:
                        has_word_sync = True
                        if raw_s_begin and raw_s_begin.strip().startswith("+"):
                            eff_s_begin = (p_begin or 0) + parsed_s_begin
                        elif p_begin is not None and parsed_s_begin < p_begin:
                            eff_s_begin = p_begin + parsed_s_begin
                        else:
                            eff_s_begin = parsed_s_begin + body_begin
                    else:
                        eff_s_begin = p_begin

                    if parsed_s_end is not None:
                        if raw_s_end and raw_s_end.strip().startswith("+"):
                            eff_s_end = (p_begin or 0) + parsed_s_end
                        elif p_begin is not None and parsed_s_end < p_begin:
                            eff_s_end = p_begin + parsed_s_end
                        else:
                            eff_s_end = parsed_s_end + body_begin
                    else:
                        eff_s_end = p_end

                    current_syllables.append({
                        "text": s_text,
                        "start_ms": eff_s_begin,
                        "end_ms": eff_s_end,
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