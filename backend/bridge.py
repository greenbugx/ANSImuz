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

TrackDict = dict[str, Any]

__all__ = [
    "AuthenticationError",
    "NetworkError",
    "ProviderError",
    "RateLimitError",
    "TrackNotFoundError",
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
