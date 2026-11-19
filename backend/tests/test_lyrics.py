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
    clean_title_for_search,
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
    parse_lyrics_plus,
    parse_lyricsfile,
    parse_qrc,
    parse_timestamp,
    parse_ttml,
)
from providers.lyrics.better_lyrics import BetterLyricsProvider
from providers.lyrics.genius import (
    DEFAULT_GENIUS_ATTRIBUTION,
    DEFAULT_GENIUS_USER_AGENT,
    GeniusProvider,
    best_match,
    clean_query,
    is_section_header,
    parse_genius_html,
    strip_artifacts,
    text_to_lyric_lines,
)
from providers.lyrics.lrclib import LrclibProvider
from providers.lyrics.lyrics_plus import LyricsPlusProvider
from providers.lyrics.lrc_red import LrcRedProvider, _version_conflicts
from providers.lyrics.megalobiz import (
    DEFAULT_MEGALOBIZ_ATTRIBUTION,
    MegalobizProvider,
    score_candidate as megalobiz_score_candidate,
)
import providers.lyrics


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


class TestCleanTitleForSearch(unittest.TestCase):
    def test_clean_video_and_audio_tags(self):
        self.assertEqual(
            clean_title_for_search("Levitating (Official Music Video)"),
            "Levitating",
        )
        self.assertEqual(
            clean_title_for_search("Starboy [Official Video]"),
            "Starboy",
        )
        self.assertEqual(
            clean_title_for_search("Song Title (Lyric Video)"),
            "Song Title",
        )
        self.assertEqual(
            clean_title_for_search("Another Song - Official Audio"),
            "Another Song",
        )
        self.assertEqual(
            clean_title_for_search("Track (Remastered)"),
            "Track",
        )

    def test_clean_title_unchanged(self):
        self.assertEqual(clean_title_for_search("Bohemian Rhapsody"), "Bohemian Rhapsody")
        self.assertEqual(clean_title_for_search(""), "")


class TestParseQrc(unittest.TestCase):
    def test_parse_empty_or_whitespace(self):
        self.assertEqual(parse_qrc(""), ("none", []))
        self.assertEqual(parse_qrc("   "), ("none", []))

    def test_parse_standard_qrc_words(self):
        qrc = (
            "[1000,2000]Hello (1000,500)world (1500,500)\n"
            "[3000,2000]How (3000,600)are (3600,400)you (4000,1000)"
        )
        sync_type, lines = parse_qrc(qrc)
        self.assertEqual(sync_type, "word")
        self.assertEqual(len(lines), 2)

        line0 = lines[0]
        self.assertEqual(line0["start_ms"], 1000)
        self.assertEqual(line0["end_ms"], 3000)
        self.assertEqual(line0["text"], "Hello world")
        self.assertEqual(len(line0["words"]), 2)
        self.assertEqual(line0["words"][0]["text"], "Hello")
        self.assertEqual(line0["words"][0]["start_ms"], 1000)
        self.assertEqual(line0["words"][0]["end_ms"], 1500)
        self.assertEqual(line0["words"][1]["text"], "world")
        self.assertEqual(line0["words"][1]["start_ms"], 1500)
        self.assertEqual(line0["words"][1]["end_ms"], 2000)

    def test_parse_qrc_xml_wrapped(self):
        xml_qrc = (
            '<?xml version="1.0" encoding="utf-8"?>\n'
            '<QrcInfos>\n'
            '<Lyric_1 LyricType="1" LyricContent="[ti:Test]\n'
            '[offset:0]\n'
            '[16392,2152]I\'m (16392,264)tryna (16656,208)put (16864,178)you (17042,181)"/>\n'
            '</QrcInfos>'
        )
        sync_type, lines = parse_qrc(xml_qrc)
        self.assertEqual(sync_type, "word")
        self.assertEqual(len(lines), 1)
        self.assertEqual(lines[0]["text"], "I'm tryna put you")
        self.assertEqual(lines[0]["start_ms"], 16392)
        self.assertEqual(lines[0]["end_ms"], 16392 + 2152)

    def test_parse_qrc_background_vocals(self):
        qrc = "[5000,1500]((5000,750)yeah (5750,750))"
        sync_type, lines = parse_qrc(qrc)
        self.assertEqual(len(lines), 1)
        self.assertTrue(lines[0]["is_background"])


class TestBetterLyricsProvider(unittest.TestCase):
    def setUp(self):
        self.mock_session = MagicMock()
        self.provider = BetterLyricsProvider(session=self.mock_session)

    def test_name_and_attribution(self):
        self.assertEqual(self.provider.name, "BetterLyrics")
        self.assertIn("BetterLyrics", self.provider.attribution)

    def test_missing_title_returns_none(self):
        self.assertIsNone(self.provider.get_lyrics({}))
        self.assertIsNone(self.provider.get_lyrics({"title": ""}))

    def test_primary_endpoint_success_ttml(self):
        ttml_content = (
            '<tt xmlns="http://www.w3.org/ns/ttml" xmlns:itunes="http://music.apple.com/lyric-ttml-internal">'
            '<body><div><p begin="00:09.50" end="00:11.70">'
            '<span>If </span><span>you </span><span>wanna</span>'
            '</p></div></body></tt>'
        )
        mock_resp = MagicMock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {"ttml": ttml_content}
        self.mock_session.get.return_value = mock_resp

        track = {
            "title": "Levitating",
            "artist": "Dua Lipa",
            "duration_ms": 203000,
            "album": "Future Nostalgia",
        }
        res = self.provider.get_lyrics(track)
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "BetterLyrics")
        self.assertEqual(res["title"], "Levitating")
        self.assertEqual(res["artist"], "Dua Lipa")
        self.assertEqual(len(res["lines"]), 1)
        self.assertEqual(res["lines"][0]["text"], "If you wanna")

    def test_fallback_to_portato_when_primary_fails(self):
        # First call (BASE) returns 401
        base_resp = MagicMock()
        base_resp.status_code = 401
        base_resp.json.return_value = {"error": "API key required"}

        # Second call (PORTATO) returns 200 with QRC
        portato_resp = MagicMock()
        portato_resp.status_code = 200
        portato_resp.json.return_value = {
            "lyrics": "[1000,2000]Starboy (1000,1000)rocks (2000,1000)",
            "provider": "qq",
        }

        self.mock_session.get.side_effect = [base_resp, portato_resp]

        track = {
            "title": "Starboy",
            "artist": "The Weeknd",
            "duration_ms": 230000,
        }
        res = self.provider.get_lyrics(track)
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "BetterLyrics (QQ)")
        self.assertEqual(len(res["lines"]), 1)
        self.assertEqual(res["lines"][0]["text"], "Starboy rocks")

    def test_both_endpoints_401_returns_none(self):
        base_resp = MagicMock()
        base_resp.status_code = 401
        portato_resp = MagicMock()
        portato_resp.status_code = 401

        self.mock_session.get.side_effect = [base_resp, portato_resp]

        track = {"title": "Unknown Song", "artist": "Unknown Artist", "duration_ms": 180000}
        self.assertIsNone(self.provider.get_lyrics(track))

    def test_request_exception_handled_gracefully(self):
        import requests
        self.mock_session.get.side_effect = requests.RequestException("Network failed")
        track = {"title": "Any Song", "artist": "Any Artist"}
        self.assertIsNone(self.provider.get_lyrics(track))


