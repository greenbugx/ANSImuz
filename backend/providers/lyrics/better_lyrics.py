from __future__ import annotations

import logging
from typing import Any
import requests

from providers.lyrics.base import (
    DEFAULT_TIMEOUT,
    DEFAULT_USER_AGENT,
    LyricsProvider,
    build_lyrics_response,
    clean_title_for_search,
    extract_album,
    extract_artists,
    extract_duration_seconds,
    extract_isrc,
    extract_title,
    parse_qrc,
    parse_ttml,
)

logger = logging.getLogger(__name__)

DEFAULT_BETTER_LYRICS_ATTRIBUTION = "Lyrics provided by BetterLyrics"


class BetterLyricsProvider(LyricsProvider):
    """
    Word/syllable-timed lyrics provider using BetterLyrics' backend APIs:
    - Primary endpoint: Apple Music TTML with per-syllable timing and duet singer channels.
    - Fallback endpoint (Portato): QQ Music QRC with per-word / syllable karaoke timings.

    Uncached queries to boidu.dev may return 401 when an API key is required, in which case
    the provider gracefully returns None to allow subsequent fallback providers in the chain
    (such as LrcRedProvider) to serve lyrics.
    """

    BASE_URL = "https://lyrics-api.boidu.dev/getLyrics"
    PORTATO_URL = "https://lyrics-api.boidu.dev/qq/getLyrics"

    def __init__(
        self,
        base_url: str = BASE_URL,
        portato_url: str = PORTATO_URL,
        user_agent: str = DEFAULT_USER_AGENT,
        attribution: str = DEFAULT_BETTER_LYRICS_ATTRIBUTION,
        timeout: float = DEFAULT_TIMEOUT,
        session: requests.Session | None = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.portato_url = portato_url.rstrip("/")
        self.user_agent = user_agent
        self.attribution = attribution
        self.timeout = timeout
        self.session = session or requests.Session()

    @property
    def name(self) -> str:
        return "BetterLyrics"

    def _headers(self) -> dict[str, str]:
        return {
            "User-Agent": self.user_agent,
            "Accept": "application/json",
        }

    def _fetch_endpoint(
        self,
        url: str,
        title: str,
        artist: str,
        duration_sec: int | None,
        album: str | None,
    ) -> dict[str, Any] | None:
        params: dict[str, str] = {"s": title, "a": artist}
        if duration_sec is not None and duration_sec > 0:
            params["d"] = str(duration_sec)
        if album and album.strip():
            params["al"] = album.strip()

        try:
            resp = self.session.get(
                url,
                params=params,
                headers=self._headers(),
                timeout=self.timeout,
            )
            if resp.status_code == 200:
                try:
                    data = resp.json()
                    if isinstance(data, dict):
                        return data
                except ValueError:
                    return None
            elif resp.status_code in (401, 404):
                logger.debug("BetterLyrics returned status %d for %s - %s", resp.status_code, artist, title)
                return None
            else:
                logger.debug("BetterLyrics endpoint %s returned HTTP %d", url, resp.status_code)
                return None
        except (requests.RequestException, OSError) as exc:
            logger.debug("BetterLyrics request to %s failed: %s", url, exc)
            return None

    def get_lyrics(self, track: dict[str, Any]) -> dict[str, Any] | None:
        """
        Retrieves syllable/word-level lyrics for the given track.
        Tries the primary TTML endpoint first, then falls back to Portato (QQ QRC).
        Returns normalized lyrics dictionary or None if unavailable.
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

        # 1. Primary endpoint: BetterLyrics Apple Music TTML
        for t_query in ([title, cleaned_title] if cleaned_title != title else [title]):
            data = self._fetch_endpoint(self.base_url, t_query, artist, duration_int, album)
            if data and "ttml" in data and isinstance(data["ttml"], str):
                sync_type, lines = parse_ttml(data["ttml"])
                if lines:
                    return build_lyrics_response(
                        source="BetterLyrics",
                        sync_type=sync_type,
                        title=title,
                        artist=artist,
                        isrc=isrc,
                        lines=lines,
                        album=album,
                        duration_seconds=dur_sec,
                        raw_ttml=data["ttml"],
                        raw_lrc=None,
                        attribution=self.attribution,
                        url=None,
                    )

        # 2. Fallback endpoint: Portato (QQ Music QRC)
        for t_query in ([title, cleaned_title] if cleaned_title != title else [title]):
            data = self._fetch_endpoint(self.portato_url, t_query, artist, duration_int, album)
            if data and "lyrics" in data and isinstance(data["lyrics"], str):
                sync_type, lines = parse_qrc(data["lyrics"])
                if lines:
                    return build_lyrics_response(
                        source="BetterLyrics (QQ)",
                        sync_type=sync_type,
                        title=title,
                        artist=artist,
                        isrc=isrc,
                        lines=lines,
                        album=album,
                        duration_seconds=dur_sec,
                        raw_ttml=None,
                        raw_lrc=data["lyrics"],
                        attribution=self.attribution,
                        url=None,
                    )

        return None

