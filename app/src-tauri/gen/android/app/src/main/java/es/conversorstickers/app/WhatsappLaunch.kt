package es.conversorstickers.app

/** Which WhatsApp to bring to the front, in order of preference. Kotlin only, so JUnit can test it. */
object WhatsappLaunch {
  /** WhatsApp first; WhatsApp Business only if the regular app is not installed or cannot open. */
  val PACKAGES = listOf("com.whatsapp", "com.whatsapp.w4b")

  /** The first package, in order, that `open` manages to launch; `null` if none does. */
  fun firstOpened(packages: List<String>, open: (String) -> Boolean): String? =
    packages.firstOrNull(open)

  private val NUMBER = Regex("[0-9]{8,15}")

  /** The chat id WhatsApp reads in the `jid` extra of a share; `null` if `number` is not 8 to 15 digits. */
  fun jid(number: String): String? = if (NUMBER.matches(number)) "$number@s.whatsapp.net" else null
}
