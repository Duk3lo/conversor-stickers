package es.conversorstickers.app

/**
 * Translates the "Share" intent of another app without touching Android, so it can be tested with
 * JUnit. What arrives is external: only `http(s)` links and `content://` URIs are accepted.
 */
object SharedIntent {
  const val ACTION_SEND = "android.intent.action.SEND"
  const val ACTION_SEND_MULTIPLE = "android.intent.action.SEND_MULTIPLE"

  data class Media(val uri: String, val mime: String?)

  data class Shared(val links: List<String>, val media: List<Media>) {
    val isEmpty: Boolean
      get() = links.isEmpty() && media.isEmpty()
  }

  val NOTHING = Shared(emptyList(), emptyList())

  /** `mimes` is aligned with `uris`; a wildcard type (with an asterisk) or a missing one is `null`. */
  fun translate(action: String?, text: String?, uris: List<String>, mimes: List<String?>): Shared {
    if (action != ACTION_SEND && action != ACTION_SEND_MULTIPLE) return NOTHING
    val media =
      uris.mapIndexedNotNull { i, uri ->
        if (!uri.startsWith("content://")) return@mapIndexedNotNull null
        val mime = mimes.getOrNull(i)?.takeIf { it.isNotBlank() && !it.contains('*') }
        Media(uri, mime)
      }
    return Shared(linksFromText(text), media)
  }

  /** The pieces that start with `http://` or `https://`, without punctuation stuck at the end. */
  fun linksFromText(text: String?): List<String> =
    text
      .orEmpty()
      .split(Regex("\\s+"))
      .map { it.trimEnd('.', ',', ';', ')', '"', '\'', '>') }
      .filter { (it.startsWith("https://") || it.startsWith("http://")) && it.length > 8 }
      .distinct()
}
