import math
import re
import threading
import unicodedata
from difflib import SequenceMatcher
from typing import Any
from spotify_scraper import SpotifyClient
from ytmusicapi import YTMusic

SPOTIFY_LIMIT = 10
YTMUSIC_LIMIT = 10
MIN_MATCH_SCORE = 0.68 # higher the better, but may reduce matches

_ytmusic: YTMusic | None = None
_ytmusic_lock = threading.Lock()
_resolved_cache: dict[str, dict[str, Any]] = {}
_cache_lock = threading.Lock()


def _get_ytmusic() -> YTMusic:
    global _ytmusic
    if _ytmusic is None:
        with _ytmusic_lock:
            if _ytmusic is None:
                _ytmusic = YTMusic()
    return _ytmusic


def _field(obj: Any, name: str, default: Any = None) -> Any:
    if isinstance(obj, dict):
        return obj.get(name, default)
    return getattr(obj, name, default)

def _normalize(text: str) -> str:
    text = unicodedata.normalize("NFKD", text or "")
    text = "".join(char for char in text if not unicodedata.combining(char))
    text = text.lower()
    text = re.sub(
        r"\b(official(?:\s+music)?\s+video|official\s+audio|lyrics?\s+video|lyrics|visualizer|topic|hd|uhd|4k)\b",
        " ",
        text,
    )
    return re.sub(r"[^a-z0-9]+", " ", text).strip()

def _similarity(expected: str, actual: str) -> float:
    expected = _normalize(expected)
    actual = _normalize(actual)
    if not expected or not actual:
        return 0.0

    expected_tokens = set(expected.split())
    actual_tokens = set(actual.split())
    coverage = len(expected_tokens & actual_tokens) / len(expected_tokens)
    sequence = SequenceMatcher(None, expected, actual).ratio()
    return 0.65 * coverage + 0.35 * sequence

def _get_album_name(track: Any) -> str:
    album = _field(track, "album")
    if isinstance(album, str):
        return album
    return _field(album, "name", "") or ""

def _get_artists(track: Any) -> list[str]:
    return [
        name
        for artist in (_field(track, "artists", ()) or ())
        if (name := _field(artist, "name", ""))
    ]

def _get_artwork(track: Any) -> str:
    images = _field(track, "images", ()) or ()
    if not images:
        return ""
    image = max(
        images,
        key=lambda item: (
            (_field(item, "width", 0) or 0)
            * (_field(item, "height", 0) or 0)
        ),
    )
    return _field(image, "url", "") or ""

def _spotify_duration_seconds(track: Any) -> float | None:
    milliseconds = _field(track, "duration_ms")
    if isinstance(milliseconds, (int, float)) and milliseconds > 0:
        return milliseconds / 1000.0
    return None

def _youtube_duration_seconds(song: dict[str, Any]) -> float | None:
    seconds = song.get("duration_seconds")
    if isinstance(seconds, (int, float)) and seconds > 0:
        return float(seconds)

    duration = song.get("duration")
    if isinstance(duration, str) and duration:
        try:
            result = 0
            for part in duration.split(":"):
                result = result * 60 + int(part)
            return float(result) if result > 0 else None
        except ValueError:
            return None
    return None

def search(query: str) -> list[dict[str, Any]]:
    """Search Spotify and return metadata without performing YouTube lookup."""
    if not query or not query.strip():
        return []

    try:
        with SpotifyClient() as spotify:
            response = spotify.search(
                query.strip(),
                types=("track",),
                limit=SPOTIFY_LIMIT,
            )
    except Exception as exc:
        raise RuntimeError(f"Spotify search failed: {exc}") from exc

    tracks = []
    for item in list(response.tracks or ())[:SPOTIFY_LIMIT]:
        artists = _get_artists(item)
        track_id = _field(item, "id", "") or ""
        track_url = _field(item, "url", "") or (
            f"https://open.spotify.com/track/{track_id}" if track_id else ""
        )
        duration_ms = _field(item, "duration_ms", 0) or 0

        tracks.append({
            "title": _field(item, "name", "") or "",
            "artist": ", ".join(artists) or "Unknown Artist",
            "artists": artists,
            "album": _get_album_name(item),
            "duration_ms": duration_ms,
            "duration_seconds": _spotify_duration_seconds(item),
            "spotify_id": track_id,
            "spotify_url": track_url,
            "stream_url": "",
            "youtube_id": "",
            "art_url": _get_artwork(item),
            "source": "spotify",
            "resolved": False,
        })

    return tracks

