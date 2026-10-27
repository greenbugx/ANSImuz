from providers import spotify, ytmusic


def search_music(query, provider="ytmusic"):
    """
    Main bridge function invoked directly by Rust via PyO3.
    """
    if provider == "spotify":
        return spotify.search(query)
    return ytmusic.search(query)


def resolve_playable(track):
    """
    Resolves lazy playback URL for Spotify tracks.
    """
    if track.get("stream_url"):
        return track
    return spotify.resolve_playable(track)
