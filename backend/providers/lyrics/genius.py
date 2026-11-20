from __future__ import annotations

import logging
import re
import urllib.parse
from html.parser import HTMLParser
from typing import Any

import requests

from providers.lyrics.base import (
    DEFAULT_TIMEOUT,
    LyricsProvider,
    build_lyrics_response,
    extract_album,
    extract_artists,
    extract_duration_seconds,
    extract_isrc,
    extract_title,
)

logger = logging.getLogger(__name__)

DEFAULT_GENIUS_ATTRIBUTION = "Lyrics provided by Genius"
# Non-browser User-Agent so Cloudflare does not challenge browser TLS fingerprints
DEFAULT_GENIUS_USER_AGENT = "ANSImuz"

TITLE_SEPARATOR = re.compile(r"\s*[-–—:]\s*")
DECORATIVE_CHARS = re.compile(r"[♪♫★☆【】《》「」~_]")
PRODUCER_TAGS = re.compile(r"(?i)\b(?:prod(?:uced)?\.?(?:\s+by)?)\s+.*$")
NOISE = re.compile(
    r"\s*[(\[]\s*(?:from|feat\.?|ft\.?|featuring|with|prod\.?|produced by|official|lyrical|video|audio|remix|music video|visualizer|mv|hd|4k|hq|full song)[^)\]]*[)\]]|"
    r"\s*\b(?:official\s+(?:music\s+)?(?:video|audio)|lyrical(?:\s+video)?|full\s+song|4k\s+video|hd\s+video|music\s+video)\b",
    re.IGNORECASE,
)
BRACKETED_CONTENT = re.compile(r"\s*[\(\[].*?[\)\]]")
NON_ALPHANUMERIC = re.compile(r"[^\w\s]", re.UNICODE)
YOU_MIGHT_ALSO_LIKE = re.compile(r"\d*You might also like", re.IGNORECASE)
TRAILING_EMBED = re.compile(r"\d*Embed\s*$", re.IGNORECASE)
UNICODE_SPACES = re.compile(r"[\u00a0\u2000-\u200b\u202f\u205f\u3000\ufeff]")


def clean_query(text: str) -> str:
    """Cleans decorative noise, video/producer tags and artifacts from title/artist query."""
    cleaned = DECORATIVE_CHARS.sub(" ", text)
    cleaned = NOISE.sub(" ", cleaned)
    cleaned = PRODUCER_TAGS.sub(" ", cleaned)
    if " | " in cleaned:
        cleaned = cleaned.split(" | ", 1)[0]
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    return cleaned if cleaned else text.strip()


def strip_artifacts(raw: str) -> str:
    """Strips Genius scraper artifacts, zero-width/unicode spaces, recommendations, and Embed markers."""
    cleaned = UNICODE_SPACES.sub(" ", raw)
    cleaned = YOU_MIGHT_ALSO_LIKE.sub("", cleaned).strip()
    cleaned = TRAILING_EMBED.sub("", cleaned).strip()
    return cleaned


def is_section_header(text: str) -> bool:
    """Checks if a line is a section header like [Verse 1], [Chorus], [Bridge], [Intro]."""
    s = text.strip()
    return s.startswith("[") and s.endswith("]") and bool(s[1:-1].strip())


def text_to_lyric_lines(text: str) -> list[dict[str, Any]]:
    """
    Converts clean multi-line lyrics text into structured lyric line dictionaries,
    retaining section headers and stanza separation gaps while discarding duplicate empty lines.
    """
    lines = text.splitlines()
    result: list[dict[str, Any]] = []
    last_was_gap = False

    for raw_line in lines:
        line = TRAILING_EMBED.sub("", raw_line.strip()).strip()
        if not line:
            if not last_was_gap and len(result) > 0:
                result.append({
                    "text": "",
                    "start_ms": None,
                    "end_ms": None,
                    "words": [],
                    "is_background": False,
                    "singer": 0,
                })
                last_was_gap = True
        else:
            sec_header = is_section_header(line)
            is_bg = not sec_header and (
                (line.startswith("(") and line.endswith(")"))
                or (line.startswith("（") and line.endswith("）"))
            )
            words = [
                {"text": tok, "start_ms": None, "end_ms": None, "syllables": []}
                for tok in line.split()
            ]
            result.append({
                "text": line,
                "start_ms": None,
                "end_ms": None,
                "words": words,
                "is_background": is_bg,
                "singer": 0,
            })
            last_was_gap = False

    # Drop leading and trailing gaps
    while result and not result[0]["text"]:
        result.pop(0)
    while result and not result[-1]["text"]:
        result.pop()

    return result


