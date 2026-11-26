import contextlib
import hashlib
import importlib.metadata
import importlib.util
import io
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

BACKEND_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
if BACKEND_DIR not in sys.path:
    sys.path.insert(0, BACKEND_DIR)

HAS_YTDLP = importlib.util.find_spec("yt_dlp") is not None


class TestYtdlpOptionsAndAllowList(unittest.TestCase):
    def test_assert_allowed_url_accepts_youtube_hosts(self):
        from providers.ytdlp.options import assert_allowed_url

        for url in (
            "https://youtube.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://music.youtube.com/watch?v=Go2MaihKX9o",
            "https://youtu.be/dQw4w9WgXcQ",
        ):
            assert_allowed_url(url)

    def test_assert_allowed_url_rejects_disallowed_schemes_and_hosts(self):
        from providers.ytdlp.options import assert_allowed_url

        for bad in (
            "",
            "http://youtube.com/watch?v=dQw4w9WgXcQ",
            "file:///etc/passwd",
            "https://evil.example.com/watch?v=123",
            "https://user@youtube.com/watch?v=123",
            "ftp://youtube.com/watch?v=123",
        ):
            with self.assertRaises(ValueError):
                assert_allowed_url(bad)

    def test_build_params_uses_silent_logger_and_dict_js_runtimes(self):
        from providers.ytdlp.options import TIER1_PLAYER_CLIENTS, build_params

        with patch("providers.ytdlp.runtime.js_runtimes_param", return_value={"node": {"path": "/bin/node"}}):
            params = build_params(cache_dir="/tmp/ansimuz-cache", player_clients=TIER1_PLAYER_CLIENTS)

        self.assertTrue(params["ignoreconfig"])
        self.assertTrue(params["noplaylist"])
        self.assertTrue(params["skip_download"])
        self.assertEqual(params["remote_components"], [])
        self.assertEqual(params["allowed_extractors"], [r"^youtube$"])
        self.assertIsInstance(params["js_runtimes"], dict)
        self.assertEqual(params["extractor_args"]["youtube"]["player_client"], TIER1_PLAYER_CLIENTS)

        stdout = io.StringIO()
        stderr = io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            params["logger"].debug("debug line")
            params["logger"].info("info line")
            params["logger"].warning("warning line")
            params["logger"].error("error line")
        self.assertEqual(stdout.getvalue(), "")
        self.assertEqual(stderr.getvalue(), "")


class TestYtdlpRuntimeDetection(unittest.TestCase):
    def test_sha256_verification(self):
        from providers.ytdlp.runtime import _verify_sha256

        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "qjs"
            payload = b"quickjs-test-binary"
            p.write_bytes(payload)
            good_hash = hashlib.sha256(payload).hexdigest()
            self.assertTrue(_verify_sha256(p, good_hash))
            self.assertFalse(_verify_sha256(p, "0" * 64))

    def test_detect_runtime_prefers_system_node_over_none_and_uses_bundled_qjs_fallback(self):
        from providers.ytdlp import runtime

        with patch.object(runtime, "_bundled_qjs_verified", return_value=Path("/opt/ansimuz/runtime/qjs")):
            with patch("shutil.which", side_effect=lambda name: "/usr/bin/node" if name == "node" else None):
                runtimes, mode, path = runtime.detect_runtime()
                self.assertEqual(mode, "system-node")
                self.assertEqual(path, "/usr/bin/node")
                self.assertEqual(runtimes, {"node": {"path": "/usr/bin/node"}})

            with patch("shutil.which", return_value=None):
                runtimes, mode, path = runtime.detect_runtime()
                self.assertEqual(mode, "bundled-quickjs")
                self.assertTrue(path.endswith("qjs"))
                self.assertEqual(runtimes, {"quickjs": {"path": str(Path("/opt/ansimuz/runtime/qjs"))}})

        with patch.object(runtime, "_bundled_qjs_verified", return_value=None):
            with patch("shutil.which", return_value=None):
                runtimes, mode, path = runtime.detect_runtime()
                self.assertEqual(mode, "none")
                self.assertIsNone(path)
                self.assertEqual(runtimes, {})

    def test_bundled_qjs_requires_pinned_sha256(self):
        from providers.ytdlp import runtime

        with tempfile.TemporaryDirectory() as tmp:
            runtime_dir = Path(tmp) / "runtime"
            runtime_dir.mkdir(parents=True, exist_ok=True)
            exe_path = runtime_dir / runtime._bundled_qjs_filename()
            payload = b"mock-qjs-binary"
            exe_path.write_bytes(payload)
            payload_hash = hashlib.sha256(payload).hexdigest()
            target_key = runtime._runtime_target_key()

            with patch.dict(runtime.BUNDLED_QJS_SHA256, {target_key: payload_hash}, clear=True):
                self.assertEqual(runtime._bundled_qjs_verified(tmp), exe_path)

            with patch.dict(runtime.BUNDLED_QJS_SHA256, {target_key: "f" * 64}, clear=True):
                self.assertIsNone(runtime._bundled_qjs_verified(tmp))

            with patch.dict(runtime.BUNDLED_QJS_SHA256, {}, clear=True):
                self.assertIsNone(runtime._bundled_qjs_verified(tmp))