class TestParseLyricsPlus(unittest.TestCase):
    def test_parse_empty_or_invalid(self):
        self.assertEqual(parse_lyrics_plus({}), ("none", []))
        self.assertEqual(parse_lyrics_plus({"lyrics": []}), ("none", []))

    def test_parse_syllables_and_word_grouping(self):
        data = {
            "type": "Word",
            "lyrics": [
                {
                    "time": 1000,
                    "duration": 2000,
                    "syllabus": [
                        {"time": 1000, "duration": 400, "text": "He"},
                        {"time": 1400, "duration": 600, "text": "llo "},
                        {"time": 2000, "duration": 1000, "text": "world"},
                    ],
                }
            ],
        }
        sync_type, lines = parse_lyrics_plus(data)
        self.assertEqual(sync_type, "syllable")
        self.assertEqual(len(lines), 1)

        line0 = lines[0]
        self.assertEqual(line0["start_ms"], 1000)
        self.assertEqual(line0["end_ms"], 3000)
        self.assertEqual(line0["text"], "Hello world")
        self.assertEqual(len(line0["words"]), 2)

        # Word 1: Hello (2 syllables)
        w0 = line0["words"][0]
        self.assertEqual(w0["text"], "Hello")
        self.assertEqual(w0["start_ms"], 1000)
        self.assertEqual(w0["end_ms"], 2000)
        self.assertEqual(len(w0["syllables"]), 2)
        self.assertEqual(w0["syllables"][0]["text"], "He")
        self.assertEqual(w0["syllables"][1]["text"], "llo")

        # Word 2: world (single token)
        w1 = line0["words"][1]
        self.assertEqual(w1["text"], "world")
        self.assertEqual(w1["start_ms"], 2000)
        self.assertEqual(w1["end_ms"], 3000)
        self.assertEqual(len(w1["syllables"]), 0)

    def test_parse_line_level_sync_only(self):
        data = {
            "lyrics": [
                {"time": 2000, "duration": 4000, "text": "Line level synced only"}
            ]
        }
        sync_type, lines = parse_lyrics_plus(data)
        self.assertEqual(sync_type, "line")
        self.assertEqual(len(lines), 1)
        self.assertEqual(lines[0]["start_ms"], 2000)
        self.assertEqual(lines[0]["end_ms"], 6000)
        self.assertEqual(lines[0]["text"], "Line level synced only")

    def test_parse_duet_singers_and_opposite_tag(self):
        data = {
            "lyrics": [
                {"time": 1000, "text": "Lead line", "element": {"singer": "v1"}},
                {"time": 3000, "text": "Duet answering line", "element": {"singer": "v2"}},
                {"time": 5000, "text": "Opposite line", "element": ["opposite"]},
            ]
        }
        sync_type, lines = parse_lyrics_plus(data)
        self.assertEqual(len(lines), 3)
        self.assertEqual(lines[0]["singer"], 0)  # v1 is primary
        self.assertEqual(lines[1]["singer"], 1)  # v2 is secondary
        self.assertEqual(lines[2]["singer"], 1)  # opposite tag forces singer 1

    def test_parse_background_agent_detection(self):
        data = {
            "metadata": {
                "agents": {
                    "v2": {"type": "background", "alias": "bg_singer"}
                }
            },
            "lyrics": [
                {"time": 1000, "text": "Lead", "element": {"singer": "v1"}},
                {"time": 2000, "text": "Harmony", "element": {"singer": "v2"}},
            ],
        }
        sync_type, lines = parse_lyrics_plus(data)
        self.assertEqual(len(lines), 2)
        self.assertFalse(lines[0]["is_background"])
        self.assertTrue(lines[1]["is_background"])


class TestLyricsPlusProvider(unittest.TestCase):
    def setUp(self):
        self.mock_session = MagicMock()
        self.provider = LyricsPlusProvider(
            mirrors=["https://mirror1.test", "https://mirror2.test"],
            session=self.mock_session,
        )

    def test_name_and_attribution(self):
        self.assertEqual(self.provider.name, "LyricsPlus")
        self.assertIn("LyricsPlus", self.provider.attribution)

    def test_missing_title_returns_none(self):
        self.assertIsNone(self.provider.get_lyrics({}))
        self.assertIsNone(self.provider.get_lyrics({"title": "  "}))

    def test_race_selects_first_valid_mirror_and_remembers_last_good(self):
        def mock_get(url, **kwargs):
            mock_resp = MagicMock()
            if "mirror1" in url:
                mock_resp.status_code = 404
            elif "mirror2" in url:
                mock_resp.status_code = 200
                mock_resp.json.return_value = {
                    "lyrics": [
                        {
                            "time": 5000,
                            "text": "Winner line",
                            "syllabus": [{"time": 5000, "duration": 500, "text": "Winner "}],
                        }
                    ]
                }
            return mock_resp

        self.mock_session.get.side_effect = mock_get

        track = {"title": "Levitating", "artist": "Dua Lipa", "duration_ms": 203000}
        res = self.provider.get_lyrics(track)
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "LyricsPlus")
        self.assertEqual(len(res["lines"]), 1)
        self.assertEqual(res["lines"][0]["text"], "Winner")
        self.assertEqual(self.provider.last_good_mirror, "https://mirror2.test")

    def test_all_mirrors_failing_returns_none(self):
        mock_resp = MagicMock()
        mock_resp.status_code = 500
        self.mock_session.get.return_value = mock_resp

        track = {"title": "Unknown Song", "artist": "Unknown Artist"}
        self.assertIsNone(self.provider.get_lyrics(track))

    def test_shared_executor_is_bounded_and_reusable(self):
        from providers.lyrics.lyrics_plus import get_shared_mirror_executor

        exec1 = get_shared_mirror_executor()
        exec2 = get_shared_mirror_executor()
        self.assertIs(exec1, exec2, "Shared mirror executor must be a singleton")
        self.assertEqual(exec1._max_workers, 8)

    def test_thread_safe_last_good_mirror(self):
        self.assertIsNone(self.provider.last_good_mirror)
        self.provider.set_last_good_mirror("https://test.mirror")
        self.assertEqual(self.provider.last_good_mirror, "https://test.mirror")

    def test_mirror_race_cancels_losing_requests(self):
        started_mirrors: list[str] = []

        def slow_get(url, **kwargs):
            started_mirrors.append(url)
            mock_resp = MagicMock()
            if "mirror1" in url:
                mock_resp.status_code = 200
                mock_resp.json.return_value = {
                    "lyrics": [{"time": 1000, "text": "Line 1", "syllabus": []}]
                }
                return mock_resp
            # mirror2 would be slow
            import time

            time.sleep(0.05)
            mock_resp.status_code = 200
            mock_resp.json.return_value = {
                "lyrics": [{"time": 1000, "text": "Slow Line", "syllabus": []}]
            }
            return mock_resp

        self.mock_session.get.side_effect = slow_get
        track = {"title": "Fast Song", "artist": "Artist"}
        res = self.provider.get_lyrics(track)
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "LyricsPlus")
        self.assertEqual(res["url"], "https://mirror1.test")
        self.assertIn("LyricsPlus", res["attribution"])


class TestParseLyricsfile(unittest.TestCase):
    def test_parse_empty_or_invalid(self):
        self.assertIsNone(parse_lyricsfile(""))
        self.assertIsNone(parse_lyricsfile("invalid yaml without lines"))

    def test_parse_word_sync_lyricsfile(self):
        yaml_text = (
            "version: '1.0'\n"
            "lines:\n"
            "- text: When you were here before\n"
            "  start_ms: 19980\n"
            "  end_ms: 22048\n"
            "  words:\n"
            "  - text: 'When '\n"
            "    start_ms: 19980\n"
            "  - text: 'you '\n"
            "    start_ms: 20260\n"
            "  - text: before\n"
            "    start_ms: 20800\n"
        )
        res = parse_lyricsfile(yaml_text)
        self.assertIsNotNone(res)
        sync_type, lines = res
        self.assertEqual(sync_type, "word")
        self.assertEqual(len(lines), 1)
        self.assertEqual(lines[0]["text"], "When you were here before")
        self.assertEqual(lines[0]["start_ms"], 19980)
        self.assertEqual(lines[0]["end_ms"], 22048)
        self.assertEqual(len(lines[0]["words"]), 3)
        self.assertEqual(lines[0]["words"][0]["text"], "When")
        self.assertEqual(lines[0]["words"][0]["start_ms"], 19980)
        self.assertEqual(lines[0]["words"][0]["end_ms"], 20260)


