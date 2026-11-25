package es.conversorstickers.app

/** Name and type of a sound saved in `Music/ConversorStickers`; both come from Rust, not trusted. */
object SavedSounds {
  const val FOLDER = "Music/ConversorStickers"

  private val MIMES = mapOf("m4a" to "audio/mp4", "webm" to "audio/webm")

  /** Same filter as the videos, with `sonido.m4a` if nothing is left. */
  fun displayName(name: String): String {
    val clean = SavedVideos.displayName(name)
    return if (clean == "video.mp4") "sonido.m4a" else clean
  }

  /** `null` if it is not a sound extension we save. */
  fun mime(name: String): String? = MIMES[name.substringAfterLast('.', "").lowercase()]
}