def _build_search_queries(title: str, artists: list[str], album: str) -> list[str]:
    primary_artist = artists[0] if artists else ""
    primary = f"{title} {primary_artist}".strip()
    queries = [primary]

    if album and _normalize(album) != _normalize(title):
        with_album = f"{title} {primary_artist} {album}".strip()
        if _normalize(with_album) != _normalize(primary):
            queries.append(with_album)

    queries.append(title.strip())
    return list(dict.fromkeys(query for query in queries if query))

def _version_conflicts(source_title: str, candidate_title: str) -> list[str]:
    source = _normalize(source_title).split()
    candidate = _normalize(candidate_title).split()
    markers = [
        ("live",), ("remix",), ("cover",), ("karaoke",),
        ("instrumental",), ("nightcore",), ("slowed",),
        ("sped", "up"), ("acoustic",),
    ]
    conflicts = []
    for marker in markers:
        size = len(marker)
        candidate_has = any(
            tuple(candidate[index:index + size]) == marker
            for index in range(len(candidate) - size + 1)
        )
        source_has = any(
            tuple(source[index:index + size]) == marker
            for index in range(len(source) - size + 1)
        )
        if candidate_has and not source_has:
            conflicts.append(" ".join(marker))
    return conflicts

def _rank_candidate(track: dict[str, Any], song: dict[str, Any]) -> dict[str, Any]:
    title_score = _similarity(track["title"], song.get("title", ""))

    youtube_artists = [
        artist.get("name", "")
        for artist in (song.get("artists") or [])
        if artist.get("name")
    ]
    artist_fields = youtube_artists + [
        song.get("title", ""),
        song.get("author", ""),
        song.get("artist", ""),
    ]
    artist_score = max(
        (
            _similarity(expected, actual)
            for expected in (track.get("artists") or [])
            for actual in artist_fields
            if actual
        ),
        default=0.0,
    )

    youtube_album_data = song.get("album") or {}
    youtube_album = (
        youtube_album_data if isinstance(youtube_album_data, str)
        else youtube_album_data.get("name", "")
    )
    spotify_album = track.get("album", "")
    album_score = (
        _similarity(spotify_album, youtube_album)
        if spotify_album and youtube_album else None
    )

    expected_duration = track.get("duration_seconds")
    candidate_duration = _youtube_duration_seconds(song)
    difference = (
        abs(expected_duration - candidate_duration)
        if expected_duration is not None and candidate_duration is not None
        else None
    )
    tolerance = max(15.0, expected_duration * 0.08) if expected_duration else None
    duration_score = (
        math.exp(-difference / max(5.0, expected_duration * 0.025))
        if difference is not None and expected_duration else None
    )

    components = [(title_score, 0.42), (artist_score, 0.33)]
    if album_score is not None:
        components.append((album_score, 0.10))
    if duration_score is not None:
        components.append((duration_score, 0.15))
    score = sum(value * weight for value, weight in components) / sum(
        weight for _, weight in components
    )

    conflicts = _version_conflicts(track["title"], song.get("title", ""))
    accepted = (
        bool(song.get("videoId"))
        and title_score >= 0.60
        and artist_score >= 0.55
        and score >= MIN_MATCH_SCORE
        and difference is not None
        and tolerance is not None
        and difference <= tolerance
        and not conflicts
    )

    return {
        "video_id": song.get("videoId", ""),
        "title": song.get("title", ""),
        "artists": youtube_artists,
        "album": youtube_album,
        "duration": candidate_duration,
        "duration_difference": difference,
        "tolerance": tolerance,
        "title_score": title_score,
        "artist_score": artist_score,
        "album_score": album_score,
        "duration_score": duration_score,
        "score": score,
        "conflicts": conflicts,
        "accepted": accepted,
    }

