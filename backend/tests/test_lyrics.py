from __future__ import annotations

import os
import sys
import unittest
from unittest.mock import MagicMock, patch

# Ensure backend directory is in sys.path
BACKEND_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
if BACKEND_DIR not in sys.path:
    sys.path.insert(0, BACKEND_DIR)

from exceptions import NetworkError, ProviderError, RateLimitError
from providers.lyrics.base import (
    DEFAULT_ATTRIBUTION,
    DEFAULT_TIMEOUT,
    DEFAULT_USER_AGENT,
    LyricsProvider,
    compute_similarity,
    extract_album,
    extract_artists,
    extract_duration_seconds,
    extract_isrc,
    extract_title,
    format_timestamp,
    get_xml_attr,
    normalize_text,
    parse_lrc,
    parse_timestamp,
    parse_ttml,
)
from providers.lyrics.lrc_red import LrcRedProvider, _version_conflicts


class TestLyricsProviderInterface(unittest.TestCase):
    def test_cannot_instantiate_abstract_base_directly(self):
        with self.assertRaises(TypeError):
            LyricsProvider()  # type: ignore

    def test_concrete_subclass_implements_interface(self):
        class DummyProvider(LyricsProvider):
            @property
            def name(self) -> str:
                return "dummy"

            def get_lyrics(self, track: dict) -> dict | None:
                return {
                    "source": self.name,
                    "title": track.get("title"),
                    "artist": track.get("artist"),
                    "lines": [],
                }

        provider = DummyProvider()
        self.assertEqual(provider.name, "dummy")
        res = provider.get_lyrics({"title": "Test Song", "artist": "Test Artist"})
        self.assertEqual(res["source"], "dummy")
        self.assertEqual(res["title"], "Test Song")


class TestTimestampAndFormattingHelpers(unittest.TestCase):
    def test_parse_timestamp_numeric(self):
        self.assertEqual(parse_timestamp(0), 0)
        self.assertEqual(parse_timestamp(12.34), 12340)
        self.assertEqual(parse_timestamp(238.64), 238640)
        self.assertIsNone(parse_timestamp(None))

    def test_parse_timestamp_seconds_string(self):
        self.assertEqual(parse_timestamp("0.000"), 0)
        self.assertEqual(parse_timestamp("19.764"), 19764)
        self.assertEqual(parse_timestamp("19.764s"), 19764)
        self.assertEqual(parse_timestamp("19764ms"), 19764)
        self.assertEqual(parse_timestamp("  2.906 "), 2906)

    def test_parse_timestamp_minutes_seconds(self):
        self.assertEqual(parse_timestamp("00:19.76"), 19760)
        self.assertEqual(parse_timestamp("01:23.45"), 83450)
        self.assertEqual(parse_timestamp("03:45"), 225000)

    def test_parse_timestamp_hours_minutes_seconds(self):
        self.assertEqual(parse_timestamp("01:00:00"), 3600000)
        self.assertEqual(parse_timestamp("00:01:23.456"), 83456)

    def test_parse_timestamp_frames_format(self):
        # 00:01:00:15 at 30fps is 60.5s -> 60500ms
        self.assertEqual(parse_timestamp("00:01:00:15"), 60500)

    def test_parse_timestamp_invalid(self):
        self.assertIsNone(parse_timestamp(""))
        self.assertIsNone(parse_timestamp("invalid:time:format:extra:colon"))
        self.assertIsNone(parse_timestamp("abc"))

    def test_format_timestamp(self):
        self.assertEqual(format_timestamp(0), "00:00.00")
        self.assertEqual(format_timestamp(83450), "01:23.45")
        self.assertEqual(format_timestamp(None), "--:--")
        self.assertEqual(format_timestamp(19764), "00:19.76")


