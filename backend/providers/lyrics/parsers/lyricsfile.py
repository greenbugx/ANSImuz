from __future__ import annotations

from typing import Any


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

