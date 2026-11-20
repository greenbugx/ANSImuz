from __future__ import annotations

import concurrent.futures
import logging
import threading
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

_SHARED_EXECUTOR: concurrent.futures.ThreadPoolExecutor | None = None
_EXECUTOR_LOCK = threading.Lock()


def get_shared_mirror_executor() -> concurrent.futures.ThreadPoolExecutor:
    """Returns a bounded, reusable thread pool for concurrent mirror requests."""
    global _SHARED_EXECUTOR
    if _SHARED_EXECUTOR is None:
        with _EXECUTOR_LOCK:
            if _SHARED_EXECUTOR is None:
                _SHARED_EXECUTOR = concurrent.futures.ThreadPoolExecutor(
                    max_workers=8,
                    thread_name_prefix="LyricsPlusMirror",
                )
    return _SHARED_EXECUTOR


class LyricsPlusProvider(LyricsProvider):
    """
    Syllable-timed lyrics provider using LyricsPlus (the open backend behind YouLy+).
    Aggregates Apple Music, QQ Music, and Musixmatch lyrics with syllable splits.

    Because mirrors run on community/volunteer infrastructure and may experience rate limits
    or downtime, this provider queries available mirrors concurrently using a bounded shared
    thread pool and takes the first usable result, promptly cancelling remaining mirror tasks.
    The winning mirror is remembered safely with locking so subsequent track queries prioritize
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
        self._lock = threading.Lock()

    @property
    def name(self) -> str:
        return "LyricsPlus"

    @property
    def last_good_mirror(self) -> str | None:
        with self._lock:
            return self._last_good

    def set_last_good_mirror(self, mirror: str | None) -> None:
        with self._lock:
            self._last_good = mirror

    def _headers(self) -> dict[str, str]:
        return {
            "User-Agent": self.user_agent,
            "Accept": "application/json",
        }

    def _fetch_from_mirror(
        self,
        host: str,
        params: dict[str, str],
        stop_event: threading.Event | None = None,
    ) -> tuple[str, dict[str, Any]] | None:
        if stop_event and stop_event.is_set():
            return None

        url = f"{host.rstrip('/')}/v2/lyrics/get"
        try:
            resp = self.session.get(
                url,
                params=params,
                headers=self._headers(),
                timeout=self.timeout,
            )
            if stop_event and stop_event.is_set():
                return None

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

    def _race_mirrors(
        self,
        params: dict[str, str],
        is_cancelled: Any = None,
    ) -> tuple[str, dict[str, Any]] | None:
        """
        Races available mirrors in parallel using a bounded shared executor,
        taking the first valid response and cancelling remaining requests.
        Prioritizes the last known good host if available.
        """
        with self._lock:
            last_good = self._last_good

        hosts: list[str] = []
        if last_good and last_good in self.mirrors:
            hosts.append(last_good)
            hosts.extend([m for m in self.mirrors if m != last_good])
        else:
            hosts = list(self.mirrors)

        if not hosts:
            return None

        # When only 1 host is configured, perform synchronous call directly
        if len(hosts) == 1:
            res = self._fetch_from_mirror(hosts[0], params)
            if res:
                with self._lock:
                    self._last_good = res[0]
                return res
            return None

        stop_event = threading.Event()
        executor = get_shared_mirror_executor()
        futures: dict[concurrent.futures.Future, str] = {}

        try:
            for host in hosts:
                if stop_event.is_set() or (is_cancelled and is_cancelled()):
                    break
                f = executor.submit(self._fetch_from_mirror, host, params, stop_event)
                futures[f] = host

            for future in concurrent.futures.as_completed(futures):
                if is_cancelled and is_cancelled():
                    stop_event.set()
                    break

                try:
                    res = future.result()
                    if res is not None:
                        winner_host, data = res
                        # Stop losing mirror requests immediately
                        stop_event.set()
                        with self._lock:
                            self._last_good = winner_host
                        logger.debug("LyricsPlus winner host: %s", winner_host)
                        return res
                except Exception as exc:
                    logger.debug("LyricsPlus mirror task exception: %s", exc)

            return None
        finally:
            stop_event.set()
            for f in futures:
                if not f.done():
                    f.cancel()

    def get_lyrics(self, track: dict[str, Any], is_cancelled: Any = None) -> dict[str, Any] | None:
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
            if is_cancelled and is_cancelled():
                return None

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

            res = self._race_mirrors(params, is_cancelled=is_cancelled)
            if res:
                winner_host, data = res
                sync_type, lines = parse_lyrics_plus(data)
                if lines:
                    return build_lyrics_response(
                        source="LyricsPlus",
                        sync_type=sync_type,
                        title=title,
                        artist=artist,
                        isrc=isrc,
                        lines=lines,
                        album=album,
                        duration_seconds=dur_sec,
                        raw_ttml=None,
                        raw_lrc=None,
                        attribution=self.attribution,
                        url=winner_host,
                    )

        return None