class TestTrackExtractionHelpers(unittest.TestCase):
    def test_extract_title(self):
        self.assertEqual(extract_title({"title": "Creep"}), "Creep")
        self.assertEqual(extract_title({"name": "Creep"}), "Creep")
        self.assertEqual(extract_title({}), "")

    def test_extract_artists(self):
        self.assertEqual(
            extract_artists({"artists": ["Radiohead", "Thom Yorke"]}),
            ["Radiohead", "Thom Yorke"],
        )
        self.assertEqual(
            extract_artists({"artists": [{"name": "Radiohead"}, {"name": "Thom Yorke"}]}),
            ["Radiohead", "Thom Yorke"],
        )
        self.assertEqual(
            extract_artists({"artist": "Radiohead, Thom Yorke"}),
            ["Radiohead", "Thom Yorke"],
        )
        self.assertEqual(extract_artists({"artist": "Queen"}), ["Queen"])
        self.assertEqual(extract_artists({}), [])

    def test_extract_album(self):
        self.assertEqual(extract_album({"album": "Pablo Honey"}), "Pablo Honey")
        self.assertEqual(
            extract_album({"album": {"name": "Pablo Honey"}}), "Pablo Honey"
        )
        self.assertEqual(extract_album({}), "")

    def test_extract_duration_seconds(self):
        self.assertEqual(extract_duration_seconds({"duration_seconds": 238.64}), 238.64)
        self.assertEqual(extract_duration_seconds({"duration_ms": 238640}), 238.64)
        self.assertEqual(extract_duration_seconds({"duration": 238.64}), 238.64)
        self.assertEqual(extract_duration_seconds({"duration": "03:58"}), 238.0)
        self.assertIsNone(extract_duration_seconds({}))

    def test_extract_isrc(self):
        self.assertEqual(extract_isrc({"isrc": "GBAYE9200070"}), "GBAYE9200070")
        self.assertEqual(
            extract_isrc({"external_ids": {"isrc": "GBAYE9200070"}}),
            "GBAYE9200070",
        )
        self.assertIsNone(extract_isrc({}))


