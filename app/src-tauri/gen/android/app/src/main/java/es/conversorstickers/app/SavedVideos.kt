package es.conversorstickers.app

/** Name and type of a video saved in `Movies/ConversorStickers`; both come from Rust, not trusted. */
object SavedVideos {
  const val FOLDER = "Movies/ConversorStickers"

  private val MIMES = mapOf("mp4" to "video/mp4", "mkv" to "video/x-matroska", "webm" to "video/webm")

  /** Only `[A-Za-z0-9._-]`, no leading dot or dash, at most 80 characters in the base and `video` if empty. */
  fun displayName(name: String): String {
    val filtered =
      name.map { if (it.isWhitespace()) '_' else it }.filter {
        it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it == '.' || it == '_' || it == '-'
      }
    val clean = filtered.joinToString("").trimStart('.', '-')
    if (clean.isEmpty()) return "video.mp4"
    // Only the base is truncated, so the extension that `mime` and Rust check survives.
    val dot = clean.lastIndexOf('.')
    if (dot <= 0) return clean.take(80)
    return clean.substring(0, dot).take(80) + clean.substring(dot)
  }

  /** `null` if it is not a video extension we save. */
  fun mime(name: String): String? = MIMES[name.substringAfterLast('.', "").lowercase()]
}