class TestLrclibProvider(unittest.TestCase):
    def setUp(self):
        self.mock_session = MagicMock()
        self.provider = LrclibProvider(session=self.mock_session)

    def test_name_and_attribution(self):
        self.assertEqual(self.provider.name, "LRCLIB")
        self.assertIn("LRCLIB", self.provider.attribution)

    def test_missing_title_returns_none(self):
        self.assertIsNone(self.provider.get_lyrics({}))

    def test_get_by_signature_with_lyricsfile_word_sync(self):
        mock_resp = MagicMock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {
            "id": 12345,
            "trackName": "Creep",
            "artistName": "Radiohead",
            "hasWordSync": True,
            "lyricsfile": (
                "lines:\n"
                "- text: I'm a creep\n"
                "  start_ms: 1000\n"
                "  end_ms: 3000\n"
                "  words:\n"
                "  - text: 'I''m '\n"
                "    start_ms: 1000\n"
                "  - text: creep\n"
                "    start_ms: 2000\n"
            ),
        }
        self.mock_session.get.return_value = mock_resp

        res = self.provider.get_lyrics({"title": "Creep", "artist": "Radiohead", "duration_ms": 238000})
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "LRCLIB")
        self.assertEqual(res["sync_type"], "word")
        self.assertEqual(len(res["lines"]), 1)
        self.assertEqual(res["lines"][0]["text"], "I'm a creep")

    def test_fallback_to_search_when_get_returns_404(self):
        get_resp = MagicMock()
        get_resp.status_code = 404

        search_resp = MagicMock()
        search_resp.status_code = 200
        search_resp.json.return_value = [
            {
                "id": 999,
                "trackName": "Starboy",
                "artistName": "The Weeknd",
                "duration": 230,
                "syncedLyrics": "[00:16.39] I'm tryna put you in the worst mood",
            }
        ]
        self.mock_session.get.side_effect = [get_resp, search_resp]

        res = self.provider.get_lyrics({"title": "Starboy", "artist": "The Weeknd", "duration_ms": 230000})
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "LRCLIB")
        self.assertEqual(res["sync_type"], "line")
        self.assertEqual(len(res["lines"]), 1)


