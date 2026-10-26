from ytmusicapi import YTMusic

def search(query):
    """
    Queries YouTube Music for matching songs.
    Returns a unified format: list of track dicts.
    """
    try:
        yt = YTMusic()
        search_results = yt.search(query, filter="songs")
    except Exception as e:
        raise RuntimeError(f"YT Music search failed: {e}") from e

    tracks = []
    # pull up to top 10 matches
    for item in search_results[:10]:
        # grab artist name
        artist = "Unknown Artist"
        if "artists" in item and item["artists"]:
            artist = item["artists"][0]["name"]

        # max resolution thumbnail endpoint, falls back to hqdefault at download time
        video_id = item.get("videoId", "")
        art_url = f"https://img.youtube.com/vi/{video_id}/maxresdefault.jpg" if video_id else ""

        tracks.append({
            "title": item.get("title", "Unknown Track"),
            "artist": artist,
            "stream_url": f"https://youtube.com/watch?v={video_id}",
            "art_url": art_url,
            "source": "youtube"
        })
    return tracks
