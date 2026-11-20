from __future__ import annotations

import re
import unicodedata
import xml.etree.ElementTree as ET
from typing import Any


def normalize_text(text: str) -> str:
    """Normalizes text for fuzzy matching by removing accents, punctuation, and markers."""
    text = unicodedata.normalize("NFKD", text or "")
    text = "".join(char for char in text if not unicodedata.combining(char))
    text = text.lower()
    text = re.sub(
        r"\b(official(?:\s+music)?\s+video|official\s+audio|lyrics?\s+video|lyrics|visualizer|topic|hd|uhd|4k)\b",
        " ",
        text,
    )
    return re.sub(r"[^a-z0-9]+", " ", text).strip()


def clean_title_for_search(title: str) -> str:
    """
    Cleans metadata, audio/video tags, and version suffixes from a title string
    to improve lyrics search hit rates.
    """
    if not title:
        return ""
    # Strip parenthesized or bracketed video/metadata tags
    cleaned = re.sub(
        r"\s*[\(\[](?:(?:official\s*)?(?:music\s*video|lyric\s*video|video|audio|visualizer)|lyrics?|hd|uhd|4k|remastered|explicit)[\)\]]",
        "",
        title,
        flags=re.IGNORECASE,
    )
    # Strip trailing dash markers like " - Official Video"
    cleaned = re.sub(
        r"\s*-\s*(?:(?:official\s*)?(?:music\s*video|lyric\s*video|video|audio|visualizer)|lyrics?|hd|uhd|4k|remastered)\s*$",
        "",
        cleaned,
        flags=re.IGNORECASE,
    )
    return cleaned.strip()


def extract_title(track: dict[str, Any]) -> str:
    """Extracts track title from various dict formats."""
    return str(track.get("title") or track.get("name") or "").strip()


def extract_artists(track: dict[str, Any]) -> list[str]:
    """Extracts list of artist names from track dict."""
    artists = track.get("artists")
    if isinstance(artists, list):
        names = []
        for item in artists:
            if isinstance(item, str) and item.strip():
                names.append(item.strip())
            elif isinstance(item, dict) and item.get("name"):
                names.append(str(item["name"]).strip())
        if names:
            return names

    artist_str = track.get("artist") or track.get("author")
    if isinstance(artist_str, str) and artist_str.strip():
        return [a.strip() for a in artist_str.split(",") if a.strip()]

    return []


def extract_album(track: dict[str, Any]) -> str:
    """Extracts album name from track dict."""
    album = track.get("album")
    if isinstance(album, str):
        return album.strip()
    if isinstance(album, dict):
        return str(album.get("name") or "").strip()
    return ""


def extract_duration_seconds(track: dict[str, Any]) -> float | None:
    """Extracts duration in seconds as a float."""
    dur_sec = track.get("duration_seconds")
    if isinstance(dur_sec, (int, float)) and dur_sec > 0:
        return float(dur_sec)

    dur_ms = track.get("duration_ms")
    if isinstance(dur_ms, (int, float)) and dur_ms > 0:
        return float(dur_ms) / 1000.0

    dur = track.get("duration")
    if isinstance(dur, (int, float)) and dur > 0:
        return float(dur)
    if isinstance(dur, str) and dur.strip():
        try:
            parts = [float(p) for p in dur.split(":")]
            total = 0.0
            for p in parts:
                total = total * 60.0 + p
            return total if total > 0 else None
        except (ValueError, TypeError):
            return None

    return None


def extract_isrc(track: dict[str, Any]) -> str | None:
    """Extracts ISRC from track dict if available."""
    isrc = track.get("isrc")
    if isinstance(isrc, str) and isrc.strip():
        return isrc.strip()

    ext_ids = track.get("external_ids")
    if isinstance(ext_ids, dict):
        ext_isrc = ext_ids.get("isrc")
        if isinstance(ext_isrc, str) and ext_isrc.strip():
            return ext_isrc.strip()

    return None


def get_xml_attr(elem: ET.Element, name: str) -> str | None:
    """Retrieves an XML element attribute matching the local name, ignoring namespaces."""
    if name in elem.attrib:
        return elem.attrib[name]
    for k, v in elem.attrib.items():
        if k.split("}")[-1] == name:
            return v
    return None


def build_lyrics_response(
    source: str,
    sync_type: str,
    lines: list[dict[str, Any]],
    title: str = "",
    artist: str = "",
    album: str = "",
    isrc: str | None = None,
    duration_seconds: float | None = None,
    language: str | None = None,
    plain_lyrics: str | None = None,
    **extra: Any,
) -> dict[str, Any]:
    """
    Constructs a canonical, normalized lyrics response dictionary across all providers.
    Automatically generates plain_lyrics from line texts if not explicitly provided.
    """
    if plain_lyrics is None and lines:
        plain_lyrics = "\n".join(l.get("text", "") for l in lines)

    payload: dict[str, Any] = {
        "source": source,
        "sync_type": sync_type,
        "title": title,
        "artist": artist,
        "isrc": isrc,
        "lines": lines,
        "album": album,
        "duration_seconds": duration_seconds,
        "language": language,
        "plain_lyrics": plain_lyrics,
    }
    payload.update(extra)
    return payload