class TestLyricsProviderChain(unittest.TestCase):
    @patch("providers.lyrics.get_genius_provider")
    @patch("providers.lyrics.get_megalobiz_provider")
    @patch("providers.lyrics.get_better_lyrics_provider")
    @patch("providers.lyrics.get_lrclib_provider")
    @patch("providers.lyrics.get_lyrics_plus_provider")
    @patch("providers.lyrics.get_lrc_red_provider")
    def test_chain_prefers_lrc_red(
        self, mock_get_lrcred, mock_get_plus, mock_get_lrclib, mock_get_better, mock_get_mega, mock_get_genius
    ):
        mock_lrcred = MagicMock()
        mock_lrcred.get_lyrics.return_value = {
            "source": "lrc.red",
            "lines": [{"text": "Hello from lrc.red", "start_ms": 0}],
        }
        mock_get_lrcred.return_value = mock_lrcred

        mock_plus = MagicMock()
        mock_get_plus.return_value = mock_plus

        mock_lrclib = MagicMock()
        mock_get_lrclib.return_value = mock_lrclib

        mock_better = MagicMock()
        mock_get_better.return_value = mock_better

        mock_mega = MagicMock()
        mock_get_mega.return_value = mock_mega

        mock_genius = MagicMock()
        mock_get_genius.return_value = mock_genius

        res = providers.lyrics.get_lyrics({"title": "Test", "artist": "Artist"})
        self.assertEqual(res["source"], "lrc.red")
        mock_lrcred.get_lyrics.assert_called_once()
        mock_plus.get_lyrics.assert_not_called()
        mock_lrclib.get_lyrics.assert_not_called()
        mock_better.get_lyrics.assert_not_called()
        mock_mega.get_lyrics.assert_not_called()
        mock_genius.get_lyrics.assert_not_called()

    @patch("providers.lyrics.get_genius_provider")
    @patch("providers.lyrics.get_megalobiz_provider")
    @patch("providers.lyrics.get_better_lyrics_provider")
    @patch("providers.lyrics.get_lrclib_provider")
    @patch("providers.lyrics.get_lyrics_plus_provider")
    @patch("providers.lyrics.get_lrc_red_provider")
    def test_chain_falls_back_to_lyrics_plus_when_lrcred_returns_none(
        self, mock_get_lrcred, mock_get_plus, mock_get_lrclib, mock_get_better, mock_get_mega, mock_get_genius
    ):
        mock_lrcred = MagicMock()
        mock_lrcred.get_lyrics.return_value = None
        mock_get_lrcred.return_value = mock_lrcred

        mock_plus = MagicMock()
        mock_plus.get_lyrics.return_value = {
            "source": "LyricsPlus",
            "lines": [{"text": "Plus line", "start_ms": 50}],
        }
        mock_get_plus.return_value = mock_plus

        mock_lrclib = MagicMock()
        mock_get_lrclib.return_value = mock_lrclib

        mock_better = MagicMock()
        mock_get_better.return_value = mock_better

        mock_mega = MagicMock()
        mock_get_mega.return_value = mock_mega

        mock_genius = MagicMock()
        mock_get_genius.return_value = mock_genius

        res = providers.lyrics.get_lyrics({"title": "Test", "artist": "Artist"})
        self.assertEqual(res["source"], "LyricsPlus")
        mock_lrcred.get_lyrics.assert_called_once()
        mock_plus.get_lyrics.assert_called_once()
        mock_lrclib.get_lyrics.assert_not_called()
        mock_better.get_lyrics.assert_not_called()
        mock_mega.get_lyrics.assert_not_called()
        mock_genius.get_lyrics.assert_not_called()

    @patch("providers.lyrics.get_genius_provider")
    @patch("providers.lyrics.get_megalobiz_provider")
    @patch("providers.lyrics.get_better_lyrics_provider")
    @patch("providers.lyrics.get_lrclib_provider")
    @patch("providers.lyrics.get_lyrics_plus_provider")
    @patch("providers.lyrics.get_lrc_red_provider")
    def test_chain_falls_back_to_lrclib_when_lrcred_and_plus_return_none(
        self, mock_get_lrcred, mock_get_plus, mock_get_lrclib, mock_get_better, mock_get_mega, mock_get_genius
    ):
        mock_lrcred = MagicMock()
        mock_lrcred.get_lyrics.return_value = None
        mock_get_lrcred.return_value = mock_lrcred

        mock_plus = MagicMock()
        mock_plus.get_lyrics.return_value = None
        mock_get_plus.return_value = mock_plus

        mock_lrclib = MagicMock()
        mock_lrclib.get_lyrics.return_value = {
            "source": "LRCLIB",
            "lines": [{"text": "LRCLIB line", "start_ms": 75}],
        }
        mock_get_lrclib.return_value = mock_lrclib

        mock_better = MagicMock()
        mock_get_better.return_value = mock_better

        mock_mega = MagicMock()
        mock_get_mega.return_value = mock_mega

        mock_genius = MagicMock()
        mock_get_genius.return_value = mock_genius

        res = providers.lyrics.get_lyrics({"title": "Test", "artist": "Artist"})
        self.assertEqual(res["source"], "LRCLIB")
        mock_lrcred.get_lyrics.assert_called_once()
        mock_plus.get_lyrics.assert_called_once()
        mock_lrclib.get_lyrics.assert_called_once()
        mock_better.get_lyrics.assert_not_called()
        mock_mega.get_lyrics.assert_not_called()
        mock_genius.get_lyrics.assert_not_called()

    @patch("providers.lyrics.get_genius_provider")
    @patch("providers.lyrics.get_megalobiz_provider")
    @patch("providers.lyrics.get_better_lyrics_provider")
    @patch("providers.lyrics.get_lrclib_provider")
    @patch("providers.lyrics.get_lyrics_plus_provider")
    @patch("providers.lyrics.get_lrc_red_provider")
    def test_chain_falls_back_to_megalobiz_when_first_three_return_none(
        self, mock_get_lrcred, mock_get_plus, mock_get_lrclib, mock_get_better, mock_get_mega, mock_get_genius
    ):
        mock_lrcred = MagicMock()
        mock_lrcred.get_lyrics.return_value = None
        mock_get_lrcred.return_value = mock_lrcred

        mock_plus = MagicMock()
        mock_plus.get_lyrics.return_value = None
        mock_get_plus.return_value = mock_plus

        mock_lrclib = MagicMock()
        mock_lrclib.get_lyrics.return_value = None
        mock_get_lrclib.return_value = mock_lrclib

        mock_mega = MagicMock()
        mock_mega.get_lyrics.return_value = {
            "source": "Megalobiz",
            "sync_type": "line",
            "lines": [{"text": "Megalobiz line", "start_ms": 150}],
        }
        mock_get_mega.return_value = mock_mega

        mock_better = MagicMock()
        mock_get_better.return_value = mock_better

        mock_genius = MagicMock()
        mock_get_genius.return_value = mock_genius

        res = providers.lyrics.get_lyrics({"title": "Test", "artist": "Artist"})
        self.assertEqual(res["source"], "Megalobiz")
        mock_lrcred.get_lyrics.assert_called_once()
        mock_plus.get_lyrics.assert_called_once()
        mock_lrclib.get_lyrics.assert_called_once()
        mock_mega.get_lyrics.assert_called_once()
        mock_better.get_lyrics.assert_not_called()
        mock_genius.get_lyrics.assert_not_called()

    @patch("providers.lyrics.get_genius_provider")
    @patch("providers.lyrics.get_megalobiz_provider")
    @patch("providers.lyrics.get_better_lyrics_provider")
    @patch("providers.lyrics.get_lrclib_provider")
    @patch("providers.lyrics.get_lyrics_plus_provider")
    @patch("providers.lyrics.get_lrc_red_provider")
    def test_chain_falls_back_to_better_lyrics_when_first_four_return_none(
        self, mock_get_lrcred, mock_get_plus, mock_get_lrclib, mock_get_better, mock_get_mega, mock_get_genius
    ):
        mock_lrcred = MagicMock()
        mock_lrcred.get_lyrics.return_value = None
        mock_get_lrcred.return_value = mock_lrcred

        mock_plus = MagicMock()
        mock_plus.get_lyrics.return_value = None
        mock_get_plus.return_value = mock_plus

        mock_lrclib = MagicMock()
        mock_lrclib.get_lyrics.return_value = None
        mock_get_lrclib.return_value = mock_lrclib

        mock_mega = MagicMock()
        mock_mega.get_lyrics.return_value = None
        mock_get_mega.return_value = mock_mega

        mock_better = MagicMock()
        mock_better.get_lyrics.return_value = {
            "source": "BetterLyrics",
            "lines": [{"text": "Fallback BetterLyrics", "start_ms": 100}],
        }
        mock_get_better.return_value = mock_better

        mock_genius = MagicMock()
        mock_get_genius.return_value = mock_genius

        res = providers.lyrics.get_lyrics({"title": "Test", "artist": "Artist"})
        self.assertEqual(res["source"], "BetterLyrics")
        mock_lrcred.get_lyrics.assert_called_once()
        mock_plus.get_lyrics.assert_called_once()
        mock_lrclib.get_lyrics.assert_called_once()
        mock_mega.get_lyrics.assert_called_once()
        mock_better.get_lyrics.assert_called_once()
        mock_genius.get_lyrics.assert_not_called()

    @patch("providers.lyrics.get_genius_provider")
    @patch("providers.lyrics.get_megalobiz_provider")
    @patch("providers.lyrics.get_better_lyrics_provider")
    @patch("providers.lyrics.get_lrclib_provider")
    @patch("providers.lyrics.get_lyrics_plus_provider")
    @patch("providers.lyrics.get_lrc_red_provider")
    def test_chain_falls_back_to_genius_when_first_five_return_none(
        self, mock_get_lrcred, mock_get_plus, mock_get_lrclib, mock_get_better, mock_get_mega, mock_get_genius
    ):
        mock_lrcred = MagicMock()
        mock_lrcred.get_lyrics.return_value = None
        mock_get_lrcred.return_value = mock_lrcred

        mock_plus = MagicMock()
        mock_plus.get_lyrics.return_value = None
        mock_get_plus.return_value = mock_plus

        mock_lrclib = MagicMock()
        mock_lrclib.get_lyrics.return_value = None
        mock_get_lrclib.return_value = mock_lrclib

        mock_mega = MagicMock()
        mock_mega.get_lyrics.return_value = None
        mock_get_mega.return_value = mock_mega

        mock_better = MagicMock()
        mock_better.get_lyrics.return_value = None
        mock_get_better.return_value = mock_better

        mock_genius = MagicMock()
        mock_genius.get_lyrics.return_value = {
            "source": "Genius",
            "sync_type": "none",
            "lines": [{"text": "Genius extreme fallback line", "start_ms": None}],
        }
        mock_get_genius.return_value = mock_genius

        res = providers.lyrics.get_lyrics({"title": "Test", "artist": "Artist"})
        self.assertEqual(res["source"], "Genius")
        mock_lrcred.get_lyrics.assert_called_once()
        mock_plus.get_lyrics.assert_called_once()
        mock_lrclib.get_lyrics.assert_called_once()
        mock_mega.get_lyrics.assert_called_once()
        mock_better.get_lyrics.assert_called_once()
        mock_genius.get_lyrics.assert_called_once()

    @patch("providers.lyrics.get_genius_provider")
    @patch("providers.lyrics.get_megalobiz_provider")
    @patch("providers.lyrics.get_better_lyrics_provider")
    @patch("providers.lyrics.get_lrclib_provider")
    @patch("providers.lyrics.get_lyrics_plus_provider")
    @patch("providers.lyrics.get_lrc_red_provider")
    def test_chain_falls_back_when_providers_raise_exceptions(
        self, mock_get_lrcred, mock_get_plus, mock_get_lrclib, mock_get_better, mock_get_mega, mock_get_genius
    ):
        mock_lrcred = MagicMock()
        mock_lrcred.get_lyrics.side_effect = RuntimeError("Crash in lrc.red")
        mock_get_lrcred.return_value = mock_lrcred

        mock_plus = MagicMock()
        mock_plus.get_lyrics.side_effect = RuntimeError("Crash in LyricsPlus")
        mock_get_plus.return_value = mock_plus

        mock_lrclib = MagicMock()
        mock_lrclib.get_lyrics.side_effect = RuntimeError("Crash in LRCLIB")
        mock_get_lrclib.return_value = mock_lrclib

        mock_better = MagicMock()
        mock_better.get_lyrics.side_effect = RuntimeError("Crash in BetterLyrics")
        mock_get_better.return_value = mock_better

        mock_mega = MagicMock()
        mock_mega.get_lyrics.side_effect = RuntimeError("Crash in Megalobiz")
        mock_get_mega.return_value = mock_mega

        mock_genius = MagicMock()
        mock_genius.get_lyrics.return_value = {
            "source": "Genius",
            "lines": [{"text": "Safe Genius fallback", "start_ms": None}],
        }
        mock_get_genius.return_value = mock_genius

        res = providers.lyrics.get_lyrics({"title": "Test", "artist": "Artist"})
        self.assertEqual(res["source"], "Genius")
        mock_lrcred.get_lyrics.assert_called_once()
        mock_plus.get_lyrics.assert_called_once()
        mock_lrclib.get_lyrics.assert_called_once()
        mock_better.get_lyrics.assert_called_once()
        mock_mega.get_lyrics.assert_called_once()
        mock_genius.get_lyrics.assert_called_once()

    @patch("providers.lyrics.get_genius_provider")
    @patch("providers.lyrics.get_megalobiz_provider")
    @patch("providers.lyrics.get_better_lyrics_provider")
    @patch("providers.lyrics.get_lrclib_provider")
    @patch("providers.lyrics.get_lyrics_plus_provider")
    @patch("providers.lyrics.get_lrc_red_provider")
    def test_chain_returns_none_when_all_six_providers_fail(
        self, mock_get_lrcred, mock_get_plus, mock_get_lrclib, mock_get_better, mock_get_mega, mock_get_genius
    ):
        mock_lrcred = MagicMock()
        mock_lrcred.get_lyrics.return_value = None
        mock_get_lrcred.return_value = mock_lrcred

        mock_plus = MagicMock()
        mock_plus.get_lyrics.return_value = None
        mock_get_plus.return_value = mock_plus

        mock_lrclib = MagicMock()
        mock_lrclib.get_lyrics.return_value = None
        mock_get_lrclib.return_value = mock_lrclib

        mock_better = MagicMock()
        mock_better.get_lyrics.return_value = None
        mock_get_better.return_value = mock_better

        mock_mega = MagicMock()
        mock_mega.get_lyrics.return_value = None
        mock_get_mega.return_value = mock_mega

        mock_genius = MagicMock()
        mock_genius.get_lyrics.return_value = None
        mock_get_genius.return_value = mock_genius

        res = providers.lyrics.get_lyrics({"title": "Test", "artist": "Artist"})
        self.assertIsNone(res)
        mock_lrcred.get_lyrics.assert_called_once()
        mock_plus.get_lyrics.assert_called_once()
        mock_lrclib.get_lyrics.assert_called_once()
        mock_better.get_lyrics.assert_called_once()
        mock_mega.get_lyrics.assert_called_once()
        mock_genius.get_lyrics.assert_called_once()


