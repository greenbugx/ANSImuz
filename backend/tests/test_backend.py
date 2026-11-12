import os
import sys
import unittest
from unittest.mock import MagicMock, patch

# Ensure backend directory is in sys.path
BACKEND_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
if BACKEND_DIR not in sys.path:
    sys.path.insert(0, BACKEND_DIR)


class TestBackendImports(unittest.TestCase):
    def test_backend_modules_import_cleanly(self):
        from artwork import thumbnails
        import bridge
        from providers import spotify, ytmusic

        self.assertIsNotNone(bridge)
        self.assertIsNotNone(spotify)
        self.assertIsNotNone(ytmusic)
        self.assertIsNotNone(thumbnails)

    def test_spotify_provider_lazy_initializes_ytmusic(self):
        from providers.spotify import spotify

        # Reset global state for testing
        spotify._ytmusic = None
        self.assertIsNone(spotify._ytmusic, "_ytmusic must be None at import time")

        # Calling _get_ytmusic lazily instantiates it
        with patch("providers.spotify.spotify.YTMusic") as mock_ytm:
            instance = mock_ytm.return_value
            client = spotify._get_ytmusic()
            self.assertEqual(client, instance)
            self.assertEqual(spotify._ytmusic, instance)

        # Subsequent call reuses the lazy instance
        client2 = spotify._get_ytmusic()
        self.assertEqual(client2, instance)


class TestBridgeSmoke(unittest.TestCase):
    def test_bridge_resolve_playable_preserves_resolved_track(self):
        import bridge

        track = {
            "title": "Already Resolved",
            "artist": "Artist",
            "stream_url": "https://music.youtube.com/watch?v=123",
        }
        res = bridge.resolve_playable(track)
        self.assertEqual(res["stream_url"], "https://music.youtube.com/watch?v=123")

    @patch("bridge.spotify.search")
    @patch("bridge.ytmusic.search")
    def test_bridge_search_music_routing(self, mock_yt_search, mock_sp_search):
        import bridge

        mock_sp_search.return_value = [{"title": "Sp Track"}]
        mock_yt_search.return_value = [{"title": "Yt Track"}]

        sp_res = bridge.search_music("test", provider="spotify")
        self.assertEqual(sp_res, [{"title": "Sp Track"}])
        mock_sp_search.assert_called_once_with("test")

        yt_res = bridge.search_music("test", provider="ytmusic")
        self.assertEqual(yt_res, [{"title": "Yt Track"}])
        mock_yt_search.assert_called_once_with("test")


class TestSpotifyResolutionErrorReporting(unittest.TestCase):
    def test_resolve_missing_required_fields(self):
        from providers.spotify import spotify

        with self.assertRaises(ValueError) as ctx:
            spotify.resolve_playable({})
        self.assertIn("spotify_id", str(ctx.exception))

        with self.assertRaises(ValueError) as ctx:
            spotify.resolve_playable({"spotify_id": "sp123"})
        self.assertIn("title", str(ctx.exception))

    @patch("providers.spotify.spotify._get_ytmusic")
    def test_resolve_reports_diagnostic_rejection_reason(self, mock_get_yt):
        from providers.spotify import spotify

        mock_yt = MagicMock()
        mock_get_yt.return_value = mock_yt

        # Candidate returned has duration mismatch and version conflict
        mock_yt.search.return_value = [
            {
                "videoId": "yt_remix",
                "title": "Song Title (Remix)",
                "artists": [{"name": "Different Artist"}],
                "duration": "10:00",
                "duration_seconds": 600,
            }
        ]

        track = {
            "spotify_id": "sp_test_id_unique",
            "title": "Song Title",
            "artist": "Original Artist",
            "artists": ["Original Artist"],
            "duration_ms": 180000,  # 3 minutes
        }

        with self.assertRaises(LookupError) as ctx:
            spotify.resolve_playable(track)

        err_msg = str(ctx.exception)
        self.assertIn("No confident YouTube Music match", err_msg)
        # Verify improved diagnostic details were reported
        self.assertTrue(
            "version conflict" in err_msg or "duration" in err_msg or "Reason:" in err_msg,
            f"Expected diagnostic details in error, got: {err_msg}",
        )

    @patch("providers.spotify.spotify._get_ytmusic")
    def test_resolve_handles_all_search_attempts_raising_exceptions(self, mock_get_yt):
        from providers.spotify import spotify

        mock_yt = MagicMock()
        mock_get_yt.return_value = mock_yt
        mock_yt.search.side_effect = RuntimeError("Network connection unreachable")

        track = {
            "spotify_id": "sp_fail_queries_all",
            "title": "Unreachable Track",
            "artist": "Artist",
            "artists": ["Artist"],
            "duration_ms": 200000,
        }

        with self.assertRaises(LookupError) as ctx:
            spotify.resolve_playable(track)

        err_msg = str(ctx.exception)
        self.assertIn("No confident YouTube Music match", err_msg)
        self.assertIn("Search errors:", err_msg)
        self.assertIn("Network connection unreachable", err_msg)

