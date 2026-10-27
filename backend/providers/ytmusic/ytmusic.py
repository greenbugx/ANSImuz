from ytmusicapi import YTMusic

from artwork.thumbnails import prewarm_artwork_cache


def search(query):
    try:
        yt = YTMusic()
        search_results = yt.search(query, filter="songs")
    except Exception as e:
        raise RuntimeError(f"YT Music search failed: {e}") from e

    tracks = []
    for item in search_results[:10]:
        artist = "Unknown Artist"
        if "artists" in item and item["artists"]:
            artist = item["artists"][0]["name"]

        video_id = item.get("videoId", "")
        art_url = f"https://img.youtube.com/vi/{video_id}/maxresdefault.jpg" if video_id else ""

        tracks.append({
            "title": item.get("title", "Unknown Track"),
            "artist": artist,
            "stream_url": f"https://youtube.com/watch?v={video_id}",
            "art_url": art_url,
            "source": "youtube",
        })

    prewarm_artwork_cache(tracks)
    return tracks