class TestProviderHealthAndDeadline(unittest.TestCase):
    def setUp(self):
        providers.lyrics.reset_provider_health()

    def tearDown(self):
        providers.lyrics.reset_provider_health()

    def test_provider_health_429_triggers_immediate_backoff(self):
        tracker = providers.lyrics.ProviderHealthTracker(rate_limit_backoff_sec=60.0)
        self.assertTrue(tracker.is_available("Megalobiz"))

        res = providers.lyrics.ProviderResult(
            error="Rate limit exceeded",
            status_code=429,
            provider_name="Megalobiz",
        )
        tracker.record_result(res)
        self.assertFalse(tracker.is_available("Megalobiz"))
        self.assertGreater(tracker.get_backoff_remaining("Megalobiz"), 0.0)

        # Reset clears backoff
        tracker.reset()
        self.assertTrue(tracker.is_available("Megalobiz"))

    def test_provider_health_consecutive_network_failures_trigger_backoff(self):
        tracker = providers.lyrics.ProviderHealthTracker(
            failure_threshold=2, base_backoff_sec=30.0
        )
        self.assertTrue(tracker.is_available("LRCLIB"))

        # 1st failure does not trigger backoff yet
        res1 = providers.lyrics.ProviderResult(
            error="connection_error",
            provider_name="LRCLIB",
        )
        tracker.record_result(res1)
        self.assertTrue(tracker.is_available("LRCLIB"))

        # 2nd failure hits threshold -> triggers backoff
        res2 = providers.lyrics.ProviderResult(
            error="connection_error",
            provider_name="LRCLIB",
        )
        tracker.record_result(res2)
        self.assertFalse(tracker.is_available("LRCLIB"))

    def test_provider_health_404_not_found_does_not_trigger_backoff(self):
        tracker = providers.lyrics.ProviderHealthTracker(failure_threshold=2)
        res = providers.lyrics.ProviderResult(
            error="not_found",
            status_code=404,
            provider_name="Genius",
        )
        for _ in range(5):
            tracker.record_result(res)
        self.assertTrue(tracker.is_available("Genius"))

    @patch("providers.lyrics.get_genius_provider")
    @patch("providers.lyrics.get_megalobiz_provider")
    @patch("providers.lyrics.get_better_lyrics_provider")
    @patch("providers.lyrics.get_lrclib_provider")
    @patch("providers.lyrics.get_lyrics_plus_provider")
    @patch("providers.lyrics.get_lrc_red_provider")
    def test_chain_skips_backed_off_provider(
        self,
        mock_get_lrcred,
        mock_get_plus,
        mock_get_lrclib,
        mock_get_better,
        mock_get_mega,
        mock_get_genius,
    ):
        mock_lrcred = MagicMock()
        mock_get_lrcred.return_value = mock_lrcred

        mock_plus = MagicMock()
        mock_plus.get_lyrics.return_value = {
            "source": "LyricsPlus",
            "lines": [{"text": "Found on LyricsPlus", "start_ms": 100}],
        }
        mock_get_plus.return_value = mock_plus

        # Put lrc.red into active backoff
        health = providers.lyrics.get_provider_health()
        health.record_result(
            providers.lyrics.ProviderResult(
                error="Rate limited", status_code=429, provider_name="lrc.red"
            )
        )
        self.assertFalse(health.is_available("lrc.red"))

        res = providers.lyrics.get_lyrics({"title": "Test", "artist": "Artist"})
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "LyricsPlus")
        # lrc.red was skipped completely!
        mock_lrcred.get_lyrics.assert_not_called()
        mock_plus.get_lyrics.assert_called_once()

    @patch("providers.lyrics.get_genius_provider")
    @patch("providers.lyrics.get_megalobiz_provider")
    @patch("providers.lyrics.get_better_lyrics_provider")
    @patch("providers.lyrics.get_lrclib_provider")
    @patch("providers.lyrics.get_lyrics_plus_provider")
    @patch("providers.lyrics.get_lrc_red_provider")
    def test_chain_aborts_when_retrieval_deadline_exceeded(
        self,
        mock_get_lrcred,
        mock_get_plus,
        mock_get_lrclib,
        mock_get_better,
        mock_get_mega,
        mock_get_genius,
    ):
        import time

        def slow_lrc_red(track):
            time.sleep(0.06)
            return None

        mock_lrcred = MagicMock()
        mock_lrcred.get_lyrics.side_effect = slow_lrc_red
        mock_get_lrcred.return_value = mock_lrcred

        mock_plus = MagicMock()
        mock_get_plus.return_value = mock_plus

        # Set a very tight deadline (0.05s)
        res = providers.lyrics.get_lyrics(
            {"title": "Test", "artist": "Artist"},
            deadline_seconds=0.05,
        )
        self.assertIsNone(res)
        mock_lrcred.get_lyrics.assert_called_once()
        # LyricsPlus was not called because deadline expired during lrc.red!
        mock_plus.get_lyrics.assert_not_called()