class TestTTMLParser(unittest.TestCase):
    def test_parse_empty_or_invalid_ttml(self):
        sync, lines = parse_ttml("")
        self.assertEqual(sync, "none")
        self.assertEqual(lines, [])

        sync, lines = parse_ttml("<invalid-xml>")
        self.assertEqual(sync, "none")
        self.assertEqual(lines, [])

    def test_parse_ttml_with_word_and_syllable_sync(self):
        # TTML with XML namespaces and syllable-level timing in line 2
        ttml = """
        <tt xmlns="http://www.w3.org/ns/ttml" xmlns:lrc="http://lrc.red/lyric-ttml-internal">
            <body>
                <div lrc:songPart="Verse">
                    <p begin="0.000s" end="2.906s">
                        <span begin="0.000" end="0.458">Is</span>
                        <span begin="0.458" end="0.806">this</span>
                        <span begin="0.806" end="1.224">the</span>
                        <span begin="1.224" end="2.057">real</span>
                        <span begin="2.057" end="2.906">life?</span>
                    </p>
                    <p begin="3.725s" end="6.373s">
                        <span begin="3.725" end="4.140">Is</span>
                        <span begin="4.140" end="4.526">this</span>
                        <span begin="4.526" end="4.891">just</span>
                        <span begin="4.891" end="5.274">fan</span><span begin="5.274" end="5.527">ta</span><span begin="5.527" end="6.373">sy?</span>
                    </p>
                </div>
            </body>
        </tt>
        """
        sync, lines = parse_ttml(ttml)
        self.assertEqual(sync, "syllable")
        self.assertEqual(len(lines), 2)

        # Line 0: word-level timing without sub-syllables
        self.assertEqual(lines[0]["start_ms"], 0)
        self.assertEqual(lines[0]["end_ms"], 2906)
        self.assertEqual(lines[0]["text"], "Is this the real life?")
        self.assertEqual(len(lines[0]["words"]), 5)
        self.assertEqual(
            lines[0]["words"][0],
            {"text": "Is", "start_ms": 0, "end_ms": 458, "syllables": []},
        )
        self.assertEqual(
            lines[0]["words"][4],
            {"text": "life?", "start_ms": 2057, "end_ms": 2906, "syllables": []},
        )

        # Line 1: syllable-level timing on the word "fantasy?"
        self.assertEqual(lines[1]["start_ms"], 3725)
        self.assertEqual(lines[1]["end_ms"], 6373)
        self.assertEqual(lines[1]["text"], "Is this just fantasy?")
        self.assertEqual(len(lines[1]["words"]), 4)
        self.assertEqual(lines[1]["words"][0]["text"], "Is")
        self.assertEqual(lines[1]["words"][0]["syllables"], [])

        fantasy_word = lines[1]["words"][3]
        self.assertEqual(fantasy_word["text"], "fantasy?")
        self.assertEqual(fantasy_word["start_ms"], 4891)
        self.assertEqual(fantasy_word["end_ms"], 6373)
        self.assertEqual(
            fantasy_word["syllables"],
            [
                {"text": "fan", "start_ms": 4891, "end_ms": 5274},
                {"text": "ta", "start_ms": 5274, "end_ms": 5527},
                {"text": "sy?", "start_ms": 5527, "end_ms": 6373},
            ],
        )

    def test_parse_ttml_nested_syllables_model(self):
        ttml = """
        <tt xmlns="http://www.w3.org/ns/ttml">
            <body>
                <div>
                    <p begin="12.000s" end="15.000s">
                        <span begin="12000ms" end="12500ms">Hello</span>
                        <span begin="12.600s" end="14.000s">
                            <span begin="12.600s" end="13.200s">wor</span>
                            <span begin="13.200s" end="14.000s">ld</span>
                        </span>
                    </p>
                </div>
            </body>
        </tt>
        """
        sync, lines = parse_ttml(ttml)
        self.assertEqual(sync, "syllable")
        self.assertEqual(len(lines), 1)
        self.assertEqual(lines[0]["start_ms"], 12000)
        self.assertEqual(lines[0]["end_ms"], 15000)
        self.assertEqual(lines[0]["text"], "Hello world")
        self.assertEqual(len(lines[0]["words"]), 2)

        self.assertEqual(
            lines[0]["words"][0],
            {"text": "Hello", "start_ms": 12000, "end_ms": 12500, "syllables": []},
        )
        self.assertEqual(
            lines[0]["words"][1],
            {
                "text": "world",
                "start_ms": 12600,
                "end_ms": 14000,
                "syllables": [
                    {"text": "wor", "start_ms": 12600, "end_ms": 13200},
                    {"text": "ld", "start_ms": 13200, "end_ms": 14000},
                ],
            },
        )

    def test_parse_ttml_line_sync_only(self):
        ttml = """
        <tt xmlns="http://www.w3.org/ns/ttml">
            <body>
                <div>
                    <p begin="10.500" end="14.200">First line without inner spans</p>
                    <p begin="15.000">Second line without end attribute</p>
                </div>
            </body>
        </tt>
        """
        sync, lines = parse_ttml(ttml)
        self.assertEqual(sync, "line")
        self.assertEqual(len(lines), 2)
        self.assertEqual(lines[0]["start_ms"], 10500)
        self.assertEqual(lines[0]["end_ms"], 14200)
        self.assertEqual(lines[0]["text"], "First line without inner spans")

        # Words are populated with consistent schema and missing timestamps
        self.assertEqual(len(lines[0]["words"]), 5)
        self.assertEqual(
            lines[0]["words"][0],
            {"text": "First", "start_ms": None, "end_ms": None, "syllables": []},
        )

        self.assertEqual(lines[1]["start_ms"], 15000)
        self.assertIsNone(lines[1]["end_ms"])

    def test_parse_ttml_duet_agents(self):
        ttml = """
        <tt xmlns="http://www.w3.org/ns/ttml" xmlns:ttm="http://www.w3.org/ns/ttml#metadata">
            <head>
                <metadata>
                    <ttm:agent xml:id="v1" type="person"><ttm:name type="full">Singer 1</ttm:name></ttm:agent>
                    <ttm:agent xml:id="v2" type="person"><ttm:name type="full">Singer 2</ttm:name></ttm:agent>
                </metadata>
            </head>
            <body>
                <div>
                    <p begin="10.000s" end="14.000s" ttm:agent="v1">I'll stop the world</p>
                    <p begin="12.000s" end="15.000s" ttm:agent="v2">and melt with you</p>
                    <p begin="16.000s" end="20.000s" ttm:agent="v1">You've seen the difference</p>
                </div>
            </body>
        </tt>
        """
        sync, lines = parse_ttml(ttml)
        self.assertEqual(sync, "line")
        self.assertEqual(len(lines), 3)
        self.assertEqual(lines[0]["singer"], 0)
        self.assertEqual(lines[0]["text"], "I'll stop the world")
        self.assertEqual(lines[1]["singer"], 1)
        self.assertEqual(lines[1]["text"], "and melt with you")
        self.assertEqual(lines[2]["singer"], 0)
        self.assertEqual(lines[2]["text"], "You've seen the difference")