class GeniusLyricsParser(HTMLParser):
    """HTML parser to extract lyrics from Genius data-lyrics-container and div.lyrics tags."""

    def __init__(self) -> None:
        super().__init__()
        self.depth = 0
        self.container_levels: list[int] = []
        self.exclude_level: int | None = None
        self.text_parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.depth += 1
        attr_dict = {k: v or "" for k, v in attrs}

        is_container = (
            (tag == "div" and attr_dict.get("data-lyrics-container") == "true")
            or (tag == "div" and "lyrics" in attr_dict.get("class", "").split())
        )
        if is_container:
            self.container_levels.append(self.depth)

        if self.container_levels:
            cls_list = attr_dict.get("class", "").split()
            is_excluded = (
                tag in ("script", "style", "button")
                or attr_dict.get("data-exclude-from-selection") == "true"
                or any(
                    "LyricsHeader" in c or "SongBio" in c or "InreadAd" in c
                    for c in cls_list
                )
            )
            if is_excluded and self.exclude_level is None:
                self.exclude_level = self.depth
            elif self.exclude_level is None and tag in ("br", "p"):
                self.text_parts.append("\n")

    def handle_endtag(self, tag: str) -> None:
        if self.container_levels:
            if self.exclude_level is None and tag in ("p", "div"):
                self.text_parts.append("\n")

        if self.exclude_level is not None and self.depth <= self.exclude_level:
            self.exclude_level = None

        if self.container_levels and self.depth == self.container_levels[-1]:
            self.container_levels.pop()
            self.text_parts.append("\n")

        self.depth -= 1

    def handle_data(self, data: str) -> None:
        if self.container_levels and self.exclude_level is None:
            self.text_parts.append(data)


def parse_genius_html(html_text: str) -> list[dict[str, Any]] | None:
    """Parses Genius HTML, extracts and cleans lyrics, returning structured lyric lines."""
    if not html_text or not html_text.strip():
        return None
    try:
        parser = GeniusLyricsParser()
        parser.feed(html_text)
        raw = "".join(parser.text_parts)
        if not raw or not raw.strip():
            return None
        cleaned = strip_artifacts(raw)
        lines = text_to_lyric_lines(cleaned)
        return lines if lines else None
    except Exception as exc:
        logger.debug("Failed to parse Genius HTML: %s", exc)
        return None


def best_match(
    candidates: list[dict[str, Any]], target_title: str, target_artist: str
) -> dict[str, Any] | None:
    """
    Ranks Genius song candidates based on title and artist matches, penalizing
    translations, tracklists, album art, and review pages.
    """
    if not candidates:
        return None
    norm_title = target_title.lower()
    norm_artist = target_artist.lower()

    scored: list[tuple[dict[str, Any], int]] = []
    for item in candidates:
        title = (item.get("title") or "").lower()
        artist = (item.get("artist_names") or "").lower()
        score = 0

        title_matches = bool(norm_title) and (
            title == norm_title or norm_title in title or title in norm_title
        )
        artist_matches = bool(norm_artist) and (
            artist == norm_artist or norm_artist in artist or artist in norm_artist
        )

        if not title_matches and not artist_matches:
            continue

        if title == norm_title:
            score += 50
        elif title_matches:
            score += 25

        if artist_matches:
            if artist == norm_artist:
                score += 40
            else:
                score += 20

        # Penalize translations / reviews / tracklists unless explicitly in title
        path = (item.get("path") or "").lower()
        if "translation" in path and "translation" not in norm_title:
            score -= 30
        if any(
            term in path
            for term in (
                "türkçe",
                "turkce",
                "polskie-tlumaczenie",
                "ceviri",
                "traduction",
                "traduccion",
            )
        ):
            score -= 40
        if any(term in path for term in ("tracklist", "album-art", "review", "credits")):
            score -= 50

        if score <= 0:
            continue

        scored.append((item, score))

    if not scored:
        return None

    scored.sort(key=lambda x: x[1], reverse=True)
    return scored[0][0]