class TestCleanTitleForSearch(unittest.TestCase):
    def test_clean_video_and_audio_tags(self):
        self.assertEqual(
            clean_title_for_search("Levitating (Official Music Video)"),
            "Levitating",
        )
        self.assertEqual(
            clean_title_for_search("Starboy [Official Video]"),
            "Starboy",
        )
        self.assertEqual(
            clean_title_for_search("Song Title (Lyric Video)"),
            "Song Title",
        )
        self.assertEqual(
            clean_title_for_search("Another Song - Official Audio"),
            "Another Song",
        )
        self.assertEqual(
            clean_title_for_search("Track (Remastered)"),
            "Track",
        )

    def test_clean_title_unchanged(self):
        self.assertEqual(clean_title_for_search("Bohemian Rhapsody"), "Bohemian Rhapsody")
        self.assertEqual(clean_title_for_search(""), "")


class TestParseQrc(unittest.TestCase):
    def test_parse_empty_or_whitespace(self):
        self.assertEqual(parse_qrc(""), ("none", []))
        self.assertEqual(parse_qrc("   "), ("none", []))

    def test_parse_standard_qrc_words(self):
        qrc = (
            "[1000,2000]Hello (1000,500)world (1500,500)\n"
            "[3000,2000]How (3000,600)are (3600,400)you (4000,1000)"
        )
        sync_type, lines = parse_qrc(qrc)
        self.assertEqual(sync_type, "word")
        self.assertEqual(len(lines), 2)

        line0 = lines[0]
        self.assertEqual(line0["start_ms"], 1000)
        self.assertEqual(line0["end_ms"], 3000)
        self.assertEqual(line0["text"], "Hello world")
        self.assertEqual(len(line0["words"]), 2)
        self.assertEqual(line0["words"][0]["text"], "Hello")
        self.assertEqual(line0["words"][0]["start_ms"], 1000)
        self.assertEqual(line0["words"][0]["end_ms"], 1500)
        self.assertEqual(line0["words"][1]["text"], "world")
        self.assertEqual(line0["words"][1]["start_ms"], 1500)
        self.assertEqual(line0["words"][1]["end_ms"], 2000)

    def test_parse_qrc_xml_wrapped(self):
        xml_qrc = (
            '<?xml version="1.0" encoding="utf-8"?>\n'
            '<QrcInfos>\n'
            '<Lyric_1 LyricType="1" LyricContent="[ti:Test]\n'
            '[offset:0]\n'
            '[16392,2152]I\'m (16392,264)tryna (16656,208)put (16864,178)you (17042,181)"/>\n'
            '</QrcInfos>'
        )
        sync_type, lines = parse_qrc(xml_qrc)
        self.assertEqual(sync_type, "word")
        self.assertEqual(len(lines), 1)
        self.assertEqual(lines[0]["text"], "I'm tryna put you")
        self.assertEqual(lines[0]["start_ms"], 16392)
        self.assertEqual(lines[0]["end_ms"], 16392 + 2152)

    def test_parse_qrc_background_vocals(self):
        qrc = "[5000,1500]((5000,750)yeah (5750,750))"
        sync_type, lines = parse_qrc(qrc)
        self.assertEqual(len(lines), 1)
        self.assertTrue(lines[0]["is_background"])


class TestBetterLyricsProvider(unittest.TestCase):
    def setUp(self):
        self.mock_session = MagicMock()
        self.provider = BetterLyricsProvider(session=self.mock_session)

    def test_name_and_attribution(self):
        self.assertEqual(self.provider.name, "BetterLyrics")
        self.assertIn("BetterLyrics", self.provider.attribution)

    def test_missing_title_returns_none(self):
        self.assertIsNone(self.provider.get_lyrics({}))
        self.assertIsNone(self.provider.get_lyrics({"title": ""}))

    def test_primary_endpoint_success_ttml(self):
        ttml_content = (
            '<tt xmlns="http://www.w3.org/ns/ttml" xmlns:itunes="http://music.apple.com/lyric-ttml-internal">'
            '<body><div><p begin="00:09.50" end="00:11.70">'
            '<span>If </span><span>you </span><span>wanna</span>'
            '</p></div></body></tt>'
        )
        mock_resp = MagicMock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {"ttml": ttml_content}
        self.mock_session.get.return_value = mock_resp

        track = {
            "title": "Levitating",
            "artist": "Dua Lipa",
            "duration_ms": 203000,
            "album": "Future Nostalgia",
        }
        res = self.provider.get_lyrics(track)
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "BetterLyrics")
        self.assertEqual(res["title"], "Levitating")
        self.assertEqual(res["artist"], "Dua Lipa")
        self.assertEqual(len(res["lines"]), 1)
        self.assertEqual(res["lines"][0]["text"], "If you wanna")

    def test_fallback_to_portato_when_primary_fails(self):
        # First call (BASE) returns 401
        base_resp = MagicMock()
        base_resp.status_code = 401
        base_resp.json.return_value = {"error": "API key required"}

        # Second call (PORTATO) returns 200 with QRC
        portato_resp = MagicMock()
        portato_resp.status_code = 200
        portato_resp.json.return_value = {
            "lyrics": "[1000,2000]Starboy (1000,1000)rocks (2000,1000)",
            "provider": "qq",
        }

        self.mock_session.get.side_effect = [base_resp, portato_resp]

        track = {
            "title": "Starboy",
            "artist": "The Weeknd",
            "duration_ms": 230000,
        }
        res = self.provider.get_lyrics(track)
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "BetterLyrics (QQ)")
        self.assertEqual(len(res["lines"]), 1)
        self.assertEqual(res["lines"][0]["text"], "Starboy rocks")

    def test_both_endpoints_401_returns_none(self):
        base_resp = MagicMock()
        base_resp.status_code = 401
        portato_resp = MagicMock()
        portato_resp.status_code = 401

        self.mock_session.get.side_effect = [base_resp, portato_resp]

        track = {"title": "Unknown Song", "artist": "Unknown Artist", "duration_ms": 180000}
        self.assertIsNone(self.provider.get_lyrics(track))

    def test_request_exception_handled_gracefully(self):
        import requests
        self.mock_session.get.side_effect = requests.RequestException("Network failed")
        track = {"title": "Any Song", "artist": "Any Artist"}
        self.assertIsNone(self.provider.get_lyrics(track))