class TestLRCParser(unittest.TestCase):
    def test_parse_empty_lrc(self):
        sync, lines = parse_lrc("")
        self.assertEqual(sync, "none")
        self.assertEqual(lines, [])

    def test_parse_standard_lrc(self):
        lrc = """
        [ti:Creep]
        [ar:Radiohead]
        [al:Pablo Honey]
        [00:19.76]When you were here before
        [00:26.50]Couldn't look you in the eye
        """
        sync, lines = parse_lrc(lrc)
        self.assertEqual(sync, "line")
        self.assertEqual(len(lines), 2)
        self.assertEqual(lines[0]["start_ms"], 19760)
        self.assertEqual(lines[0]["end_ms"], 26500)  # automatically filled from line 1
        self.assertEqual(lines[0]["text"], "When you were here before")

        # Consistent normalized words representation with missing word timestamps
        self.assertEqual(len(lines[0]["words"]), 5)
        self.assertEqual(
            lines[0]["words"][0],
            {"text": "When", "start_ms": None, "end_ms": None, "syllables": []},
        )
        self.assertEqual(lines[1]["start_ms"], 26500)
        self.assertEqual(lines[1]["text"], "Couldn't look you in the eye")

    def test_parse_enhanced_lrc_word_and_syllable_sync(self):
        lrc = """
        [00:03.73]<00:03.73>Is <00:04.14>this <00:04.53>just <00:04.89>fan<00:05.27>ta<00:05.52>sy? <00:06.37>
        """
        sync, lines = parse_lrc(lrc)
        self.assertEqual(sync, "syllable")
        self.assertEqual(len(lines), 1)
        self.assertEqual(lines[0]["start_ms"], 3730)
        self.assertEqual(lines[0]["end_ms"], 6370)
        self.assertEqual(lines[0]["text"], "Is this just fantasy?")
        self.assertEqual(len(lines[0]["words"]), 4)

        self.assertEqual(
            lines[0]["words"][0],
            {"text": "Is", "start_ms": 3730, "end_ms": 4140, "syllables": []},
        )
        self.assertEqual(
            lines[0]["words"][1],
            {"text": "this", "start_ms": 4140, "end_ms": 4530, "syllables": []},
        )
        self.assertEqual(
            lines[0]["words"][2],
            {"text": "just", "start_ms": 4530, "end_ms": 4890, "syllables": []},
        )

        fantasy_word = lines[0]["words"][3]
        self.assertEqual(fantasy_word["text"], "fantasy?")
        self.assertEqual(fantasy_word["start_ms"], 4890)
        self.assertEqual(fantasy_word["end_ms"], 6370)
        self.assertEqual(
            fantasy_word["syllables"],
            [
                {"text": "fan", "start_ms": 4890, "end_ms": 5270},
                {"text": "ta", "start_ms": 5270, "end_ms": 5520},
                {"text": "sy?", "start_ms": 5520, "end_ms": 6370},
            ],
        )

    def test_parse_plain_unsynced_lyrics(self):
        plain = """
        When you were here before
        Couldn't look you in the eye
        """
        sync, lines = parse_lrc(plain)
        self.assertEqual(sync, "none")
        self.assertEqual(len(lines), 2)
        self.assertIsNone(lines[0]["start_ms"])
        self.assertIsNone(lines[0]["end_ms"])
        self.assertEqual(lines[0]["text"], "When you were here before")
        self.assertEqual(len(lines[0]["words"]), 5)
        self.assertEqual(
            lines[0]["words"][0],
            {"text": "When", "start_ms": None, "end_ms": None, "syllables": []},
        )

    def test_parse_lrc_with_offset(self):
        lrc = """
        [offset:+500]
        [00:10.00]Line with offset
        """
        sync, lines = parse_lrc(lrc)
        self.assertEqual(lines[0]["start_ms"], 10500)

    def test_parse_lrc_multiple_leading_tags(self):
        lrc = """
        [00:10.00][00:20.00]Repeated chorus line
        """
        sync, lines = parse_lrc(lrc)
        self.assertEqual(len(lines), 2)
        self.assertEqual(lines[0]["start_ms"], 10000)
        self.assertEqual(lines[0]["text"], "Repeated chorus line")
        self.assertEqual(lines[1]["start_ms"], 20000)
        self.assertEqual(lines[1]["text"], "Repeated chorus line")


