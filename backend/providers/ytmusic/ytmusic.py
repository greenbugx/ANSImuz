from ytmusicapi import YTMusic

from artwork.thumbnails import prewarm_artwork_cache
from exceptions import classify_error


def search(query):
    try:
        yt = YTMusic()
        search_results = yt.search(query, filter="songs")
    except Exception as e:
        raise classify_error("YouTube Music search failed", e) from e

    tracks = []
    for item in search_results[:10]:
        artists = [
            a["name"]
            for a in item.get("artists", [])
            if isinstance(a, dict) and a.get("name")
        ]
        artist = ", ".join(artists) if artists else "Unknown Artist"

        video_id = item.get("videoId") or ""
        track_id = video_id if video_id else None

        album = None
        album_val = item.get("album")
        if isinstance(album_val, dict):
            album = album_val.get("name")
        elif isinstance(album_val, str) and album_val.strip():
            album = album_val.strip()

        art_url = (
            f"https://img.youtube.com/vi/{video_id}/maxresdefault.jpg"
            if video_id
            else None
        )

        stream_url = f"https://youtube.com/watch?v={video_id}" if video_id else None

        tracks.append({
            "provider": "ytmusic",
            "track_id": track_id,
            "title": item.get("title", "Unknown Track"),
            "artist": artist,
            "album": album,
            "art_url": art_url,
            "stream_url": stream_url,
            "artists": artists,
            "source": "youtube",
        })

    prewarm_artwork_cache(tracks)
    return tracks
