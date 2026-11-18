from __future__ import annotations

import logging
from typing import Any
import requests

from providers.lyrics.base import (
    DEFAULT_TIMEOUT,
    DEFAULT_USER_AGENT,
    LyricsProvider,
    clean_title_for_search,
    compute_similarity,
    extract_album,
    extract_artists,
    extract_duration_seconds,
    extract_isrc,
    extract_title,
    normalize_text,
    parse_lrc,
    parse_lyricsfile,
)

logger = logging.getLogger(__name__)

DEFAULT_LRCLIB_ATTRIBUTION = "Lyrics provided by LRCLIB"


class LrclibProvider(LyricsProvider):
    """
    Lyrics provider implementation using the official LRCLIB API (https://lrclib.net).
    Supports:
    - Word-level synced lyrics via Lyricsfile YAML format (`hasWordSync: true`).
    - Line-level synced lyrics via standard LRC format (`syncedLyrics`).
    - Unsynchronized plain lyrics (`plainLyrics`).
    """

    BASE_URL = "https://lrclib.net"

    def __init__(
        self,
        base_url: str = BASE_URL,
        user_agent: str = DEFAULT_USER_AGENT,
        attribution: str = DEFAULT_LRCLIB_ATTRIBUTION,
        timeout: float = DEFAULT_TIMEOUT,
        session: requests.Session | None = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.user_agent = user_agent
        self.attribution = attribution
        self.timeout = timeout
        self.session = session or requests.Session()

    @property
    def name(self) -> str:
        return "LRCLIB"

    def _headers(self) -> dict[str, str]:
        return {
            "User-Agent": self.user_agent,
            "Accept": "application/json",
        }

    def _get_by_signature(
        self,
        title: str,
        artist: str,
        duration_sec: int | None,
        album: str | None,
    ) -> dict[str, Any] | None:
        params: dict[str, str] = {
            "track_name": title,
            "artist_name": artist,
        }
        if duration_sec is not None and duration_sec > 0:
            params["duration"] = str(duration_sec)
        if album and album.strip():
            params["album_name"] = album.strip()

        try:
            resp = self.session.get(
                f"{self.base_url}/api/get",
                params=params,
                headers=self._headers(),
                timeout=self.timeout,
            )
            if resp.status_code == 200:
                try:
                    data = resp.json()
                    if isinstance(data, dict) and not data.get("instrumental"):
                        return data
                except ValueError:
                    return None
            elif resp.status_code in (404, 400):
                return None
            else:
                logger.debug("LRCLIB /api/get returned HTTP %d", resp.status_code)
                return None
        except (requests.RequestException, OSError) as exc:
            logger.debug("LRCLIB /api/get request failed: %s", exc)
            return None

    def _search_best(
        self,
        title: str,
        artist: str,
        duration_sec: int | None,
        album: str | None,
    ) -> dict[str, Any] | None:
        params: dict[str, str] = {
            "track_name": title,
            "artist_name": artist,
        }
        try:
            resp = self.session.get(
                f"{self.base_url}/api/search",
                params=params,
                headers=self._headers(),
                timeout=self.timeout,
            )
            if resp.status_code == 200:
                try:
                    candidates = resp.json()
                except ValueError:
                    return None
                if not isinstance(candidates, list) or not candidates:
                    return None

                best_candidate: dict[str, Any] | None = None
                best_score = -1.0

                norm_title = normalize_text(title)
                norm_artist = normalize_text(artist)

                for c in candidates:
                    if not isinstance(c, dict) or c.get("instrumental"):
                        continue
                    c_title = normalize_text(c.get("trackName") or "")
                    c_artist = normalize_text(c.get("artistName") or "")
                    c_dur = c.get("duration")

                    title_score = compute_similarity(norm_title, c_title)
                    artist_score = compute_similarity(norm_artist, c_artist)

                    dur_score = 1.0
                    if duration_sec and isinstance(c_dur, (int, float)) and c_dur > 0:
                        diff = abs(duration_sec - float(c_dur))
                        if diff > 10.0:
                            dur_score = max(0.0, 1.0 - (diff - 10.0) / 20.0)

                    total_score = title_score * 0.5 + artist_score * 0.3 + dur_score * 0.2
                    if c.get("hasWordSync"):
                        total_score += 0.05
                    elif c.get("syncedLyrics"):
                        total_score += 0.02

                    if total_score > best_score and total_score >= 0.60:
                        best_score = total_score
                        best_candidate = c

                return best_candidate
        except (requests.RequestException, OSError) as exc:
            logger.debug("LRCLIB /api/search request failed: %s", exc)
            return None
        return None

    def get_lyrics(self, track: dict[str, Any]) -> dict[str, Any] | None:
        """
        Retrieves lyrics for the given track using LRCLIB.net.
        Attempts exact signature match (/api/get) first, then search fallback (/api/search).
        Prefers word-level lyricsfile if available, otherwise LRC-synced lyrics.
        """
        title = extract_title(track)
        if not title:
            return None

        artists = extract_artists(track)
        artist = artists[0] if artists else ""
        album = extract_album(track)
        dur_sec = extract_duration_seconds(track)
        duration_int = int(round(dur_sec)) if dur_sec is not None and dur_sec > 0 else None
        isrc = extract_isrc(track)

        cleaned_title = clean_title_for_search(title)
        query_titles = [title]
        if cleaned_title and cleaned_title != title:
            query_titles.append(cleaned_title)

        record: dict[str, Any] | None = None
        for q_title in query_titles:
            record = self._get_by_signature(q_title, artist, duration_int, album)
            if record:
                break

        if not record:
            for q_title in query_titles:
                record = self._search_best(q_title, artist, duration_int, album)
                if record:
                    break

        if not record:
            return None

        # 1. Prefer word-synced lyricsfile
        if record.get("hasWordSync") and record.get("lyricsfile"):
            res = parse_lyricsfile(record["lyricsfile"])
            if res:
                sync_type, lines = res
                if lines:
                    return {
                        "source": "LRCLIB",
                        "sync_type": sync_type,
                        "title": record.get("trackName") or title,
                        "artist": record.get("artistName") or artist,
                        "isrc": isrc,
                        "lines": lines,
                        "album": record.get("albumName") or album,
                        "duration_seconds": record.get("duration") or dur_sec,
                        "language": None,
                        "plain_lyrics": record.get("plainLyrics") or "\n".join(l["text"] for l in lines),
                        "raw_ttml": None,
                        "raw_lrc": record.get("syncedLyrics"),
                        "attribution": self.attribution,
                        "url": f"{self.base_url}/api/get/{record['id']}" if record.get("id") else None,
                    }

        # 2. Line-synced lyrics
        if record.get("syncedLyrics"):
            sync_type, lines = parse_lrc(record["syncedLyrics"])
            if lines:
                return {
                    "source": "LRCLIB",
                    "sync_type": sync_type,
                    "title": record.get("trackName") or title,
                    "artist": record.get("artistName") or artist,
                    "isrc": isrc,
                    "lines": lines,
                    "album": record.get("albumName") or album,
                    "duration_seconds": record.get("duration") or dur_sec,
                    "language": None,
                    "plain_lyrics": record.get("plainLyrics") or "\n".join(l["text"] for l in lines),
                    "raw_ttml": None,
                    "raw_lrc": record.get("syncedLyrics"),
                    "attribution": self.attribution,
                    "url": f"{self.base_url}/api/get/{record['id']}" if record.get("id") else None,
                }

        # 3. Plain unsynchronized lyrics
        if record.get("plainLyrics"):
            sync_type, lines = parse_lrc(record["plainLyrics"])
            if lines:
                return {
                    "source": "LRCLIB",
                    "sync_type": "none",
                    "title": record.get("trackName") or title,
                    "artist": record.get("artistName") or artist,
                    "isrc": isrc,
                    "lines": lines,
                    "album": record.get("albumName") or album,
                    "duration_seconds": record.get("duration") or dur_sec,
                    "language": None,
                    "plain_lyrics": record.get("plainLyrics"),
                    "raw_ttml": None,
                    "raw_lrc": None,
                    "attribution": self.attribution,
                    "url": f"{self.base_url}/api/get/{record['id']}" if record.get("id") else None,
                }

        return None

