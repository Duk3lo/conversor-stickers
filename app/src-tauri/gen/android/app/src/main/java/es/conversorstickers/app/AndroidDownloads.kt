package es.conversorstickers.app

import org.json.JSONObject

/** What to download and where; `mode` is `sticker` (video only) or `save` (with audio, mp4). */
data class DownloadJob(
  val link: String,
  val destination: String,
  val mode: String = "sticker",
  /** Where Android extracted `libffmpeg.so`; `save` merges with it. */
  val nativeLibraries: String? = null,
  /** Cache folder where `ffmpeg` and `ffprobe` links to those libraries are made. */
  val linksFolder: String? = null,
)

/** Who really downloads (yt-dlp through Chaquopy); in tests, a fake. */
interface DownloadEngine {
  /** Blocks. Returns the JSON of `link_download.download`, which never throws. */
  fun download(job: DownloadJob): String

  fun progress(): String

  fun cancel()
}

/**
 * One download at a time: Rust starts it and polls it, because the bridge only answers once per
 * call. The final result stays until another one starts.
 */
class AndroidDownloads(private val engine: DownloadEngine) {
  @Volatile private var thread: Thread? = null
  @Volatile private var result: JSONObject? = null

  /** `false` if a download is already running. */
  @Synchronized
  fun start(job: DownloadJob): Boolean {
    if (thread?.isAlive == true) return false
    result = null
    thread =
      Thread {
          result =
            try {
              JSONObject(engine.download(job))
            } catch (e: Exception) {
              JSONObject().put("result", "error").put("message", e.message ?: "fallo interno")
            }
        }
        .also { it.start() }
    return true
  }

  /**
   * `phase`: `idle`, `in_progress` (with `bytes` and `total`), `ready` (`path`), `cancelled` or
   * `error` (`message`).
   */
  fun status(): String {
    val finished = result
    if (finished != null) {
      return JSONObject(finished.toString()).put("phase", finished.getString("result")).toString()
    }
    if (thread?.isAlive != true) return JSONObject().put("phase", "idle").toString()
    return JSONObject(engine.progress()).put("phase", "in_progress").toString()
  }

  fun cancel() {
    if (thread?.isAlive == true) engine.cancel()
  }

  /** Only for tests. */
  fun awaitFinish() {
    thread?.join()
  }
}