class TestGeniusHelpers(unittest.TestCase):
    def test_clean_query(self):
        self.assertEqual(
            clean_query("Dua Lipa - Levitating (Official Music Video) [4K]"),
            "Dua Lipa - Levitating",
        )
        self.assertEqual(
            clean_query("Song prod. by Metro Boomin"),
            "Song",
        )
        self.assertEqual(
            clean_query("★ Starboy ~"),
            "Starboy",
        )
        self.assertEqual(
            clean_query("Track Name | Album Title"),
            "Track Name",
        )

    def test_strip_artifacts(self):
        raw = "Lyrics\u00a0line\u200bwith\ufeffspaces\u2005here\n12You might also like\nMore lyrics\n123Embed"
        cleaned = strip_artifacts(raw)
        self.assertNotIn("You might also like", cleaned)
        self.assertNotIn("Embed", cleaned)
        self.assertNotIn("\u00a0", cleaned)
        self.assertNotIn("\u200b", cleaned)
        self.assertNotIn("\ufeff", cleaned)
        self.assertIn("Lyrics line with spaces here", cleaned)
        self.assertIn("More lyrics", cleaned)

    def test_is_section_header(self):
        self.assertTrue(is_section_header("[Verse 1]"))
        self.assertTrue(is_section_header("[Chorus]"))
        self.assertTrue(is_section_header("[Bridge: Dua Lipa]"))
        self.assertTrue(is_section_header("[Intro]"))
        self.assertFalse(is_section_header("I had a premonition"))
        self.assertFalse(is_section_header("[]"))
        self.assertFalse(is_section_header("[ ]"))

    def test_text_to_lyric_lines(self):
        text = "\n\n[Verse 1]\nLine 1\n(Backing vocal)\n\n\n[Chorus]\nChorus line\n\n"
        lines = text_to_lyric_lines(text)
        self.assertEqual(len(lines), 6)
        # Leading and trailing empty lines are dropped
        self.assertEqual(lines[0]["text"], "[Verse 1]")
        self.assertFalse(lines[0]["is_background"])
        self.assertIsNone(lines[0]["start_ms"])
        self.assertEqual(lines[1]["text"], "Line 1")
        self.assertFalse(lines[1]["is_background"])
        self.assertEqual(lines[2]["text"], "(Backing vocal)")
        self.assertTrue(lines[2]["is_background"])
        # Gap between stanzas
        self.assertEqual(lines[3]["text"], "")
        self.assertEqual(lines[4]["text"], "[Chorus]")
        self.assertEqual(lines[5]["text"], "Chorus line")

    def test_parse_genius_html_modern_container(self):
        html_doc = """
        <div data-lyrics-container="true">
            <div class="LyricsHeader__Container">Header to discard</div>
            [Verse 1]<br>
            If you wanna run away with me<br>
            <div data-exclude-from-selection="true">14 Contributors</div>
            I know a galaxy<br>
            12You might also like
        </div>
        <div data-lyrics-container="true">
            [Chorus]<br>
            I had a premonition<br>
            123Embed
        </div>
        """
        lines = parse_genius_html(html_doc)
        self.assertIsNotNone(lines)
        texts = [l["text"] for l in lines]
        self.assertIn("[Verse 1]", texts)
        self.assertIn("If you wanna run away with me", texts)
        self.assertIn("I know a galaxy", texts)
        self.assertIn("[Chorus]", texts)
        self.assertIn("I had a premonition", texts)
        self.assertNotIn("Header to discard", "".join(texts))
        self.assertNotIn("14 Contributors", "".join(texts))
        self.assertNotIn("You might also like", "".join(texts))
        self.assertNotIn("Embed", "".join(texts))

    def test_parse_genius_html_old_container(self):
        html_doc = """
        <div class="lyrics">
            [Verse 1]<br>
            Old container line 1<br>
            Old container line 2
        </div>
        """
        lines = parse_genius_html(html_doc)
        self.assertIsNotNone(lines)
        texts = [l["text"] for l in lines]
        self.assertIn("[Verse 1]", texts)
        self.assertIn("Old container line 1", texts)
        self.assertIn("Old container line 2", texts)

    def test_best_match_penalizes_translations_and_tracklists(self):
        candidates = [
            {
                "title": "Levitating (Türkçe Çeviri)",
                "artist_names": "Dua Lipa",
                "path": "/Dua-lipa-levitating-turkce-ceviri-lyrics",
                "url": "https://genius.com/translation",
            },
            {
                "title": "Future Nostalgia Tracklist",
                "artist_names": "Dua Lipa",
                "path": "/Dua-lipa-future-nostalgia-tracklist-lyrics",
                "url": "https://genius.com/tracklist",
            },
            {
                "title": "Levitating",
                "artist_names": "Dua Lipa",
                "path": "/Dua-lipa-levitating-lyrics",
                "url": "https://genius.com/canonical",
            },
        ]
        match = best_match(candidates, "Levitating", "Dua Lipa")
        self.assertIsNotNone(match)
        self.assertEqual(match["url"], "https://genius.com/canonical")


class TestGeniusProvider(unittest.TestCase):
    def setUp(self):
        self.mock_session = MagicMock()
        self.provider = GeniusProvider(session=self.mock_session)

    def test_provider_metadata(self):
        self.assertEqual(self.provider.name, "Genius")
        self.assertEqual(self.provider.attribution, DEFAULT_GENIUS_ATTRIBUTION)
        headers = self.provider._headers()
        self.assertEqual(headers["User-Agent"], DEFAULT_GENIUS_USER_AGENT)

    def test_get_lyrics_success(self):
        # 1. Multi-search response
        search_resp = MagicMock()
        search_resp.status_code = 200
        search_resp.json.return_value = {
            "response": {
                "sections": [
                    {
                        "type": "song",
                        "hits": [
                            {
                                "result": {
                                    "title": "Levitating",
                                    "artist_names": "Dua Lipa",
                                    "path": "/Dua-lipa-levitating-lyrics",
                                    "url": "https://genius.com/Dua-lipa-levitating-lyrics",
                                }
                            }
                        ],
                    }
                ]
            }
        }

        # 2. Page HTML response
        page_resp = MagicMock()
        page_resp.status_code = 200
        page_resp.text = """
        <div data-lyrics-container="true">
            [Verse 1]<br>
            If you wanna run away with me, I know a galaxy<br>
            And I can take you for a ride<br>
            123Embed
        </div>
        """

        self.mock_session.get.side_effect = [search_resp, page_resp]

        track = {
            "title": "Levitating (Official Video)",
            "artist": "Dua Lipa",
            "album": "Future Nostalgia",
            "duration_ms": 203000,
        }
        res = self.provider.get_lyrics(track)
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "Genius")
        self.assertEqual(res["sync_type"], "none")
        self.assertEqual(res["title"], "Levitating")
        self.assertEqual(res["artist"], "Dua Lipa")
        self.assertEqual(res["url"], "https://genius.com/Dua-lipa-levitating-lyrics")
        self.assertTrue(len(res["lines"]) >= 3)
        self.assertEqual(res["lines"][0]["text"], "[Verse 1]")
        self.assertIsNone(res["lines"][0]["start_ms"])
        self.assertFalse(res["lines"][0]["is_background"])

    def test_get_lyrics_no_search_hits(self):
        search_resp = MagicMock()
        search_resp.status_code = 200
        search_resp.json.return_value = {
            "response": {
                "sections": [{"type": "song", "hits": []}]
            }
        }
        self.mock_session.get.return_value = search_resp
        track = {"title": "Unknown Song Title", "artist": "Unknown Artist"}
        self.assertIsNone(self.provider.get_lyrics(track))

    def test_get_lyrics_network_error(self):
        import requests
        self.mock_session.get.side_effect = requests.RequestException("Timeout connecting to Genius")
        track = {"title": "Levitating", "artist": "Dua Lipa"}
        self.assertIsNone(self.provider.get_lyrics(track))


