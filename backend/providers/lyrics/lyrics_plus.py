from __future__ import annotations

import concurrent.futures
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
    parse_lyrics_plus,
)

logger = logging.getLogger(__name__)

DEFAULT_LYRICS_PLUS_ATTRIBUTION = "Lyrics provided by LyricsPlus"

DEFAULT_MIRRORS = [
    "https://lyricsplus.prjktla.my.id",
    "https://lyricsplus.atomix.one",
    "https://lyricsplus.binimum.org",
    "https://lyricsplus.prjktla.workers.dev",
    "https://lyricsplus-seven.vercel.app",
    "https://lyrics-plus-backend.vercel.app",
]


class LyricsPlusProvider(LyricsProvider):
    """
    Syllable-timed lyrics provider using LyricsPlus (the open backend behind YouLy+).
    Aggregates Apple Music, QQ Music, and Musixmatch lyrics with syllable splits.

    Because mirrors run on community/volunteer infrastructure and may experience rate limits
    or downtime, this provider queries available mirrors concurrently and takes the first
    usable result. The winning mirror is remembered so subsequent track queries prioritize
    the known-healthy host.
    """

    def __init__(
        self,
        mirrors: list[str] | None = None,
        user_agent: str = DEFAULT_USER_AGENT,
        attribution: str = DEFAULT_LYRICS_PLUS_ATTRIBUTION,
        timeout: float = 6.0,
        session: requests.Session | None = None,
    ) -> None:
        self.mirrors = list(mirrors or DEFAULT_MIRRORS)
        self.user_agent = user_agent
        self.attribution = attribution
        self.timeout = timeout
        self.session = session or requests.Session()
        self._last_good: str | None = None

    @property
    def name(self) -> str:
        return "LyricsPlus"

    @property
    def last_good_mirror(self) -> str | None:
        return self._last_good

    def _headers(self) -> dict[str, str]:
        return {
            "User-Agent": self.user_agent,
            "Accept": "application/json",
        }

    def _fetch_from_mirror(self, host: str, params: dict[str, str]) -> tuple[str, dict[str, Any]] | None:
        url = f"{host.rstrip('/')}/v2/lyrics/get"
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
                    if isinstance(data, dict) and data.get("lyrics"):
                        return host, data
                except ValueError:
                    return None
            else:
                logger.debug("LyricsPlus mirror %s returned HTTP %d", host, resp.status_code)
                return None
        except (requests.RequestException, OSError) as exc:
            logger.debug("LyricsPlus mirror %s connection error: %s", host, exc)
            return None

    def _race_mirrors(self, params: dict[str, str]) -> dict[str, Any] | None:
        """
        Races all available mirrors in parallel, taking the first valid response.
        Prioritizes the last known good host if available.
        """
        hosts: list[str] = []
        if self._last_good and self._last_good in self.mirrors:
            hosts.append(self._last_good)
            hosts.extend([m for m in self.mirrors if m != self._last_good])
        else:
            hosts = list(self.mirrors)

        if not hosts:
            return None

        # When only 1 host is configured, perform synchronous call directly
        if len(hosts) == 1:
            res = self._fetch_from_mirror(hosts[0], params)
            if res:
                self._last_good = res[0]
                return res[1]
            return None

        executor = concurrent.futures.ThreadPoolExecutor(max_workers=min(len(hosts), 6))
        try:
            futures = {
                executor.submit(self._fetch_from_mirror, host, params): host
                for host in hosts
            }
            for future in concurrent.futures.as_completed(futures):
                try:
                    res = future.result()
                    if res is not None:
                        winner_host, data = res
                        self._last_good = winner_host
                        logger.debug("LyricsPlus winner host: %s", winner_host)
                        return data
                except Exception as exc:
                    logger.debug("LyricsPlus mirror task exception: %s", exc)
            return None
        finally:
            executor.shutdown(wait=False, cancel_futures=True)

    def get_lyrics(self, track: dict[str, Any]) -> dict[str, Any] | None:
        """
        Retrieves syllable-level lyrics for the given track.
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
        query_titles = [title]
        if cleaned_title and cleaned_title != title:
            query_titles.append(cleaned_title)

        for q_title in query_titles:
            params: dict[str, str] = {
                "title": q_title,
                "artist": artist,
            }
            if duration_int is not None and duration_int > 0:
                params["duration"] = str(duration_int)
            if album and album.strip():
                params["album"] = album.strip()
            if isrc and isrc.strip():
                params["isrc"] = isrc.strip()

            data = self._race_mirrors(params)
            if data:
                sync_type, lines = parse_lyrics_plus(data)
                if lines:
                    return {
                        "source": "LyricsPlus",
                        "sync_type": sync_type,
                        "title": title,
                        "artist": artist,
                        "isrc": isrc,
                        "lines": lines,
                        "album": album,
                        "duration_seconds": dur_sec,
                        "language": None,
                        "plain_lyrics": "\n".join(l["text"] for l in lines),
                        "raw_ttml": None,
                        "raw_lrc": None,
                        "attribution": self.attribution,
                        "url": None,
                    }

        return None