class TestYtdlpErrorMapping(unittest.TestCase):
    def test_classify_all_ytdlp_error_categories(self):
        from exceptions import (
            BotCheckError,
            GeoBlockedError,
            JsRuntimeError,
            NetworkError,
            RateLimitError,
            ResolverOutdatedError,
            StreamUnavailableError,
            TrackNotFoundError,
        )
        from providers.ytdlp.errors import classify_ytdlp_error

        class FakeDownloadError(Exception):
            def __init__(self, msg, cause=None):
                super().__init__(msg)
                self.exc_info = (type(cause), cause, None) if cause else None

        class GeoRestrictedError(Exception):
            pass

        class JsChallengeProviderError(Exception):
            pass

        class FakeExtractorError(Exception):
            def __init__(self, msg, expected=True):
                super().__init__(msg)
                self.expected = expected

        self.assertIsInstance(
            classify_ytdlp_error(FakeDownloadError("blocked", GeoRestrictedError("geo"))),
            GeoBlockedError,
        )
        self.assertIsInstance(
            classify_ytdlp_error(FakeDownloadError("solver fail", JsChallengeProviderError("boom"))),
            JsRuntimeError,
        )
        self.assertIsInstance(
            classify_ytdlp_error(FakeDownloadError("ERROR: [youtube] id: This video is unavailable", FakeExtractorError("This video is unavailable"))),
            TrackNotFoundError,
        )
        self.assertIsInstance(
            classify_ytdlp_error(FakeDownloadError("ERROR: Private video")),
            TrackNotFoundError,
        )
        self.assertIsInstance(
            classify_ytdlp_error(FakeDownloadError("Sign in to confirm you're not a bot")),
            BotCheckError,
        )
        self.assertIsInstance(
            classify_ytdlp_error(FakeDownloadError("YouTube is requiring a captcha challenge before playback")),
            BotCheckError,
        )
        self.assertIsInstance(
            classify_ytdlp_error(FakeDownloadError("HTTP Error 429: Too Many Requests")),
            RateLimitError,
        )
        self.assertIsInstance(
            classify_ytdlp_error(FakeDownloadError("This content isn't available, try again later")),
            RateLimitError,
        )
        self.assertIsInstance(
            classify_ytdlp_error(FakeDownloadError("socket", TimeoutError("timed out"))),
            NetworkError,
        )
        self.assertIsInstance(
            classify_ytdlp_error(FakeDownloadError("Requested format is not available")),
            StreamUnavailableError,
        )
        self.assertIsInstance(
            classify_ytdlp_error(FakeDownloadError("Unable to extract nsig parameter")),
            ResolverOutdatedError,
        )