class TestMegalobizProvider(unittest.TestCase):
    def setUp(self):
        self.mock_session = MagicMock()
        self.provider = MegalobizProvider(session=self.mock_session)

    def test_provider_metadata(self):
        self.assertEqual(self.provider.name, "Megalobiz")
        self.assertEqual(self.provider.attribution, DEFAULT_MEGALOBIZ_ATTRIBUTION)
        headers = self.provider._headers()
        self.assertIn("ANSImuz", headers["User-Agent"])
        self.assertNotIn("X-API-Key", headers)

        key_provider = MegalobizProvider(api_key="mbz_secret_key", session=self.mock_session)
        key_headers = key_provider._headers()
        self.assertEqual(key_headers["X-API-Key"], "mbz_secret_key")

    def test_score_candidate(self):
        cand1 = {"title": "Levitating", "score": 1.0, "duration": 202.87, "duration_diff": 0.13}
        cand2 = {"title": "Levitating (Remix)", "score": 0.9, "duration": 202.87, "duration_diff": 0.13}
        cand3 = {"title": "Levitating", "score": 1.0, "duration": 260.0, "duration_diff": 57.0}

        score1 = megalobiz_score_candidate(cand1, "Levitating", "Dua Lipa", 203.0)
        score2 = megalobiz_score_candidate(cand2, "Levitating", "Dua Lipa", 203.0)
        score3 = megalobiz_score_candidate(cand3, "Levitating", "Dua Lipa", 203.0)

        # Canonical track should outscore unwanted remix and wrong duration
        self.assertGreater(score1, score2)
        self.assertGreater(score1, score3)

    def test_get_lyrics_success_with_lrc_string(self):
        search_resp = MagicMock()
        search_resp.status_code = 200
        search_resp.json.return_value = {
            "count": 1,
            "results": [
                {
                    "id": 194905,
                    "title": "Levitating",
                    "artist": "Dua Lipa",
                    "duration": 202.87,
                    "duration_diff": 0.13,
                    "score": 1.0,
                    "url": "https://megalobiz.com/lrc/194905/levitating",
                }
            ],
        }

        sheet_resp = MagicMock()
        sheet_resp.status_code = 200
        sheet_resp.json.return_value = {
            "id": 194905,
            "title": "Levitating",
            "artist": "Dua Lipa",
            "duration": 202.87,
            "url": "https://megalobiz.com/lrc/194905/levitating",
            "lrc": "[00:08.87]If you wanna run away with me, I know a galaxy\n[00:11.26]And I can take you for a ride",
        }

        self.mock_session.get.side_effect = [search_resp, sheet_resp]

        track = {
            "title": "Levitating",
            "artist": "Dua Lipa",
            "album": "Future Nostalgia",
            "duration_ms": 203000,
        }
        res = self.provider.get_lyrics(track)
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "Megalobiz")
        self.assertEqual(res["sync_type"], "line")
        self.assertEqual(res["title"], "Levitating")
        self.assertEqual(res["artist"], "Dua Lipa")
        self.assertEqual(res["url"], "https://megalobiz.com/lrc/194905/levitating")
        self.assertEqual(res["attribution"], DEFAULT_MEGALOBIZ_ATTRIBUTION)
        self.assertEqual(len(res["lines"]), 2)
        self.assertEqual(res["lines"][0]["text"], "If you wanna run away with me, I know a galaxy")
        self.assertEqual(res["lines"][0]["start_ms"], 8870)
        self.assertEqual(res["lines"][0]["end_ms"], 11260)
        self.assertEqual(res["lines"][1]["text"], "And I can take you for a ride")
        self.assertEqual(res["lines"][1]["start_ms"], 11260)

    def test_get_lyrics_success_with_lines_array(self):
        search_resp = MagicMock()
        search_resp.status_code = 200
        search_resp.json.return_value = {
            "count": 1,
            "results": [
                {
                    "id": 88888,
                    "title": "Starboy",
                    "artist": "The Weeknd",
                    "duration": 230.5,
                    "score": 0.99,
                    "url": "https://megalobiz.com/lrc/88888/starboy",
                }
            ],
        }

        sheet_resp = MagicMock()
        sheet_resp.status_code = 200
        sheet_resp.json.return_value = {
            "id": 88888,
            "title": "Starboy",
            "artist": "The Weeknd",
            "duration": 230.5,
            "url": "https://megalobiz.com/lrc/88888/starboy",
            "lrc": None,
            "lines": [
                {"time_ms": 16390, "text": "I'm tryna put you in that worst mood, ah"},
                {"time_ms": 19500, "text": "P1 cleaner than your church shoes, ah"},
            ],
        }

        self.mock_session.get.side_effect = [search_resp, sheet_resp]

        track = {"title": "Starboy", "artist": "The Weeknd", "duration_ms": 230000}
        res = self.provider.get_lyrics(track)
        self.assertIsNotNone(res)
        self.assertEqual(res["source"], "Megalobiz")
        self.assertEqual(res["sync_type"], "line")
        self.assertEqual(len(res["lines"]), 2)
        self.assertEqual(res["lines"][0]["start_ms"], 16390)
        self.assertEqual(res["lines"][0]["end_ms"], 19500)
        self.assertEqual(res["lines"][1]["start_ms"], 19500)

    def test_get_lyrics_unescapes_html_entities(self):
        search_resp = MagicMock()
        search_resp.status_code = 200
        search_resp.json.return_value = {
            "count": 1,
            "results": [
                {
                    "id": 11111,
                    "title": "Sweet Child O&#039; Mine",
                    "artist": "Guns N&amp; Roses",
                    "score": 1.0,
                    "url": "https://megalobiz.com/lrc/11111/sweet-child",
                }
            ],
        }

        sheet_resp = MagicMock()
        sheet_resp.status_code = 200
        sheet_resp.json.return_value = {
            "id": 11111,
            "title": "Sweet Child O&#039; Mine",
            "artist": "Guns N&amp; Roses",
            "url": "https://megalobiz.com/lrc/11111/sweet-child",
            "lrc": "[00:10.00]She&#039;s got a smile that it seems to me\n[00:15.00]Reminds me of childhood memories &amp; love",
        }

        self.mock_session.get.side_effect = [search_resp, sheet_resp]

        track = {"title": "Sweet Child O' Mine", "artist": "Guns N& Roses"}
        res = self.provider.get_lyrics(track)
        self.assertIsNotNone(res)
        self.assertEqual(res["title"], "Sweet Child O' Mine")
        self.assertEqual(res["artist"], "Guns N& Roses")
        self.assertEqual(res["lines"][0]["text"], "She's got a smile that it seems to me")
        self.assertEqual(res["lines"][1]["text"], "Reminds me of childhood memories & love")

    def test_get_lyrics_no_search_hits_returns_none(self):
        search_resp = MagicMock()
        search_resp.status_code = 200
        search_resp.json.return_value = {"count": 0, "results": []}
        self.mock_session.get.return_value = search_resp

        track = {"title": "Nonexistent Track Title", "artist": "Unknown Artist"}
        self.assertIsNone(self.provider.get_lyrics(track))

    def test_get_lyrics_rate_limit_handled(self):
        search_resp = MagicMock()
        search_resp.status_code = 429
        self.mock_session.get.return_value = search_resp

        track = {"title": "Levitating", "artist": "Dua Lipa"}
        self.assertIsNone(self.provider.get_lyrics(track))

    def test_get_lyrics_network_error_handled(self):
        import requests
        self.mock_session.get.side_effect = requests.RequestException("Timeout connecting to Megalobiz")

        track = {"title": "Levitating", "artist": "Dua Lipa"}
        self.assertIsNone(self.provider.get_lyrics(track))


class TestTieredLyricsCancellation(unittest.TestCase):
    def test_cancellation_callback_aborts_before_first_provider(self):
        track = {"title": "Test Track", "artist": "Test Artist"}
        is_cancelled = MagicMock(return_value=True)

        with patch("providers.lyrics.get_lrc_red_provider") as mock_lrc:
            res = providers.lyrics.get_lyrics(track, is_cancelled=is_cancelled)
            self.assertIsNone(res)
            mock_lrc.assert_not_called()

    def test_cancellation_callback_aborts_between_providers(self):
        track = {"title": "Test Track", "artist": "Test Artist"}
        call_count = 0

        def check_cancelled():
            nonlocal call_count
            call_count += 1
            return call_count >= 2

        with patch("providers.lyrics.get_lrc_red_provider") as mock_lrc, \
             patch("providers.lyrics.get_lyrics_plus_provider") as mock_lp:
            mock_lrc_inst = MagicMock()
            mock_lrc_inst.get_lyrics.return_value = None
            mock_lrc.return_value = mock_lrc_inst

            res = providers.lyrics.get_lyrics(track, is_cancelled=check_cancelled)
            self.assertIsNone(res)
            mock_lrc_inst.get_lyrics.assert_called_once()
            mock_lp.assert_not_called()


if __name__ == "__main__":
    unittest.main()