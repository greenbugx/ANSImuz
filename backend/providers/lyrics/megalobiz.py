from __future__ import annotations

import html
import logging
from typing import Any
import requests

from providers.lyrics.base import (
    DEFAULT_TIMEOUT,
    DEFAULT_USER_AGENT,
    LyricsProvider,
    clean_title_for_search,
    extract_album,
    extract_artists,
    extract_duration_seconds,
    extract_isrc,
    extract_title,
    parse_lrc,
)

logger = logging.getLogger(__name__)

DEFAULT_MEGALOBIZ_ATTRIBUTION = "Lyrics provided by Megalobiz"


def score_candidate(
    candidate: dict[str, Any],
    target_title: str,
    target_artist: str,
    target_dur: float | None,
) -> float:
    """
    Computes a composite match score for a Megalobiz search result.
    Balances API similarity score with audio recording duration difference,
    awarding a bonus for exact title match and penalizing unwanted remix/cover variants.
    """
    api_score = float(candidate.get("score") or 0.0)
    dur_diff = candidate.get("duration_diff")
    c_dur = candidate.get("duration")
    if dur_diff is None and target_dur and c_dur:
        try:
            dur_diff = abs(float(target_dur) - float(c_dur))
        except (ValueError, TypeError):
            dur_diff = None

    dur_score = 1.0
    if dur_diff is not None:
        try:
            diff_val = float(dur_diff)
            if diff_val > 2.0:
                dur_score = max(0.0, 1.0 - (diff_val - 2.0) / 15.0)
        except (ValueError, TypeError):
            pass

    c_title = (candidate.get("title") or "").strip().lower()
    t_title = target_title.strip().lower()
    exact_title_bonus = 0.15 if c_title == t_title else 0.0

    penalty = 0.0
    if ("remix" in c_title or "cover" in c_title) and not ("remix" in t_title or "cover" in t_title):
        penalty += 0.2
    if "ft." in c_title and "ft." not in t_title and "feat" not in t_title:
        penalty += 0.05

    return (api_score * 0.6 + dur_score * 0.4 + exact_title_bonus) - penalty