class TestProviderErrorClassification(unittest.TestCase):
    def test_error_hierarchy(self):
        from exceptions import (
            AuthenticationError,
            NetworkError,
            ProviderError,
            RateLimitError,
            TrackNotFoundError,
        )

        self.assertTrue(issubclass(NetworkError, ProviderError))
        self.assertTrue(issubclass(RateLimitError, ProviderError))
        self.assertTrue(issubclass(AuthenticationError, ProviderError))
        self.assertTrue(issubclass(TrackNotFoundError, ProviderError))
        self.assertTrue(issubclass(TrackNotFoundError, LookupError))

    def test_classify_network_error_with_chaining(self):
        from exceptions import NetworkError, classify_error

        orig = ConnectionResetError("Connection reset by peer")
        try:
            raise classify_error("Spotify search", orig) from orig
        except NetworkError as err:
            self.assertIs(err.__cause__, orig)
            self.assertIn("network connection error", str(err))

    def test_classify_rate_limit_error_with_chaining(self):
        from exceptions import RateLimitError, classify_error

        orig = RuntimeError("HTTP 429 Too Many Requests")
        try:
            raise classify_error("YT Music search", orig) from orig
        except RateLimitError as err:
            self.assertIs(err.__cause__, orig)
            self.assertIn("rate limit exceeded", str(err))

    def test_classify_authentication_error_with_chaining(self):
        from exceptions import AuthenticationError, classify_error

        orig = RuntimeError("HTTP 401 Unauthorized")
        try:
            raise classify_error("Provider call", orig) from orig
        except AuthenticationError as err:
            self.assertIs(err.__cause__, orig)
            self.assertIn("authentication failed", str(err))

    @patch("providers.spotify.spotify.SpotifyClient")
    def test_spotify_search_raises_chained_network_error(self, mock_client_cls):
        from exceptions import NetworkError
        from providers.spotify import spotify

        mock_client = MagicMock()
        mock_client.__enter__.return_value = mock_client
        mock_client.search.side_effect = ConnectionRefusedError("Connection refused by peer")
        mock_client_cls.return_value = mock_client

        with self.assertRaises(NetworkError) as ctx:
            spotify.search("test song")
        self.assertIsInstance(ctx.exception.__cause__, ConnectionRefusedError)

    @patch("providers.ytmusic.ytmusic.YTMusic")
    def test_ytmusic_search_raises_chained_network_error(self, mock_yt_cls):
        from exceptions import NetworkError
        from providers.ytmusic import ytmusic

        mock_yt = MagicMock()
        mock_yt.search.side_effect = TimeoutError("Socket timed out")
        mock_yt_cls.return_value = mock_yt

        with self.assertRaises(NetworkError) as ctx:
            ytmusic.search("test song")
        self.assertIsInstance(ctx.exception.__cause__, TimeoutError)

    @patch("bridge.spotify.search")
    def test_bridge_search_music_propagates_provider_error(self, mock_sp_search):
        import bridge
        from exceptions import RateLimitError

        mock_sp_search.side_effect = RateLimitError("Spotify rate limit exceeded")

        with self.assertRaises(RateLimitError):
            bridge.search_music("test", provider="spotify")

class TestArtworkThumbnails(unittest.TestCase):
    def test_cache_path_calculation(self):
        from artwork import thumbnails

        path = thumbnails.cache_path("https://img.youtube.com/vi/abc/hqdefault.jpg")
        self.assertTrue(path.endswith(".img"))
        self.assertIn("ansimuz", path)

    def test_is_gray_placeholder(self):
        from PIL import Image
        from artwork import thumbnails

        gray_img = Image.new("RGB", (120, 90), (128, 128, 128))
        self.assertTrue(thumbnails._is_gray_placeholder(gray_img))

        color_img = Image.new("RGB", (120, 90), (255, 0, 0))
        self.assertFalse(thumbnails._is_gray_placeholder(color_img))