class TestMatchingAndScoring(unittest.TestCase):
    def setUp(self):
        self.provider = LrcRedProvider()

    def test_match_track_exact(self):
        track = {
            "title": "Creep",
            "artist": "Radiohead",
            "album": "Pablo Honey",
            "duration_seconds": 238.64,
        }
        candidates = [
            {
                "isrc": "GBAYE9200070",
                "title": "Creep",
                "artist": "Radiohead",
                "album": "Pablo Honey",
                "duration": 238.64,
            },
            {
                "isrc": "GBAYE9300465",
                "title": "Creep (Acoustic)",
                "artist": "Radiohead",
                "album": "Creep - EP",
                "duration": 258.64,
            },
            {
                "isrc": "OTHER123",
                "title": "Creep",
                "artist": "TLC",
                "duration": 268.0,
            },
        ]
        best = self.provider.match_track(track, candidates)
        self.assertIsNotNone(best)
        self.assertEqual(best["isrc"], "GBAYE9200070")

    def test_match_track_version_conflict_penalty(self):
        conflicts = _version_conflicts("Creep", "Creep (Acoustic)")
        self.assertIn("acoustic", conflicts)

        conflicts_req = _version_conflicts("Creep (Acoustic)", "Creep (Acoustic)")
        self.assertEqual(conflicts_req, [])

    def test_match_track_rejects_large_duration_difference(self):
        track = {
            "title": "Song Title",
            "artist": "Artist",
            "duration_seconds": 180.0,
        }
        candidates = [
            {
                "isrc": "CAND1",
                "title": "Song Title",
                "artist": "Artist",
                "duration": 600.0,
            }
        ]
        best = self.provider.match_track(track, candidates)
        self.assertIsNone(best)


