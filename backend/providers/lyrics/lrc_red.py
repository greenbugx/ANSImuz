from __future__ import annotations

import math
import urllib.parse
from typing import Any
import requests

from exceptions import (
    ProviderError,
    RateLimitError,
    TrackNotFoundError,
    classify_error,
)
from providers.lyrics.base import (
    DEFAULT_ATTRIBUTION,
    DEFAULT_TIMEOUT,
    DEFAULT_USER_AGENT,
    LyricsProvider,
    build_lyrics_response,
    compute_similarity,
    extract_album,
    extract_artists,
    extract_duration_seconds,
    extract_isrc,
    extract_title,
    normalize_text,
    parse_lrc,
    parse_ttml,
)

MIN_MATCH_SCORE = 0.60


def _version_conflicts(source_title: str, candidate_title: str) -> list[str]:
    """Detects version descriptor mismatches between expected track title and candidate."""
    source = normalize_text(source_title).split()
    candidate = normalize_text(candidate_title).split()
    markers = [
        ("live",),
        ("remix",),
        ("cover",),
        ("karaoke",),
        ("instrumental",),
        ("nightcore",),
        ("slowed",),
        ("sped", "up"),
        ("acoustic",),
    ]
    conflicts = []
    for marker in markers:
        size = len(marker)
        candidate_has = any(
            tuple(candidate[i : i + size]) == marker
            for i in range(len(candidate) - size + 1)
        )
        source_has = any(
            tuple(source[i : i + size]) == marker
            for i in range(len(source) - size + 1)
        )
        if candidate_has and not source_has:
            conflicts.append(" ".join(marker))
    return conflicts


