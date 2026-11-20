from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any

from providers.lyrics.matching import compute_similarity
from providers.lyrics.normalization import (
    clean_title_for_search,
    extract_album,
    extract_artists,
    extract_duration_seconds,
    extract_isrc,
    extract_title,
    get_xml_attr,
    normalize_text,
)
from providers.lyrics.parsers import (
    parse_lrc,
    parse_lyrics_plus,
    parse_lyricsfile,
    parse_qrc,
    parse_ttml,
)
from providers.lyrics.timestamps import (
    format_timestamp,
    parse_timestamp,
)

# Default API User-Agent identifying the app and contact according to API TOS
DEFAULT_USER_AGENT = (
    "ANSImuz/0.1.3 (+https://github.com/greenbugx/ANSImuz; contact: greenbugx@proton.me)"
)
DEFAULT_ATTRIBUTION = "Lyrics provided by lrc.red"
DEFAULT_TIMEOUT = 15.0


class LyricsProvider(ABC):
    """Abstract base class for all lyrics providers."""

    @property
    @abstractmethod
    def name(self) -> str:
        """Unique provider identifier."""
        ...

    @abstractmethod
    def get_lyrics(self, track: dict[str, Any]) -> dict[str, Any] | None:
        """
        Accepts a track dictionary and returns its lyrics in a normalized format,
        or None if no lyrics could be found.
        """
        ...


__all__ = [
    "DEFAULT_ATTRIBUTION",
    "DEFAULT_TIMEOUT",
    "DEFAULT_USER_AGENT",
    "LyricsProvider",
    "clean_title_for_search",
    "compute_similarity",
    "extract_album",
    "extract_artists",
    "extract_duration_seconds",
    "extract_isrc",
    "extract_title",
    "format_timestamp",
    "get_xml_attr",
    "normalize_text",
    "parse_lrc",
    "parse_lyrics_plus",
    "parse_lyricsfile",
    "parse_qrc",
    "parse_timestamp",
    "parse_ttml",
]