class TestLrcRedProviderMockedRequests(unittest.TestCase):
    def setUp(self):
        self.mock_session = MagicMock()
        self.provider = LrcRedProvider(session=self.mock_session)

    def test_headers_and_user_agent(self):
        headers = self.provider._headers()
        self.assertIn("User-Agent", headers)
        self.assertIn("ANSImuz", headers["User-Agent"])
        self.assertIn("contact:", headers["User-Agent"])
        self.assertEqual(self.provider.attribution, "Lyrics provided by lrc.red")

    def test_search_request_construction(self):
        mock_resp = MagicMock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {
            "hits": [
                {
                    "isrc": "GBAYE9200070",
                    "title": "Creep",
                    "artist": "Radiohead",
                    "album": "Pablo Honey",
                    "duration": 238.64,
                }
            ]
        }
        self.mock_session.get.return_value = mock_resp

        hits = self.provider.search("Creep Radiohead")
        self.assertEqual(len(hits), 1)
        self.assertEqual(hits[0]["isrc"], "GBAYE9200070")
        self.mock_session.get.assert_called_once_with(
            "https://lrc.red/search.json",
            params={"q": "Creep Radiohead"},
            headers=self.provider._headers(accept="application/json"),
            timeout=DEFAULT_TIMEOUT,
        )

    def test_get_track_metadata(self):
        mock_resp = MagicMock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {
            "isrc": "GBAYE9200070",
            "title": "Creep",
            "artist": "Radiohead",
            "files": {
                "ttml": "https://lrc.red/s/GBAYE9200070.ttml",
                "lrc": "https://lrc.red/s/GBAYE9200070.lrc",
            },
        }
        self.mock_session.get.return_value = mock_resp

        meta = self.provider.get_track_metadata("GBAYE9200070")
        self.assertIsNotNone(meta)
        self.assertEqual(meta["title"], "Creep")

    def test_get_lyrics_by_isrc_normalized_schema(self):
        mock_meta_resp = MagicMock()
        mock_meta_resp.status_code = 200
        mock_meta_resp.json.return_value = {
            "isrc": "EXAMPLE12345",
            "title": "Example Song",
            "artist": "Example Artist",
            "files": {
                "ttml": "https://lrc.red/s/EXAMPLE12345.ttml",
            },
        }

        mock_ttml_resp = MagicMock()
        mock_ttml_resp.status_code = 200
        mock_ttml_resp.text = """
        <tt xmlns="http://www.w3.org/ns/ttml">
            <body>
                <div>
                    <p begin="12.000s" end="15.000s">
                        <span begin="12.000" end="12.500">Hello</span>
                        <span begin="12.600" end="14.000">world</span>
                    </p>
                </div>
            </body>
        </tt>
        """

        self.mock_session.get.side_effect = [mock_meta_resp, mock_ttml_resp]

        lyrics = self.provider.get_lyrics_by_isrc("EXAMPLE12345")
        self.assertIsNotNone(lyrics)

        # Verify exact target schema matches user specification
        self.assertEqual(lyrics["source"], "lrc.red")
        self.assertEqual(lyrics["sync_type"], "word")
        self.assertEqual(lyrics["title"], "Example Song")
        self.assertEqual(lyrics["artist"], "Example Artist")
        self.assertEqual(lyrics["isrc"], "EXAMPLE12345")
        self.assertEqual(len(lyrics["lines"]), 1)

        line = lyrics["lines"][0]
        self.assertEqual(line["start_ms"], 12000)
        self.assertEqual(line["end_ms"], 15000)
        self.assertEqual(len(line["words"]), 2)
        self.assertEqual(
            line["words"][0],
            {"text": "Hello", "start_ms": 12000, "end_ms": 12500, "syllables": []},
        )
        self.assertEqual(
            line["words"][1],
            {"text": "world", "start_ms": 12600, "end_ms": 14000, "syllables": []},
        )

    def test_get_lyrics_by_isrc_fallback_to_lrc(self):
        mock_meta_resp = MagicMock()
        mock_meta_resp.status_code = 200
        mock_meta_resp.json.return_value = {
            "isrc": "GBAYE9200070",
            "title": "Creep",
            "artist": "Radiohead",
            "files": {
                "ttml": "https://lrc.red/s/GBAYE9200070.ttml",
                "lrc": "https://lrc.red/s/GBAYE9200070.lrc",
            },
        }

        # TTML returns 404
        mock_ttml_resp = MagicMock()
        mock_ttml_resp.status_code = 404

        # LRC returns 200 with standard LRC
        mock_lrc_resp = MagicMock()
        mock_lrc_resp.status_code = 200
        mock_lrc_resp.text = "[00:19.76]When you were here before"

        self.mock_session.get.side_effect = [mock_meta_resp, mock_ttml_resp, mock_lrc_resp]

        lyrics = self.provider.get_lyrics_by_isrc("GBAYE9200070")
        self.assertIsNotNone(lyrics)
        self.assertEqual(lyrics["sync_type"], "line")
        self.assertEqual(lyrics["source"], "lrc.red")
        self.assertIsNone(lyrics["raw_ttml"])
        self.assertIsNotNone(lyrics["raw_lrc"])
        self.assertEqual(lyrics["plain_lyrics"], "When you were here before")
        self.assertEqual(len(lyrics["lines"][0]["words"]), 5)

    def test_get_lyrics_end_to_end_search_and_fetch(self):
        mock_search_resp = MagicMock()
        mock_search_resp.status_code = 200
        mock_search_resp.json.return_value = {
            "hits": [
                {
                    "isrc": "GBAYE9200070",
                    "title": "Creep",
                    "artist": "Radiohead",
                    "duration": 238.64,
                }
            ]
        }

        mock_meta_resp = MagicMock()
        mock_meta_resp.status_code = 200
        mock_meta_resp.json.return_value = {
            "isrc": "GBAYE9200070",
            "title": "Creep",
            "artist": "Radiohead",
            "files": {"lrc": "https://lrc.red/s/GBAYE9200070.lrc"},
        }

        mock_ttml_resp = MagicMock()
        mock_ttml_resp.status_code = 404

        mock_lrc_resp = MagicMock()
        mock_lrc_resp.status_code = 200
        mock_lrc_resp.text = "[00:19.76]When you were here before"

        self.mock_session.get.side_effect = [
            mock_search_resp,
            mock_meta_resp,
            mock_ttml_resp,
            mock_lrc_resp,
        ]

        track = {
            "title": "Creep",
            "artist": "Radiohead",
            "duration_seconds": 238.64,
        }
        res = self.provider.get_lyrics(track)
        self.assertIsNotNone(res)
        self.assertEqual(res["isrc"], "GBAYE9200070")
        self.assertEqual(res["title"], "Creep")
        self.assertEqual(res["sync_type"], "line")
        self.assertEqual(res["source"], "lrc.red")


