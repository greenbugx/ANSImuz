from __future__ import annotations

from providers.lyrics.parsers.lrc import parse_lrc
from providers.lyrics.parsers.lyrics_plus import parse_lyrics_plus
from providers.lyrics.parsers.lyricsfile import parse_lyricsfile
from providers.lyrics.parsers.qrc import parse_qrc
from providers.lyrics.parsers.ttml import parse_ttml

__all__ = [
    "parse_lrc",
    "parse_lyrics_plus",
    "parse_lyricsfile",
    "parse_qrc",
    "parse_ttml",
]

