import io
import os
import threading

import requests
from PIL import Image

MIN_ARTWORK_SIDE = 200


def _cache_base_dir():
    return os.path.join(
        os.environ.get("XDG_CACHE_HOME", os.path.join(os.path.expanduser("~"), ".cache")),
        "ansimuz",
    )


ARTWORK_CACHE_DIR = os.path.join(_cache_base_dir(), "artwork")


def cache_path(url):
    # MUST mirror the FNV-1a 32-bit hash in Rust (artwork::cache_path)
    h = 2166136261
    for byte in url.encode("utf-8"):
        h = ((h ^ byte) * 16777619) & 0xFFFFFFFF
    return os.path.join(ARTWORK_CACHE_DIR, f"{h:08x}.img")


def _is_gray_placeholder(img):
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
