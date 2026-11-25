package es.conversorstickers.app

import java.io.File

/** What the share bridge decides without touching Android, so it can be tested with JUnit. */
object ShareFile {
  /** Cache folder shared through the FileProvider; `file_paths.xml` names the same one. */
  const val DIRECTORY = "conversor-stickers-share"
  const val MIME = "image/webp"

  private val NAME = Regex("sticker-[0-9]+\\.webp")

  /** Only the WebP copies Rust prepares in the share folder of the cache can leave the app. */
  fun isShareable(cache: File, file: File): Boolean {
    val folder = File(cache.canonicalFile, DIRECTORY)
    val target = file.canonicalFile
    return target.parentFile == folder && NAME.matches(target.name) && target.isFile
  }
}
