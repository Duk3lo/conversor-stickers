package es.conversorstickers.app

/** Play edition: no yt-dlp and no Python. Rust never registers the download commands. */
class PythonEngine(@Suppress("UNUSED_PARAMETER") context: android.content.Context) : DownloadEngine {
  override fun download(job: DownloadJob): String = throw UnsupportedOperationException()

  override fun progress(): String = throw UnsupportedOperationException()

  override fun cancel() = throw UnsupportedOperationException()
}