class GeniusProvider(LyricsProvider):
    """
    Web scraper provider for Genius.com lyrics.
    Used strictly as the extreme last-resort fallback when none of the time-synced
    providers (lrc.red, LyricsPlus, LRCLIB, BetterLyrics) have lyrics for a track.
    Provides unsynchronized plain lyrics with beautiful stanza and section header structure.
    """

    SEARCH_API = "https://genius.com/api"

    def __init__(
        self,
        api_url: str = SEARCH_API,
        user_agent: str = DEFAULT_GENIUS_USER_AGENT,
        attribution: str = DEFAULT_GENIUS_ATTRIBUTION,
        timeout: float = DEFAULT_TIMEOUT,
        session: requests.Session | None = None,
    ) -> None:
        self.api_url = api_url.rstrip("/")
        self.user_agent = user_agent
        self.attribution = attribution
        self.timeout = timeout
        self.session = session or requests.Session()

    @property
    def name(self) -> str:
        return "Genius"

    def _headers(self) -> dict[str, str]:
        return {
            "User-Agent": self.user_agent,
            "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,application/json,*/*;q=0.8",
            "Accept-Language": "en-US,en;q=0.9",
        }

    def _http_get(self, url: str) -> requests.Response | None:
        try:
            resp = self.session.get(url, headers=self._headers(), timeout=self.timeout)
            if resp.status_code == 200:
                return resp
            return None
        except (requests.RequestException, OSError) as exc:
            logger.debug("Genius HTTP GET failed for %s: %s", url, exc)
            return None

    def search_song_url(
        self, query: str, target_title: str, target_artist: str
    ) -> tuple[str, dict[str, Any]] | None:
        """Searches Genius open multi-search endpoint and matches the best candidate song URL."""
        url = f"{self.api_url}/search/multi?q={urllib.parse.quote(query)}"
        resp = self._http_get(url)
        if not resp:
            return None
        try:
            data = resp.json()
            sections = data.get("response", {}).get("sections", [])
            song_section = next(
                (s for s in sections if isinstance(s, dict) and s.get("type") == "song"),
                None,
            )
            if not song_section:
                return None
            hits = song_section.get("hits", [])
            candidates = [
                h.get("result")
                for h in hits
                if isinstance(h, dict) and isinstance(h.get("result"), dict)
            ]
            best = best_match(candidates, target_title, target_artist)
            if best and best.get("url"):
                return best["url"], best
        except Exception as exc:
            logger.debug("Failed parsing Genius search response: %s", exc)
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

        clean_t = clean_query(title)
        clean_a = clean_query(artist)

        title_parts = None
        if TITLE_SEPARATOR.search(clean_t):
            parts = TITLE_SEPARATOR.split(clean_t, maxsplit=1)
            if len(parts) == 2:
                title_parts = parts

        extracted_title = clean_t
        extracted_artist = clean_a

        if title_parts:
            p0, p1 = title_parts[0].strip(), title_parts[1].strip()
            if p0.lower() == clean_a.lower():
                extracted_title = p1
                extracted_artist = clean_a
            elif p1.lower() == clean_a.lower():
                extracted_title = p0
                extracted_artist = clean_a
            elif p0 and p1:
                extracted_title = p1
                if not clean_a:
                    extracted_artist = p0

        title_without_brackets = re.sub(
            r"\s+",
            " ",
            NON_ALPHANUMERIC.sub(" ", BRACKETED_CONTENT.sub(" ", extracted_title)),
        ).strip()

        search_attempts: list[tuple[str, str, str]] = []
        if extracted_artist and extracted_title:
            search_attempts.append(
                (f"{extracted_artist} {extracted_title}".strip(), extracted_title, extracted_artist)
            )
        if extracted_artist and title_without_brackets and title_without_brackets != extracted_title:
            search_attempts.append(
                (f"{extracted_artist} {title_without_brackets}".strip(), title_without_brackets, extracted_artist)
            )
        if clean_t != extracted_title:
            if clean_a and clean_t:
                search_attempts.append(
                    (f"{clean_a} {clean_t}".strip(), clean_t, clean_a)
                )
            search_attempts.append((clean_t, extracted_title, extracted_artist))
        if title_without_brackets:
            search_attempts.append(
                (title_without_brackets, title_without_brackets, extracted_artist)
            )
        elif extracted_title:
            search_attempts.append((extracted_title, extracted_title, extracted_artist))

        # Deduplicate attempts while preserving order
        seen_queries: set[str] = set()
        distinct_attempts: list[tuple[str, str, str]] = []
        for q, t, a in search_attempts:
            if q and q not in seen_queries:
                seen_queries.add(q)
                distinct_attempts.append((q, t, a))

        song_url: str | None = None
        best_candidate: dict[str, Any] | None = None

        for query, target_t, target_a in distinct_attempts:
            match_res = self.search_song_url(query, target_t, target_a)
            if match_res:
                song_url, best_candidate = match_res
                break

        if not song_url:
            return None

        html_resp = self._http_get(song_url)
        if not html_resp:
            return None

        lines = parse_genius_html(html_resp.text)
        if not lines:
            return None

        c_title = best_candidate.get("title") if best_candidate else title
        c_artist = best_candidate.get("artist_names") if best_candidate else artist

        return build_lyrics_response(
            source="Genius",
            sync_type="none",
            title=c_title or title,
            artist=c_artist or artist,
            isrc=isrc,
            lines=lines,
            album=album,
            duration_seconds=dur_sec,
            raw_ttml=None,
            raw_lrc=None,
            attribution=self.attribution,
            url=song_url,
        )