class TestLrcRedErrorHandling(unittest.TestCase):
    def setUp(self):
        self.mock_session = MagicMock()
        self.provider = LrcRedProvider(session=self.mock_session)

    def test_rate_limit_429_raises_rate_limit_error(self):
        mock_resp = MagicMock()
        mock_resp.status_code = 429
        self.mock_session.get.return_value = mock_resp

        with self.assertRaises(RateLimitError):
            self.provider.search("test query")

    def test_network_connection_error_raises_network_error(self):
        import requests

        self.mock_session.get.side_effect = requests.exceptions.ConnectionError(
            "Failed to establish connection"
        )

        with self.assertRaises(NetworkError):
            self.provider.search("test query")

    def test_network_timeout_raises_network_error(self):
        import requests

        self.mock_session.get.side_effect = requests.exceptions.Timeout("Read timed out")

        with self.assertRaises(NetworkError):
            self.provider.get_track_metadata("GBAYE9200070")

    def test_server_error_500_raises_provider_error(self):
        mock_resp = MagicMock()
        mock_resp.status_code = 500
        mock_resp.text = "Internal Server Error"
        self.mock_session.get.return_value = mock_resp

        with self.assertRaises(ProviderError):
            self.provider.search("test query")


if __name__ == "__main__":
    unittest.main()