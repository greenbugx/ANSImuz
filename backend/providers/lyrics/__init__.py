from __future__ import annotations

import logging
from typing import Any

from providers.lyrics.base import (
    DEFAULT_ATTRIBUTION,
    DEFAULT_TIMEOUT,
    DEFAULT_USER_AGENT,
    LyricsProvider,
    clean_title_for_search,
    format_timestamp,
    parse_lrc,
    parse_lyrics_plus,
    parse_lyricsfile,
    parse_qrc,
    parse_timestamp,
    parse_ttml,
)
from providers.lyrics.better_lyrics import (
    DEFAULT_BETTER_LYRICS_ATTRIBUTION,
    BetterLyricsProvider,
)
from providers.lyrics.genius import (
    DEFAULT_GENIUS_ATTRIBUTION,
    GeniusProvider,
)
from providers.lyrics.lrclib import (
    DEFAULT_LRCLIB_ATTRIBUTION,
    LrclibProvider,
)
from providers.lyrics.lyrics_plus import (
    DEFAULT_LYRICS_PLUS_ATTRIBUTION,
    LyricsPlusProvider,
)
from providers.lyrics.lrc_red import LrcRedProvider
from providers.lyrics.megalobiz import (
    DEFAULT_MEGALOBIZ_ATTRIBUTION,
    MegalobizProvider,
)

logger = logging.getLogger(__name__)

_better_lyrics_provider: BetterLyricsProvider | None = None
_genius_provider: GeniusProvider | None = None
_lyrics_plus_provider: LyricsPlusProvider | None = None
_lrclib_provider: LrclibProvider | None = None
_lrc_red_provider: LrcRedProvider | None = None
_megalobiz_provider: MegalobizProvider | None = None


def get_better_lyrics_provider() -> BetterLyricsProvider:
    global _better_lyrics_provider
    if _better_lyrics_provider is None:
        _better_lyrics_provider = BetterLyricsProvider()
    return _better_lyrics_provider


def get_genius_provider() -> GeniusProvider:
    global _genius_provider
    if _genius_provider is None:
        _genius_provider = GeniusProvider()
    return _genius_provider


def get_lyrics_plus_provider() -> LyricsPlusProvider:
    global _lyrics_plus_provider
    if _lyrics_plus_provider is None:
        _lyrics_plus_provider = LyricsPlusProvider()
    return _lyrics_plus_provider


def get_lrclib_provider() -> LrclibProvider:
    global _lrclib_provider
    if _lrclib_provider is None:
        _lrclib_provider = LrclibProvider()
    return _lrclib_provider


def get_megalobiz_provider() -> MegalobizProvider:
    global _megalobiz_provider
    if _megalobiz_provider is None:
        _megalobiz_provider = MegalobizProvider()
    return _megalobiz_provider


def get_lrc_red_provider() -> LrcRedProvider:
    global _lrc_red_provider
    if _lrc_red_provider is None:
        _lrc_red_provider = LrcRedProvider()
    return _lrc_red_provider


# Backward compatibility alias
get_default_provider = get_lrc_red_provider


def get_lyrics(track: dict[str, Any], is_cancelled: Any = None) -> dict[str, Any] | None:
    """
    Fetches normalized lyrics for a track dictionary using a tiered provider chain:
    1. lrc.red (LRCLIB ISRC provider with line/word-level sync)
    2. LyricsPlus (Aggregated Apple Music, QQ, Musixmatch syllable timings via mirror race)
    3. LRCLIB.net (Official LRCLIB database with Lyricsfile word sync & LRC line sync)
    4. Megalobiz (Megalobiz community synchronized LRC sheets via official REST API)
    5. BetterLyrics (Apple Music TTML syllable-timed & Portato QQ Music QRC fallback)
    6. Genius (Web scraper fallback providing unsynchronized plain lyrics with stanzas)

    Optionally accepts `is_cancelled` callable to abort requests early between provider stages.
    """
    def cancelled() -> bool:
        if is_cancelled is None:
            return False
        try:
            return bool(is_cancelled())
        except Exception:
            return False

    # 1. Attempt lrc.red first
    if cancelled():
        return None
    try:
        res = get_lrc_red_provider().get_lyrics(track)
        if res and res.get("lines"):
            return res
    except Exception as exc:
        logger.debug("lrc.red retrieval failed, falling back to LyricsPlus: %s", exc)

    # 2. Attempt LyricsPlus second
    if cancelled():
        return None
    try:
        res = get_lyrics_plus_provider().get_lyrics(track)
        if res and res.get("lines"):
            return res
    except Exception as exc:
        logger.debug("LyricsPlus retrieval failed, falling back to LRCLIB: %s", exc)

    # 3. Attempt LRCLIB.net third
    if cancelled():
        return None
    try:
        res = get_lrclib_provider().get_lyrics(track)
        if res and res.get("lines"):
            return res
    except Exception as exc:
        logger.debug("LRCLIB.net retrieval failed, falling back to Megalobiz: %s", exc)

    # 4. Attempt Megalobiz fourth
    if cancelled():
        return None
    try:
        res = get_megalobiz_provider().get_lyrics(track)
        if res and res.get("lines"):
            return res
    except Exception as exc:
        logger.debug("Megalobiz retrieval failed, falling back to BetterLyrics: %s", exc)

    # 5. Attempt BetterLyrics fifth
    if cancelled():
        return None
    try:
        res = get_better_lyrics_provider().get_lyrics(track)
        if res and res.get("lines"):
            return res
    except Exception as exc:
        logger.debug("BetterLyrics retrieval failed, falling back to Genius: %s", exc)

    # 6. Fallback to Genius (last resort)
    if cancelled():
        return None
    try:
        return get_genius_provider().get_lyrics(track)
    except Exception as exc:
        logger.debug("Genius retrieval failed: %s", exc)
        return None


__all__ = [
    "DEFAULT_ATTRIBUTION",
    "DEFAULT_BETTER_LYRICS_ATTRIBUTION",
    "DEFAULT_GENIUS_ATTRIBUTION",
    "DEFAULT_LRCLIB_ATTRIBUTION",
    "DEFAULT_LYRICS_PLUS_ATTRIBUTION",
    "DEFAULT_MEGALOBIZ_ATTRIBUTION",
    "DEFAULT_TIMEOUT",
    "DEFAULT_USER_AGENT",
    "BetterLyricsProvider",
    "GeniusProvider",
    "LrclibProvider",
    "LyricsPlusProvider",
    "LyricsProvider",
    "LrcRedProvider",
    "MegalobizProvider",
    "clean_title_for_search",
    "format_timestamp",
    "get_better_lyrics_provider",
    "get_default_provider",
    "get_genius_provider",
    "get_lrc_red_provider",
    "get_lrclib_provider",
    "get_lyrics",
    "get_lyrics_plus_provider",
    "get_megalobiz_provider",
    "parse_lrc",
    "parse_lyrics_plus",
    "parse_lyricsfile",
    "parse_qrc",
    "parse_timestamp",
    "parse_ttml",
]