@unittest.skipUnless(HAS_YTDLP, "yt_dlp not importable")
class TestYtdlpResolverWithFakeYdl(unittest.TestCase):
    def tearDown(self):
        from providers.ytdlp import resolver

        resolver.configure()

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

    def test_resolve_stream_extracts_fields_and_parses_expire(self):
        import bridge

        fake_info = {
            "id": "Go2MaihKX9o",
            "title": "Murder on My Mind",
            "duration": 270,
            "entries": [
                {
                    "id": "Go2MaihKX9o",
                    "title": "Murder on My Mind",
                    "duration": 270,
                    "requested_formats": [
                        {"acodec": "none", "url": "https://rr1.googlevideo.com/videoonly"},
                        {
                            "acodec": "opus",
                            "ext": "webm",
                            "abr": 139.8,
                            "url": "https://rr1.googlevideo.com/videoplayback?expire=1795661741&itag=251",
                            "http_headers": {"User-Agent": "ANSImuz-Test/1.0"},
                        },
                    ],
                }
            ],
        }

        class FakeYdl:
            def __init__(self, _params):
                pass

            def extract_info(self, url, download=False):
                self.last_url = url
                self.last_download = download
                return fake_info

        stdout = io.StringIO()
        stderr = io.StringIO()
        with patch("yt_dlp.YoutubeDL", FakeYdl):
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                result = bridge.resolve_stream("https://www.youtube.com/watch?v=Go2MaihKX9o")

        self.assertEqual(stdout.getvalue(), "")
        self.assertEqual(stderr.getvalue(), "")
        self.assertEqual(result["url"], "https://rr1.googlevideo.com/videoplayback?expire=1795661741&itag=251")
        self.assertEqual(result["headers"], {"User-Agent": "ANSImuz-Test/1.0"})
        self.assertEqual(result["expires_at"], 1795661741)
        self.assertEqual(result["ext"], "webm")
        self.assertEqual(result["acodec"], "opus")
        self.assertEqual(result["abr"], 139.8)
        self.assertEqual(result["duration_s"], 270)
        self.assertEqual(result["video_id"], "Go2MaihKX9o")
        self.assertIn("ytdlp_version", result)
        self.assertIn("js_runtime", result)

    def test_tier1_escalates_on_format_error_but_not_on_geo_block(self):
        from exceptions import GeoBlockedError
        from providers.ytdlp import options, resolver
        from yt_dlp.utils import DownloadError, GeoRestrictedError

        calls = []

        class EscalatingYdl:
            def __init__(self, params):
                self.clients = params["extractor_args"]["youtube"]["player_client"]

            def extract_info(self, url, download=False):
                calls.append(self.clients)
                if self.clients == options.TIER1_PLAYER_CLIENTS:
                    raise DownloadError("Requested format is not available")
                return {
                    "id": "vid1",
                    "url": "https://rr2.googlevideo.com/videoplayback?expire=1795662000",
                    "ext": "webm",
                    "acodec": "opus",
                    "http_headers": {},
                }

        resolver.configure()
        with patch("yt_dlp.YoutubeDL", EscalatingYdl):
            res = resolver.resolve_stream("https://www.youtube.com/watch?v=vid1")
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0], options.TIER1_PLAYER_CLIENTS)
        self.assertEqual(calls[1], options.FALLBACK_PLAYER_CLIENTS)
        self.assertEqual(res["expires_at"], 1795662000)

        calls.clear()

        class GeoBlockingYdl:
            def __init__(self, params):
                self.clients = params["extractor_args"]["youtube"]["player_client"]

            def extract_info(self, url, download=False):
                calls.append(self.clients)
                geo = GeoRestrictedError("blocked")
                raise DownloadError("geo", (GeoRestrictedError, geo, None))

        resolver.configure()
        with patch("yt_dlp.YoutubeDL", GeoBlockingYdl):
            with self.assertRaises(GeoBlockedError):
                resolver.resolve_stream("https://www.youtube.com/watch?v=vid1")
        self.assertEqual(len(calls), 1)

    def test_apply_hermetic_globals_clears_plugin_dirs(self):
        import yt_dlp.globals
        from providers.ytdlp.options import apply_hermetic_globals

        yt_dlp.globals.plugin_dirs.value = ["default"]
        yt_dlp.globals.all_plugins_loaded.value = False
        apply_hermetic_globals()
        self.assertEqual(yt_dlp.globals.plugin_dirs.value, [])
        self.assertTrue(yt_dlp.globals.all_plugins_loaded.value)

    def test_self_test_runs_offline_and_requires_js_runtime(self):
        import bridge
        from providers.ytdlp import runtime, self_test

        with patch.object(runtime, "detect_runtime", return_value=({}, "none", None)):
            with self.assertRaises(RuntimeError):
                bridge.self_test()

        def fake_subprocess_run(cmd, **_kwargs):
            self.assertIn("--script", cmd)

            class FakeProc:
                returncode = 0
                stdout = '{"type":"result","responses":[{"type":"result","data":{"abc123":"abc123_ok"}}]}\n'
                stderr = ""

            return FakeProc()

        with patch.object(
            runtime,
            "detect_runtime",
            return_value=({"quickjs": {"path": "/tmp/qjs"}}, "bundled-quickjs", "/tmp/qjs"),
        ):
            with patch.object(self_test.subprocess, "run", side_effect=fake_subprocess_run):
                report = bridge.self_test()

        self.assertEqual(report["js_runtime"], "bundled-quickjs")
        self.assertEqual(report["solver_output"], "abc123_ok")
        self.assertFalse(report["ytdlp_binary_needed"])
        self.assertTrue(report["ytdlp_version"])
        self.assertTrue(report["ejs_version"])
