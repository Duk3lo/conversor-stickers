"""Tests of link_download with a fake yt_dlp: `python3 -I -m unittest discover -s src/test/python`."""

import json
import os
import sys
import tempfile
import types
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "main", "python"))
import link_download as d


class FakeYoutubeDL:
    """Imitates what the module uses: extract_info, process_ie_result and the hooks."""

    info = {"title": "Un video", "duration": 5, "is_live": False}
    bytes_to_send = [100, 200]
    final_path = None

    def __init__(self, options):
        self.options = options

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def extract_info(self, url, download=False):
        return dict(self.info)

    def process_ie_result(self, info, download=True):
        for n in self.bytes_to_send:
            for hook in self.options["progress_hooks"]:
                hook({"status": "downloading", "downloaded_bytes": n, "total_bytes": 1000})
        with open(self.final_path, "wb") as f:
            f.write(b"video")
        return {"requested_downloads": [{"filepath": self.final_path}]}


def with_fake(cls=FakeYoutubeDL):
    d.yt_dlp = types.SimpleNamespace(YoutubeDL=cls)


class DownloadTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.mkdtemp()
        self.path = os.path.join(self.folder, "v.mp4")
        self.addCleanup(lambda: __import__("shutil").rmtree(self.folder, ignore_errors=True))

        class Fake(FakeYoutubeDL):
            final_path = self.path

        self.fake = Fake
        with_fake(Fake)

    def test_options_limit_quality_size_and_playlists(self):
        o = d.options("/tmp/x", lambda _: None)
        self.assertEqual(o["format"], "bv*[height<=720]/b[height<=720]/b")
        self.assertTrue(o["noplaylist"])
        self.assertEqual(o["max_filesize"], 200 * 1024 * 1024)
        self.assertNotIn("ffmpeg_location", o)
        self.assertTrue(o["hls_prefer_native"])
        self.assertEqual(o["paths"], {"home": "/tmp/x"})
        self.assertTrue(o["restrictfilenames"])
        self.assertTrue(o["cachedir"] is False)

    def test_save_mode_keeps_audio_merges_to_mp4_and_uses_the_app_ffmpeg(self):
        o = d.options("/tmp/x", lambda _: None, "save", "/c/links")
        self.assertEqual(o["format"], "bv*[height<=1080]+ba/b[height<=1080]/b")
        self.assertEqual(o["merge_output_format"], "mp4")
        self.assertEqual(o["ffmpeg_location"], "/c/links")
        self.assertTrue(o["noplaylist"])
        self.assertEqual(o["max_filesize"], 500 * 1024 * 1024)
        self.assertTrue(o["hls_prefer_native"])

    def test_prepare_ffmpeg_links_the_libraries_with_the_names_yt_dlp_looks_for(self):
        libraries = os.path.join(self.folder, "lib")
        links = os.path.join(self.folder, "links")
        os.makedirs(libraries)
        for name in ("libffmpeg.so", "libffprobe.so"):
            open(os.path.join(libraries, name), "wb").close()
        d.prepare_ffmpeg(libraries, links)
        d.prepare_ffmpeg(libraries, links)  # idempotent: replaces the links
        self.assertEqual(os.path.realpath(os.path.join(links, "ffmpeg")), os.path.realpath(os.path.join(libraries, "libffmpeg.so")))
        self.assertEqual(os.path.realpath(os.path.join(links, "ffprobe")), os.path.realpath(os.path.join(libraries, "libffprobe.so")))

    def test_save_mode_downloads_with_its_limits_and_restores_the_sticker_ones(self):
        libraries = os.path.join(self.folder, "lib")
        os.makedirs(libraries)
        for name in ("libffmpeg.so", "libffprobe.so"):
            open(os.path.join(libraries, name), "wb").close()
        links = os.path.join(self.folder, "links")
        # 20 minutes pass in save mode (30 min) but not in sticker mode (10 min).
        self.fake.info = {"title": "x", "duration": 1200, "is_live": False}
        r = json.loads(d.download("https://a.example/v", self.folder, "save", libraries, links))
        self.assertEqual(r, {"result": "ready", "path": self.path})
        r = json.loads(d.download("https://a.example/v", self.folder))
        self.assertIn("10 minutos", r["message"])
        self.fake.info = {"title": "x", "duration": 1801, "is_live": False}
        r = json.loads(d.download("https://a.example/v", self.folder, "save", libraries, links))
        self.assertIn("30 minutos", r["message"])
        self.fake.info = {"title": "Un video", "duration": 5, "is_live": False}

    def test_save_mode_without_ffmpeg_or_with_an_unknown_mode_is_an_error(self):
        r = json.loads(d.download("https://a.example/v", self.folder, "save"))
        self.assertEqual(r["result"], "error")
        self.assertIn("FFmpeg", r["message"])
        r = json.loads(d.download("https://a.example/v", self.folder, "otro"))
        self.assertEqual(r["result"], "error")

    def test_save_mode_with_a_missing_library_gives_a_message_not_an_exception(self):
        r = json.loads(d.download("https://a.example/v", self.folder, "save", self.folder, os.path.join(self.folder, "l")))
        self.assertEqual(r["result"], "error")
        self.assertIn("libffmpeg.so", r["message"])

    def test_a_good_download_gives_the_path(self):
        r = json.loads(d.download("https://a.example/v", self.folder))
        self.assertEqual(r, {"result": "ready", "path": self.path})
        self.assertEqual(json.loads(d.progress())["bytes"], 200)

    def test_a_long_video_or_a_live_stream_is_rejected_without_downloading(self):
        for info, text in [({"duration": 601}, "10 minutos"), ({"is_live": True}, "directo")]:
            self.fake.info = {"title": "x", "duration": 5, "is_live": False, **info}
            r = json.loads(d.download("https://a.example/v", self.folder))
            self.assertEqual(r["result"], "error")
            self.assertIn(text, r["message"])
            self.assertFalse(os.path.exists(self.path))
        self.fake.info = {"title": "Un video", "duration": 5, "is_live": False}

    def test_going_over_the_size_stops_the_download(self):
        self.fake.bytes_to_send = [200 * 1024 * 1024 + 1]
        r = json.loads(d.download("https://a.example/v", self.folder))
        self.assertEqual(r["result"], "error")
        self.assertIn("200 MB", r["message"])
        self.fake.bytes_to_send = [100, 200]

    def test_cancelling_midway_gives_cancelled(self):
        d.cancel()
        r = json.loads(d.download("https://a.example/v", self.folder))
        # download resets the flag; cancelling during the download does count.
        self.assertEqual(r["result"], "ready")

        class Cancels(self.fake):
            def process_ie_result(inner, info, download=True):
                d.cancel()
                return super().process_ie_result(info, download)

        with_fake(Cancels)
        r = json.loads(d.download("https://a.example/v", self.folder))
        self.assertEqual(r, {"result": "cancelled"})

    def test_fractional_byte_counts_from_hls_are_reported_as_integers(self):
        d._cancelled.clear()
        d._hook({"status": "downloading", "downloaded_bytes": 2229652.5, "total_bytes_estimate": 4000000.7})
        progress = json.loads(d.progress())
        self.assertEqual(progress, {"bytes": 2229652, "total": 4000000})
        self.assertIsInstance(progress["bytes"], int)
        self.assertIsInstance(progress["total"], int)

    def test_a_yt_dlp_failure_gives_its_message(self):
        class Fails(self.fake):
            def extract_info(inner, url, download=False):
                raise RuntimeError("Unsupported URL: https://a.example/v")

        with_fake(Fails)
        r = json.loads(d.download("https://a.example/v", self.folder))
        self.assertEqual(r["result"], "error")
        self.assertIn("Unsupported URL", r["message"])


if __name__ == "__main__":
    unittest.main()
