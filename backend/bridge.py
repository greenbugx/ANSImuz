from __future__ import annotations
from typing import Any

from exceptions import (
    AuthenticationError,
    BotCheckError,
    GeoBlockedError,
    JsRuntimeError,
    NetworkError,
    ProviderError,
    RateLimitError,
    ResolverOutdatedError,
    StreamUnavailableError,
    TrackNotFoundError,
    classify_error,
)
from providers import spotify, ytdlp, ytmusic
from providers.lyrics import get_lyrics as _get_lyrics

TrackDict = dict[str, Any]

__all__ = [
    "AuthenticationError",
    "BotCheckError",
    "GeoBlockedError",
    "JsRuntimeError",
    "NetworkError",
    "ProviderError",
    "RateLimitError",
    "ResolverOutdatedError",
    "StreamUnavailableError",
    "TrackNotFoundError",
    "configure_ytdlp",
    "fetch_lyrics",
    "get_lyrics",
    "resolve_playable",
    "resolve_stream",
    "search_music",
    "self_test",
    "ytdlp_status",
    "ytdlp_warmup",
]


def configure_ytdlp(
    cache_dir: str | None = None,
    data_dir: str | None = None,
    log_path: str | None = None,
) -> None:
    """Configures ANSImuz-owned paths for the embedded yt-dlp resolver."""
    ytdlp.configure(cache_dir=cache_dir, data_dir=data_dir, log_path=log_path)


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


def fetch_lyrics(track: TrackDict, is_cancelled: Any = None) -> dict[str, Any] | None:
    """
    Fetches normalized lyrics for the given track dictionary via lyrics providers.
    Invoked directly by Rust via PyO3.
    """
    try:
        return _get_lyrics(track, is_cancelled=is_cancelled)
    except ProviderError:
        raise
    except Exception as exc:
        raise classify_error("Lyrics retrieval failed", exc) from exc


# Maintain get_lyrics as an alias for backwards compatibility
get_lyrics = fetch_lyrics


def resolve_stream(url: str, background: bool = False) -> dict[str, Any]:
    """Resolves a YouTube URL to a direct playable audio stream and headers."""
    try:
        return ytdlp.resolve_stream(url, background=background)
    except (ProviderError, ValueError):
        raise
    except Exception as exc:
        raise classify_error("Stream resolution failed", exc) from exc


def ytdlp_status() -> dict[str, Any]:
    """Returns embedded yt-dlp version and active JS runtime info."""
    return ytdlp.ytdlp_status()


def ytdlp_warmup() -> dict[str, Any]:
    """Pre-initializes yt-dlp imports and interactive extractor state."""
    return ytdlp.ytdlp_warmup()


def self_test(data_dir: str | None = None) -> dict[str, Any]:
    """Runs offline deterministic self-checks for embedded yt-dlp and JS runtime."""
    return ytdlp.run_self_test(data_dir=data_dir)