class TestStandardizedTrackMetadata(unittest.TestCase):
    @patch("providers.ytmusic.ytmusic.YTMusic")
    def test_ytmusic_standardized_metadata_with_and_without_art(self, mock_yt_cls):
        from providers.ytmusic import ytmusic

        mock_yt = MagicMock()
        mock_yt_cls.return_value = mock_yt
        mock_yt.search.return_value = [
            {
                "videoId": "vid_full",
                "title": "Night Falls",
                "artists": [{"name": "Jody Wisternoff"}, {"name": "Booka Shade"}],
                "album": {"name": "Night Falls Album"},
                "duration": "4:20",
                "duration_seconds": 260,
                "thumbnails": [
                    {"url": "https://img.example.com/small.jpg", "width": 100, "height": 100},
                    {"url": "https://img.example.com/large.jpg", "width": 500, "height": 500},
                ],
            },
            {
                "videoId": "vid_no_art",
                "title": "No Art Track",
                "artists": [{"name": "Solo Artist"}],
                "album": "Single Album",
                "duration": "3:15",
                "thumbnails": [],
            },
            {
                "videoId": None,
                "title": "Missing Video ID",
                "artists": [],
                "album": None,
                "thumbnails": [],
            },
        ]

        tracks = ytmusic.search("night falls")
        self.assertEqual(len(tracks), 3)

        # Full track
        t1 = tracks[0]
        self.assertEqual(t1["provider"], "ytmusic")
        self.assertEqual(t1["track_id"], "vid_full")
        self.assertEqual(t1["title"], "Night Falls")
        self.assertEqual(t1["artist"], "Jody Wisternoff, Booka Shade")
        self.assertEqual(t1["album"], "Night Falls Album")
        self.assertEqual(t1["art_url"], "https://img.youtube.com/vi/vid_full/maxresdefault.jpg")
        self.assertEqual(t1["stream_url"], "https://youtube.com/watch?v=vid_full")
        self.assertEqual(t1["duration_ms"], 260000)

        # Track with no artwork from search (falls back to maxresdefault or None)
        t2 = tracks[1]
        self.assertEqual(t2["provider"], "ytmusic")
        self.assertEqual(t2["track_id"], "vid_no_art")
        self.assertEqual(t2["album"], "Single Album")
        self.assertEqual(t2["art_url"], "https://img.youtube.com/vi/vid_no_art/maxresdefault.jpg")
        self.assertEqual(t2["duration_ms"], 195000)

        # Track with no video_id and no artwork
        t3 = tracks[2]
        self.assertEqual(t3["provider"], "ytmusic")
        self.assertIsNone(t3["track_id"])
        self.assertIsNone(t3["album"])
        self.assertIsNone(t3["art_url"])
        self.assertIsNone(t3["stream_url"])
        self.assertIsNone(t3["duration_ms"])

    @patch("providers.spotify.spotify.SpotifyClient")
    def test_spotify_standardized_metadata_with_and_without_art(self, mock_client_cls):
        from providers.spotify import spotify

        mock_client = MagicMock()
        mock_client.__enter__.return_value = mock_client
        mock_client_cls.return_value = mock_client

        # Mock track objects returned by spotify_scraper
        artist_a = MagicMock()
        artist_a.name = "Artist A"
        album_a = MagicMock()
        album_a.name = "Album A"

        item_full = MagicMock()
        item_full.name = "Spotify Track"
        item_full.id = "sp_123"
        item_full.url = "https://open.spotify.com/track/sp_123"
        item_full.duration_ms = 180000
        item_full.artists = [artist_a]
        item_full.album = album_a
        item_full.images = [
            {"url": "https://i.scdn.co/image/large", "width": 640, "height": 640},
            {"url": "https://i.scdn.co/image/small", "width": 300, "height": 300},
        ]

        artist_b = MagicMock()
        artist_b.name = "Artist B"

        item_no_art = MagicMock()
        item_no_art.name = "No Art Track"
        item_no_art.id = "sp_456"
        item_no_art.url = "https://open.spotify.com/track/sp_456"
        item_no_art.duration_ms = 120000
        item_no_art.artists = [artist_b]
        item_no_art.album = None
        item_no_art.images = []

        response = MagicMock()
        response.tracks = [item_full, item_no_art]
        mock_client.search.return_value = response

        tracks = spotify.search("spotify track")
        self.assertEqual(len(tracks), 2)

        t1 = tracks[0]
        self.assertEqual(t1["provider"], "spotify")
        self.assertEqual(t1["track_id"], "sp_123")
        self.assertEqual(t1["title"], "Spotify Track")
        self.assertEqual(t1["artist"], "Artist A")
        self.assertEqual(t1["album"], "Album A")
        self.assertEqual(t1["art_url"], "https://i.scdn.co/image/large")
        self.assertIsNone(t1["stream_url"])  # lazy resolution

        t2 = tracks[1]
        self.assertEqual(t2["provider"], "spotify")
        self.assertEqual(t2["track_id"], "sp_456")
        self.assertEqual(t2["title"], "No Art Track")
        self.assertIsNone(t2["album"])
        self.assertIsNone(t2["art_url"])
        self.assertIsNone(t2["stream_url"])


if __name__ == "__main__":
    unittest.main()