def resolve_playable(track: dict[str, Any]) -> dict[str, Any]:
    """Resolve one Spotify result to a YouTube Music URL on demand."""
    track_id = track.get("spotify_id")
    if not track_id:
        raise ValueError("Track is missing spotify_id")

    with _cache_lock:
        cached = _resolved_cache.get(track_id)
    if cached:
        return {**track, **cached}

    if not track.get("title"):
        raise ValueError("Track is missing title")

    artists = track.get("artists") or [track.get("artist", "")]
    artists = [artist for artist in artists if artist]
    if not artists:
        raise ValueError("Track is missing artist metadata")

    normalized_track = {
        **track,
        "duration_seconds": track.get("duration_seconds"),
    }
    if normalized_track["duration_seconds"] is None:
        duration_ms = track.get("duration_ms")
        if isinstance(duration_ms, (int, float)) and duration_ms > 0:
            normalized_track["duration_seconds"] = duration_ms / 1000.0

    queries = _build_search_queries(
        normalized_track["title"],
        artists,
        normalized_track.get("album", ""),
    )
    found_by_id: dict[str, dict[str, Any]] = {}
    ranked: list[dict[str, Any]] = []
    winner = None
    errors = []

    for query in queries:
        try:
            yt = _get_ytmusic()
            with _ytmusic_lock:
                candidates = yt.search(
                    query,
                    filter="songs",
                    limit=YTMUSIC_LIMIT,
                )
        except Exception as exc:
            errors.append(f"{query!r}: {exc}")
            continue

        for song in candidates:
            video_id = song.get("videoId")
            if video_id:
                found_by_id[video_id] = song

        ranked = sorted(
            (
                _rank_candidate(normalized_track, song)
                for song in found_by_id.values()
            ),
            key=lambda result: result["score"],
            reverse=True,
        )
        winner = next((candidate for candidate in ranked if candidate["accepted"]), None)
        if winner:
            break

    if not winner:
        rejection_details = []
        if ranked:
            best = ranked[0]
            if best.get("conflicts"):
                rejection_details.append(
                    f"version conflict with '{best['title']}' ({', '.join(best['conflicts'])})"
                )
            elif (
                best.get("duration_difference") is not None
                and best.get("tolerance") is not None
                and best["duration_difference"] > best["tolerance"]
            ):
                rejection_details.append(
                    f"duration difference {best['duration_difference']:.1f}s exceeded tolerance "
                    f"{best['tolerance']:.1f}s on '{best['title']}'"
                )
            elif best.get("score") is not None and best["score"] < MIN_MATCH_SCORE:
                rejection_details.append(
                    f"match score {best['score']:.2f} was below threshold {MIN_MATCH_SCORE} on '{best['title']}'"
                )
            elif best.get("title_score") is not None and best["title_score"] < 0.60:
                rejection_details.append(
                    f"low title similarity ({best['title_score']:.2f}) on '{best['title']}'"
                )
            elif best.get("artist_score") is not None and best["artist_score"] < 0.55:
                rejection_details.append(
                    f"low artist similarity ({best['artist_score']:.2f}) on '{best['title']}'"
                )
        elif not found_by_id:
            rejection_details.append("no candidate songs returned by YouTube Music")

        reason_str = f" Reason: {'; '.join(rejection_details)}." if rejection_details else ""
        error_str = f" Search errors: {'; '.join(errors)}." if errors else ""

        raise LookupError(
            f"No confident YouTube Music match for "
            f"'{track['title']}' by {track.get('artist', 'Unknown Artist')}.{reason_str}{error_str}"
        )

    selected_song = found_by_id[winner["video_id"]]
    thumbnails = selected_song.get("thumbnails") or []
    art_url = track.get("art_url", "")
    if not art_url and thumbnails:
        best_thumbnail = max(
            thumbnails,
            key=lambda image: (image.get("width") or 0) * (image.get("height") or 0),
        )
        art_url = best_thumbnail.get("url", "")

    # end result
    resolved = {
        "stream_url": f"https://music.youtube.com/watch?v={winner['video_id']}",
        "youtube_id": winner["video_id"],
        "youtube_title": winner["title"],
        "youtube_artist": ", ".join(winner["artists"]),
        "match_score": round(winner["score"], 3),
        "art_url": art_url,
        "resolved": True,
    }

    with _cache_lock:
        _resolved_cache[track_id] = resolved

    return {**track, **resolved}
