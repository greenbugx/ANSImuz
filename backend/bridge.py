from __future__ import annotations
from typing import Any
from providers import spotify, ytmusic

TrackDict = dict[str, Any]

def search_music(query: str, provider: str = "ytmusic") -> list[TrackDict]:
    """
    Main bridge function invoked directly by Rust via PyO3.
    """
    if provider == "spotify":
        return spotify.search(query)
    return ytmusic.search(query)


def resolve_playable(track: TrackDict) -> TrackDict:
    """
    Resolves lazy playback URL for Spotify tracks.
    """
    if track.get("stream_url"):
        return track
    return spotify.resolve_playable(track)
