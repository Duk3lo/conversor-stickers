package es.conversorstickers.app

/** What the bridge decides without touching Android, so it can be tested with JUnit. */
object BridgeResponses {
  const val WHATSAPP = "com.whatsapp"
  const val WHATSAPP_BUSINESS = "com.whatsapp.w4b"
  const val RESULT_CANCELED = 0

  /** What Rust understands of the intent response: `result` and, if there is one, `reason`. */
  data class AddResponse(val result: String, val reason: String? = null)

  /**
   * WhatsApp answers `RESULT_CANCELED` with the `validation_error` extra if it rejects the pack,
   * and `RESULT_CANCELED` with nothing if the user cancelled. Any other answer is "added":
   * WhatsApp already told the user.
   */
  fun translateResult(code: Int, validationError: String?): AddResponse =
    when {
      code != RESULT_CANCELED -> AddResponse("added")
      !validationError.isNullOrBlank() -> AddResponse("validation_error", validationError)
      else -> AddResponse("cancelled")
    }

  sealed class Target {
    object None : Target()

    data class One(val pack: String) : Target()

    object Choose : Target()
  }

  /** With only one installed, send to it; with both, let the user choose. */
  fun target(installed: Collection<String>): Target {
    val whatsapp = installed.filter { it == WHATSAPP || it == WHATSAPP_BUSINESS }.distinct()
    return when (whatsapp.size) {
      0 -> Target.None
      1 -> Target.One(whatsapp[0])
      else -> Target.Choose
    }
  }

  /**
   * The app to open after adding: only if asked to and WhatsApp is unambiguous. With both
   * installed the chooser does not say which one the user picked, so none is opened.
   */
  fun packageToOpen(target: Target, openAfter: Boolean): String? =
    if (openAfter && target is Target.One) target.pack else null

  /**
   * The first clipboard item that is an image or a video, as (uri, mime). `candidates` are
   * (uri, mime) per item, in order.
   */
  fun pickMedia(candidates: List<Pair<String?, String?>>): Pair<String, String>? {
    for ((uri, mime) in candidates) {
      if (uri == null || mime == null) continue
      if (mime.startsWith("image/") || mime.startsWith("video/")) return uri to mime
    }
    return null
  }
}
