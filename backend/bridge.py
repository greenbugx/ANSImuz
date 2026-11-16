from __future__ import annotations
from typing import Any

from exceptions import (
    AuthenticationError,
    NetworkError,
    ProviderError,
    RateLimitError,
    TrackNotFoundError,
    classify_error,
)
from providers import spotify, ytmusic
from providers.lyrics import get_lyrics as _get_lyrics

TrackDict = dict[str, Any]

__all__ = [
    "AuthenticationError",
    "NetworkError",
    "ProviderError",
    "RateLimitError",
    "TrackNotFoundError",
    "fetch_lyrics",
    "get_lyrics",
    "resolve_playable",
    "search_music",
]


def search_music(query: str, provider: str = "ytmusic") -> list[TrackDict]:
    """
    Main bridge function invoked directly by Rust via PyO3.
    """
    try:
        if provider == "spotify":
            return spotify.search(query)
        return ytmusic.search(query)
    except ProviderError:
        raise
    except Exception as exc:
        raise classify_error(f"{provider.capitalize()} search failed", exc) from exc


def resolve_playable(track: TrackDict) -> TrackDict:
    """
    Resolves lazy playback URL for Spotify tracks.
    """
    if track.get("stream_url"):
        return track
    try:
        return spotify.resolve_playable(track)
    except ProviderError:
        raise
    except Exception as exc:
        raise classify_error("Track resolution failed", exc) from exc


def fetch_lyrics(track: TrackDict) -> dict[str, Any] | None:
    """
    Fetches normalized lyrics for the given track dictionary via lyrics providers.
    Invoked directly by Rust via PyO3.
    """
    try:
        return _get_lyrics(track)
    except ProviderError:
        raise
    except Exception as exc:
        raise classify_error("Lyrics retrieval failed", exc) from exc


# Maintain get_lyrics as an alias for backwards compatibility
get_lyrics = fetch_lyrics
