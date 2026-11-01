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


if __name__ == "__main__":
    unittest.main()
