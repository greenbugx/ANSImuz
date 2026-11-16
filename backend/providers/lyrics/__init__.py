from __future__ import annotations

from typing import Any

from providers.lyrics.base import (
    DEFAULT_ATTRIBUTION,
    DEFAULT_TIMEOUT,
    DEFAULT_USER_AGENT,
    LyricsProvider,
    format_timestamp,
    parse_lrc,
    parse_timestamp,
    parse_ttml,
)
from providers.lyrics.lrc_red import LrcRedProvider

_default_provider: LrcRedProvider | None = None


def get_default_provider() -> LrcRedProvider:
    global _default_provider
    if _default_provider is None:
        _default_provider = LrcRedProvider()
    return _default_provider


def get_lyrics(track: dict[str, Any]) -> dict[str, Any] | None:
    """
    Convenience function that fetches normalized lyrics for a track dictionary
    using the default lrc.red provider.
    """
    return get_default_provider().get_lyrics(track)


__all__ = [
    "DEFAULT_ATTRIBUTION",
    "DEFAULT_TIMEOUT",
    "DEFAULT_USER_AGENT",
    "LyricsProvider",
    "LrcRedProvider",
    "format_timestamp",
    "get_default_provider",
    "get_lyrics",
    "parse_lrc",
    "parse_timestamp",
    "parse_ttml",
]