class LrcRedProvider(LyricsProvider):
    """
    Lyrics provider implementation using the lrc.red API.
    Supports TTML word-level timing, LRC line/word-level timing, and plain lyrics.
    """

    BASE_URL = "https://lrc.red"

    def __init__(
        self,
        base_url: str = BASE_URL,
        user_agent: str = DEFAULT_USER_AGENT,
        attribution: str = DEFAULT_ATTRIBUTION,
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
        return "lrc.red"

    def _headers(self, accept: str = "application/json") -> dict[str, str]:
        return {
            "User-Agent": self.user_agent,
            "Accept": accept,
        }

    def _request(
        self,
        endpoint: str,
        params: dict[str, Any] | None = None,
        accept: str = "application/json",
    ) -> requests.Response | None:
        """
        Executes an HTTP GET request with standard User-Agent, timeout, and error classification.
        Returns the Response object, or None if the resource returns 404 Not Found.
        """
        url = endpoint if endpoint.startswith("http") else f"{self.base_url}{endpoint}"
        try:
            resp = self.session.get(
                url,
                params=params,
                headers=self._headers(accept=accept),
                timeout=self.timeout,
            )
        except Exception as exc:
            raise classify_error(f"lrc.red request to {url} failed", exc) from exc

        if resp.status_code == 404:
            return None
        if resp.status_code == 429:
            raise RateLimitError(
                f"lrc.red rate limit exceeded (429) while requesting {url}"
            )
        if resp.status_code >= 400:
            raise ProviderError(
                f"lrc.red HTTP {resp.status_code} error from {url}: {resp.text[:200]}"
            )

        return resp

    def search(self, query: str) -> list[dict[str, Any]]:
        """
        Searches lrc.red for tracks matching the query.
        Returns list of hit dictionaries containing isrc, title, artist, album, duration, etc.
        """
        query = query.strip()
        if not query:
            return []

        resp = self._request("/search.json", params={"q": query})
        if resp is None:
            return []

        try:
            data = resp.json()
            return data.get("hits", []) or []
        except Exception as exc:
            raise ProviderError(f"Failed to parse lrc.red search JSON: {exc}") from exc

    def get_track_metadata(self, isrc: str) -> dict[str, Any] | None:
        """
        Retrieves track metadata by ISRC code (/s/{isrc}.json).
        Returns metadata dict or None if not found.
        """
        isrc = isrc.strip()
        if not isrc:
            return None

        resp = self._request(f"/s/{isrc}.json")
        if resp is None:
            return None

        try:
            return resp.json()
        except Exception as exc:
            raise ProviderError(f"Failed to parse lrc.red track metadata JSON: {exc}") from exc

    def get_ttml(self, isrc: str, ttml_url: str | None = None) -> str | None:
        """
        Fetches raw TTML lyrics XML document for the given ISRC or specific URL.
        Returns TTML string or None if unavailable.
        """
        url = ttml_url or f"/s/{isrc}.ttml"
        resp = self._request(url, accept="application/xml, text/xml")
        if resp is None:
            return None
        return resp.text

    def get_lrc(self, isrc: str, lrc_url: str | None = None) -> str | None:
        """
        Fetches raw LRC lyrics document for the given ISRC or specific URL.
        Returns LRC string or None if unavailable.
        """
        url = lrc_url or f"/s/{isrc}.lrc"
        resp = self._request(url, accept="text/plain")
        if resp is None:
            return None
        return resp.text

    def match_track(
        self,
        track: dict[str, Any],
        candidates: list[dict[str, Any]],
    ) -> dict[str, Any] | None:
        """
        Ranks candidate hits against the expected track using title, artist, album, and duration.
        Returns the best matching candidate, or None if none meet the acceptance threshold.
        """
        if not candidates:
            return None

        expected_title = extract_title(track)
        if not expected_title:
            return None

        expected_artists = extract_artists(track)
        expected_album = extract_album(track)
        expected_dur = extract_duration_seconds(track)

        scored_candidates: list[tuple[float, dict[str, Any]]] = []

        for cand in candidates:
            cand_title = str(cand.get("title") or "")
            cand_artist = str(cand.get("artist") or "")
            cand_album = str(cand.get("album") or "")
            cand_dur = cand.get("duration")
            if isinstance(cand_dur, (int, float)):
                cand_dur = float(cand_dur)
            else:
                cand_dur = None

            # Title similarity
            title_score = compute_similarity(expected_title, cand_title)

            # Artist similarity
            if expected_artists:
                artist_score = max(
                    (
                        compute_similarity(exp_art, cand_artist)
                        for exp_art in expected_artists
                        if exp_art
                    ),
                    default=0.0,
                )
            else:
                artist_score = 0.5  # Neutral if no artist specified

            # Album similarity
            album_score: float | None = None
            if expected_album and cand_album:
                album_score = compute_similarity(expected_album, cand_album)

            # Duration similarity
            duration_score: float | None = None
            diff: float | None = None
            tolerance: float | None = None
            if expected_dur is not None and cand_dur is not None:
                diff = abs(expected_dur - cand_dur)
                tolerance = max(10.0, expected_dur * 0.10)
                duration_score = math.exp(-diff / max(5.0, expected_dur * 0.03))

            # Version conflict check
            conflicts = _version_conflicts(expected_title, cand_title)
            if conflicts:
                # Heavy penalty if user didn't ask for a remix/live version
                title_score *= 0.5

            # Calculate composite weighted score
            components = [(title_score, 0.42), (artist_score, 0.33)]
            if album_score is not None:
                components.append((album_score, 0.10))
            if duration_score is not None:
                components.append((duration_score, 0.15))

            composite_score = sum(val * weight for val, weight in components) / sum(
                weight for _, weight in components
            )

            # Check acceptance gates
            duration_acceptable = (
                diff is None
                or tolerance is None
                or diff <= tolerance
                or duration_score is not None and duration_score > 0.4
            )

            accepted = (
                composite_score >= MIN_MATCH_SCORE
                and title_score >= 0.50
                and artist_score >= 0.40
                and duration_acceptable
                and not conflicts
            )

            if accepted:
                scored_candidates.append((composite_score, cand))

        if not scored_candidates:
            return None

        # Return candidate with the highest composite score
        scored_candidates.sort(key=lambda item: item[0], reverse=True)
        return scored_candidates[0][1]

    def get_lyrics_by_isrc(
        self,
        isrc: str,
        track_hint: dict[str, Any] | None = None,
    ) -> dict[str, Any] | None:
        """
        Retrieves and normalizes lyrics using an ISRC identifier.
        Fetches metadata, tries TTML word timing first, falls back to LRC, then plain text.
        """
        isrc = isrc.strip()
        if not isrc:
            return None

        metadata = self.get_track_metadata(isrc)
        if metadata is None:
            return None

        files = metadata.get("files") or {}
        ttml_url = files.get("ttml")
        lrc_url = files.get("lrc")

        raw_ttml: str | None = None
        raw_lrc: str | None = None
        sync_type = "none"
        lines: list[dict[str, Any]] = []

        # Attempt 1: TTML for high-precision word/syllable timing
        ttml_content = self.get_ttml(isrc, ttml_url=ttml_url)
        if ttml_content:
            ttml_sync, ttml_lines = parse_ttml(ttml_content)
            if ttml_lines:
                raw_ttml = ttml_content
                sync_type = ttml_sync
                lines = ttml_lines

        # Attempt 2: Fall back to LRC if TTML is missing or produced no lines
        if not lines:
            lrc_content = self.get_lrc(isrc, lrc_url=lrc_url)
            if lrc_content:
                lrc_sync, lrc_lines = parse_lrc(lrc_content)
                if lrc_lines:
                    raw_lrc = lrc_content
                    sync_type = lrc_sync
                    lines = lrc_lines

        # If metadata declares unsynced and no timed lines could be parsed
        if not lines:
            return None

        # Generate full plain text representation
        plain_lyrics = "\n".join(
            line["text"] for line in lines if line.get("text")
        ).strip()

        # Build normalized output payload
        title = (
            metadata.get("title")
            or (extract_title(track_hint) if track_hint else "")
            or "Unknown Title"
        )
        artist = (
            metadata.get("artist")
            or (", ".join(extract_artists(track_hint)) if track_hint else "")
            or "Unknown Artist"
        )
        album = metadata.get("album") or (extract_album(track_hint) if track_hint else None)
        duration = metadata.get("duration") or (
            extract_duration_seconds(track_hint) if track_hint else None
        )
        language = metadata.get("language")
        page_url = metadata.get("page") or f"{self.base_url}/s/{isrc}"

        return build_lyrics_response(
            source=self.name,
            sync_type=sync_type,
            title=title,
            artist=artist,
            isrc=isrc,
            lines=lines,
            album=album,
            duration_seconds=duration,
            language=language,
            plain_lyrics=plain_lyrics,
            raw_ttml=raw_ttml,
            raw_lrc=raw_lrc,
            attribution=self.attribution,
            url=page_url,
        )

    def get_lyrics(self, track: dict[str, Any]) -> dict[str, Any] | None:
        """
        Primary entry point: accepts a track dictionary and returns normalized lyrics.
        1. If track dict has ISRC, attempts direct lookup.
        2. Otherwise searches lrc.red using title and artist.
        3. Matches and ranks candidate hits.
        4. Retrieves metadata and lyrics (TTML -> LRC -> plain).
        """
        # Direct ISRC check
        isrc = extract_isrc(track)
        if isrc:
            lyrics = self.get_lyrics_by_isrc(isrc, track_hint=track)
            if lyrics is not None:
                return lyrics

        # Search via title and artist
        title = extract_title(track)
        if not title:
            return None

        artists = extract_artists(track)
        primary_artist = artists[0] if artists else ""
        query = f"{title} {primary_artist}".strip()

        candidates = self.search(query)
        if not candidates and primary_artist:
            # Fallback search with title only
            candidates = self.search(title)

        if not candidates:
            return None

        # Candidate matching and ranking
        best_candidate = self.match_track(track, candidates)
        if not best_candidate or not best_candidate.get("isrc"):
            return None

        # Retrieve lyrics using matched candidate ISRC
        return self.get_lyrics_by_isrc(
            str(best_candidate["isrc"]),
            track_hint=track,
        )

