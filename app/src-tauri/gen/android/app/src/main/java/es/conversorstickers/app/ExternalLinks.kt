package es.conversorstickers.app

/**
 * What the app opens outside itself or receives from outside, without touching Android so JUnit
 * can test it. Rust checks too; this is the last check before an intent is built.
 */
object ExternalLinks {
  /**
   * The sites the interface links to, plus `wa.me` for the bot's chat. Keep it equal to
   * `ALLOWED_HOSTS` in `external.rs`, which lacks `wa.me`.
   */
  private val HOSTS = setOf("github.com", "developers.giphy.com", "partner.klipy.com", "wa.me")
  private const val MAX_URL_LENGTH = 2048

  /** `url` if it is `https` to one of the known sites, with no user info or port; else `null`. */
  fun openable(url: String): String? {
    if (url.length > MAX_URL_LENGTH || !url.all { it.code in 0x21..0x7e }) return null
    val rest = url.removePrefix("https://")
    if (rest == url) return null
    val host = rest.takeWhile { it != '/' && it != '?' && it != '#' }
    return if (host in HOSTS) url else null
  }

  const val BOT_LINK_PREFIX = "conversorstickers://bot#"
  private const val ACTION_VIEW = "android.intent.action.VIEW"
  private const val MAX_BOT_LINK_LENGTH = 1024

  /** The connection link of a `VIEW` intent, raw: Rust parses it and asks the person. */
  fun botLink(action: String?, data: String?): String? =
    data?.takeIf {
      action == ACTION_VIEW && it.startsWith(BOT_LINK_PREFIX) && it.length <= MAX_BOT_LINK_LENGTH
    }
}
