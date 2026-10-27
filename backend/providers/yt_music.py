import io
import os
import threading

import requests
from PIL import Image
from ytmusicapi import YTMusic

MIN_ARTWORK_SIDE = 200


def _cache_base_dir():
    return os.path.join(
        os.environ.get("XDG_CACHE_HOME", os.path.join(os.path.expanduser("~"), ".cache")),
        "ansimuz",
    )


ARTWORK_CACHE_DIR = os.path.join(_cache_base_dir(), "artwork")


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

    prewarm_artwork_cache(tracks)
    return tracks


def cache_path(url):
    # MUST mirror the FNV-1a 32-bit hash in src/art.rs (cache_path)
    h = 2166136261
    for byte in url.encode("utf-8"):
        h = ((h ^ byte) * 16777619) & 0xFFFFFFFF
    return os.path.join(ARTWORK_CACHE_DIR, f"{h:08x}.img")


def _is_gray_placeholder(img):
    """Detect YouTube's uniformly-gray placeholder thumbnails by sampling an 8x8 grid."""
    w, h = img.size
    steps = 8
    dx = max(w // steps, 1)
    dy = max(h // steps, 1)
    for sy in range(steps):
        for sx in range(steps):
            x = min(sx * dx, w - 1)
            y = min(sy * dy, h - 1)
            r, g, b = img.getpixel((x, y))[:3]
            if r != g or g != b:
                return False
    return True


def fetch_square_artwork(url):
    response = requests.get(url, timeout=8)

    if response.status_code != 200 and "maxresdefault" in url:
        response = requests.get(url.replace("maxresdefault", "hqdefault"), timeout=8)

    response.raise_for_status()

    img = Image.open(io.BytesIO(response.content))
    width, height = img.size

    if width < MIN_ARTWORK_SIDE or height < MIN_ARTWORK_SIDE:
        raise ValueError(f"Artwork too small ({width}x{height}), likely a placeholder")

    if _is_gray_placeholder(img):
        raise ValueError("Detected YouTube placeholder thumbnail")

    if width > height:
        left = (width - height) // 2
        img = img.crop((left, 0, left + height, height))

    return img


def prewarm_artwork_cache(tracks):
    def worker():
        for track in tracks:
            art_url = track.get("art_url", "")
            if not art_url:
                continue

            path = cache_path(art_url)
            if os.path.exists(path):
                continue

            try:
                img = fetch_square_artwork(art_url)
                os.makedirs(ARTWORK_CACHE_DIR, exist_ok=True)
                img.save(path, "JPEG", quality=95)
            except Exception:
                continue

    threading.Thread(target=worker, daemon=True).start()
