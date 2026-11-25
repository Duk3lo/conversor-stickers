"""Link download with yt-dlp inside the app.

Kotlin calls `download` on its own thread and `progress` and `cancel` from others. Everything it
returns is one-line JSON: `download` never raises, so the bridge does not have to interpret Python
exceptions.
"""

import json
import os
import threading

try:
    import yt_dlp
except ImportError:  # desktop tests inject a fake one
    yt_dlp = None

LIMIT_BYTES = 200 * 1024 * 1024
LIMIT_SECONDS = 10 * 60
# Only the best video, no audio to merge: a sticker does not use the audio.
FORMAT = "bv*[height<=720]/b[height<=720]/b"

# "save" keeps the sound and merges video and audio into an mp4 with the app's ffmpeg (stream copy,
# no re-encoding), so it can afford a larger file and a longer video.
SAVE_LIMIT_BYTES = 500 * 1024 * 1024
SAVE_LIMIT_SECONDS = 30 * 60
SAVE_FORMAT = "bv*[height<=1080]+ba/b[height<=1080]/b"
MODES = ("sticker", "save")

_lock = threading.Lock()
_cancelled = threading.Event()
_progress = {"bytes": 0, "total": None}
# Limits of the download in progress; `download` sets them from its mode.
_limits = {"bytes": LIMIT_BYTES, "seconds": LIMIT_SECONDS}


class _Stop(Exception):
    """Stops yt-dlp from a hook; `reason` is None when it was a cancel."""

    def __init__(self, reason=None):
        super().__init__(reason)
        self.reason = reason


def prepare_ffmpeg(native_libraries, links_folder):
    """Folder where yt-dlp finds `ffmpeg` and `ffprobe`.

    The app ships them as `libffmpeg.so` and `libffprobe.so` (the only names Gradle packages) and
    yt-dlp looks for those names, so they are linked from a folder in the app cache.
    """
    os.makedirs(links_folder, exist_ok=True)
    for name in ("ffmpeg", "ffprobe"):
        target = os.path.join(native_libraries, f"lib{name}.so")
        if not os.path.isfile(target):
            raise FileNotFoundError(f"falta lib{name}.so en la app")
        link = os.path.join(links_folder, name)
        if os.path.islink(link) or os.path.exists(link):
            os.remove(link)
        os.symlink(target, link)
    return links_folder


def options(destination, hook, mode="sticker", ffmpeg_folder=None):
    base = {
        "format": SAVE_FORMAT if mode == "save" else FORMAT,
        "noplaylist": True,
        "max_filesize": SAVE_LIMIT_BYTES if mode == "save" else LIMIT_BYTES,
        # Fragments are fetched by yt-dlp itself: the app's ffmpeg has no network.
        "hls_prefer_native": True,
        "paths": {"home": destination},
        "outtmpl": "%(title).60s.%(ext)s",
        "restrictfilenames": True,
        "cachedir": False,
        "socket_timeout": 30,
        "quiet": True,
        "no_warnings": True,
        "noprogress": True,
        "progress_hooks": [hook],
    }
    if mode == "save":
        base["merge_output_format"] = "mp4"
        base["ffmpeg_location"] = ffmpeg_folder
    return base


def _hook(data):
    if _cancelled.is_set():
        raise _Stop()
    # yt-dlp reports fractional byte counts for fragmented (HLS) downloads; the bridge expects integers.
    downloaded = int(data.get("downloaded_bytes") or 0)
    if downloaded > _limits["bytes"]:
        raise _Stop(f"pasa de {_limits['bytes'] // (1024 * 1024)} MB")
    total = data.get("total_bytes") or data.get("total_bytes_estimate")
    with _lock:
        _progress["bytes"] = downloaded
        _progress["total"] = int(total) if total else None


def _check(info):
    if info.get("is_live"):
        raise _Stop("es un directo y no se admiten directos")
    duration = info.get("duration")
    if duration is not None and duration > _limits["seconds"]:
        raise _Stop(f"dura más de {_limits['seconds'] // 60} minutos")


def download(link, destination, mode="sticker", native_libraries=None, links_folder=None):
    _cancelled.clear()
    with _lock:
        _progress.update(bytes=0, total=None)
    if mode not in MODES:
        return json.dumps({"result": "error", "message": "modo de descarga no válido"})
    saving = mode == "save"
    _limits["bytes"] = SAVE_LIMIT_BYTES if saving else LIMIT_BYTES
    _limits["seconds"] = SAVE_LIMIT_SECONDS if saving else LIMIT_SECONDS
    try:
        ffmpeg_folder = None
        if saving:
            if not native_libraries or not links_folder:
                return json.dumps({"result": "error", "message": "no se encontró FFmpeg"})
            ffmpeg_folder = prepare_ffmpeg(native_libraries, links_folder)
        with yt_dlp.YoutubeDL(options(destination, _hook, mode, ffmpeg_folder)) as ydl:
            info = ydl.extract_info(link, download=False)
            _check(info)
            outcome = ydl.process_ie_result(info, download=True)
        path = outcome["requested_downloads"][0]["filepath"]
        return json.dumps({"result": "ready", "path": path})
    except _Stop as stop:
        return _stopped(stop)
    except Exception as error:  # yt-dlp wraps the hook error in DownloadError
        if _cancelled.is_set():
            return json.dumps({"result": "cancelled"})
        cause = error.exc_info[1] if getattr(error, "exc_info", None) else None
        if isinstance(cause, _Stop):
            return _stopped(cause)
        return json.dumps({"result": "error", "message": str(error)[:500]})


def _stopped(stop):
    if stop.reason is None:
        return json.dumps({"result": "cancelled"})
    return json.dumps({"result": "error", "message": stop.reason})


def progress():
    with _lock:
        return json.dumps(_progress)


def cancel():
    _cancelled.set()
