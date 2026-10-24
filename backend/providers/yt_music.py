from ytmusicapi import YTMusic

def search(query):
    """
    Queries YouTube Music for matching songs.
    Returns a unified format: list of track dicts.
    """
    try:
        yt = YTMusic()
        search_results = yt.search(query, filter="songs")
        
        tracks = []
        # pull up to top 10 matches
        for item in search_results[:10]:
            # extract high-res artwork if available, fallback to default
            art_url = ""
            if "thumbnails" in item and item["thumbnails"]:
                art_url = item["thumbnails"][-1]["url"]

            # grab artist name
            artist = "Unknown Artist"
            if "artists" in item and item["artists"]:
                artist = item["artists"][0]["name"]

            tracks.append({
                "title": item.get("title", "Unknown Track"),
                "artist": artist,
                "stream_url": f"https://youtube.com/watch?v={item['videoId']}",
                "art_url": art_url,
                "source": "youtube"
            })
        return tracks
    except Exception as e:
        print(f"[YTMusic Backend Error]: {e}")
        return []