class MegalobizProvider(LyricsProvider):
    """
    Synchronized lyrics provider using the official Megalobiz Public API (v1).
    Fetches community-synchronized LRC sheets with millisecond precision line timestamps.
    """

    BASE_URL = "https://api.megalobiz.com/api/public/v1"

    def __init__(
        self,
        base_url: str = BASE_URL,
        user_agent: str = DEFAULT_USER_AGENT,
        attribution: str = DEFAULT_MEGALOBIZ_ATTRIBUTION,
        api_key: str | None = None,
        timeout: float = DEFAULT_TIMEOUT,
        session: requests.Session | None = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.user_agent = user_agent
        self.attribution = attribution
        self.api_key = api_key
        self.timeout = timeout
        self.session = session or requests.Session()

    @property
    def name(self) -> str:
        return "Megalobiz"

    def _headers(self) -> dict[str, str]:
        headers = {
            "User-Agent": self.user_agent,
            "Accept": "application/json",
        }
        if self.api_key:
            headers["X-API-Key"] = self.api_key
        return headers

    def search_candidates(
        self,
        title: str,
        artist: str,
        duration_sec: float | None = None,
    ) -> list[dict[str, Any]]:
        """Queries the Megalobiz /search endpoint for matching lyric sheets."""
        url = f"{self.base_url}/search"
        params: dict[str, Any] = {"limit": 10}

        cleaned_title = clean_title_for_search(title)
        query_attempts = [(title, artist)]
        if cleaned_title and cleaned_title != title:
            query_attempts.append((cleaned_title, artist))

        for q_title, q_artist in query_attempts:
            if q_title and q_artist:
                params["title"] = q_title
                params["artist"] = q_artist
            elif q_title:
                params["q"] = q_title
            else:
                continue

            if duration_sec and duration_sec > 0:
                params["duration"] = int(round(duration_sec))

            try:
                resp = self.session.get(
                    url,
                    params=params,
                    headers=self._headers(),
                    timeout=self.timeout,
                )
                if resp.status_code == 200:
                    data = resp.json()
                    results = data.get("results")
                    if isinstance(results, list) and results:
                        return results
                elif resp.status_code == 429:
                    logger.warning("Megalobiz rate limit exceeded (HTTP 429)")
                    return []
            except (requests.RequestException, OSError, ValueError) as exc:
                logger.debug("Megalobiz search failed for %s - %s: %s", q_artist, q_title, exc)

        return []

    def get_sheet(self, sheet_id: int | str) -> dict[str, Any] | None:
        """Retrieves a full synced lyric sheet by its Megalobiz ID."""
        url = f"{self.base_url}/lrc/{sheet_id}"
        try:
            resp = self.session.get(url, headers=self._headers(), timeout=self.timeout)
            if resp.status_code == 200:
                data = resp.json()
                if isinstance(data, dict):
                    return data
            elif resp.status_code == 429:
                logger.warning("Megalobiz rate limit exceeded on get_sheet (HTTP 429)")
                return None
        except (requests.RequestException, OSError, ValueError) as exc:
            logger.debug("Megalobiz get_sheet failed for ID %s: %s", sheet_id, exc)

        return None

    def get_lyrics(self, track: dict[str, Any]) -> dict[str, Any] | None:
        title = extract_title(track)
        if not title:
            return None

        artists = extract_artists(track)
        artist = artists[0] if artists else ""
        album = extract_album(track)
        dur_sec = extract_duration_seconds(track)
        isrc = extract_isrc(track)

        candidates = self.search_candidates(title, artist, dur_sec)
        if not candidates:
            return None

        # Require a minimum similarity score of 0.50
        viable = [c for c in candidates if float(c.get("score") or 0.0) >= 0.50]
        if not viable:
            return None

        best_candidate = max(
            viable,
            key=lambda c: score_candidate(c, title, artist, dur_sec),
        )

        if score_candidate(best_candidate, title, artist, dur_sec) < 0.50:
            return None

        sheet_id = best_candidate.get("id")
        if not sheet_id:
            return None

        sheet = self.get_sheet(sheet_id)
        if not sheet:
            return None

        # 1. Parse raw LRC string if available
        raw_lrc = sheet.get("lrc")
        sync_type = "line"
        lines: list[dict[str, Any]] = []

        if raw_lrc and isinstance(raw_lrc, str) and raw_lrc.strip():
            unescaped_lrc = html.unescape(raw_lrc)
            s_type, parsed_lines = parse_lrc(unescaped_lrc)
            if parsed_lines:
                sync_type = s_type
                lines = parsed_lines

        # 2. Fallback to structured lines array in JSON
        if not lines and isinstance(sheet.get("lines"), list):
            for item in sheet["lines"]:
                if not isinstance(item, dict):
                    continue
                t_ms = item.get("time_ms")
                txt = html.unescape(item.get("text") or "").strip()
                if not txt:
                    continue
                words = [
                    {"text": tok, "start_ms": None, "end_ms": None, "syllables": []}
                    for tok in txt.split()
                ]
                is_bg = (txt.startswith("(") and txt.endswith(")")) or (
                    txt.startswith("（") and txt.endswith("）")
                )
                lines.append({
                    "text": txt,
                    "start_ms": t_ms,
                    "end_ms": None,
                    "words": words,
                    "is_background": is_bg,
                    "singer": 0,
                })

            for i in range(len(lines) - 1):
                if lines[i]["end_ms"] is None and lines[i + 1]["start_ms"] is not None:
                    lines[i]["end_ms"] = lines[i + 1]["start_ms"]

        if not lines:
            return None

        # Decode any residual HTML entities in line text
        for line in lines:
            if "&" in line["text"]:
                line["text"] = html.unescape(line["text"])

        c_title = sheet.get("title") or best_candidate.get("title") or title
        c_artist = sheet.get("artist") or best_candidate.get("artist") or artist
        c_dur = sheet.get("duration") or best_candidate.get("duration") or dur_sec
        sheet_url = sheet.get("url") or best_candidate.get("url")

        return {
            "source": "Megalobiz",
            "sync_type": sync_type,
            "title": html.unescape(c_title),
            "artist": html.unescape(c_artist),
            "isrc": isrc,
            "lines": lines,
            "album": album,
            "duration_seconds": c_dur,
            "language": None,
            "plain_lyrics": "\n".join(l["text"] for l in lines),
            "raw_ttml": None,
            "raw_lrc": raw_lrc,
            "attribution": self.attribution,
            "url": sheet_url,
        }