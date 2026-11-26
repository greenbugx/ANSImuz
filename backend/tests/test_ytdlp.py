import importlib.metadata
import importlib.util
import unittest

HAS_YTDLP = importlib.util.find_spec("yt_dlp") is not None

@unittest.skipUnless(HAS_YTDLP, "yt_dlp not importable")
class TestYtdlpBaseline(unittest.TestCase):
    def test_ytdlp_and_ejs_importable(self):
        import yt_dlp
        import yt_dlp_ejs

        self.assertTrue(hasattr(yt_dlp, "YoutubeDL"))
        self.assertIsNotNone(yt_dlp_ejs)

        ver = tuple(int(part) for part in yt_dlp.version.__version__.split(".")[:3])
        self.assertGreaterEqual(ver, (2026, 11, 23))

        ejs_ver = importlib.metadata.version("yt-dlp-ejs")
        ejs_parts = tuple(int(part) for part in ejs_ver.split(".")[:2])
        self.assertGreaterEqual(ejs_parts, (0, 8))
