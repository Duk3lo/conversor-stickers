package es.conversorstickers.app

import java.io.File
import org.json.JSONObject

class IndexedSticker(val file: String, val emojis: List<String>)

class IndexedPack(
  val id: String,
  val name: String,
  val author: String,
  val animated: Boolean,
  val imageVersion: String,
  val tray: String,
  val stickers: List<IndexedSticker>,
)

/**
 * The index of the published copy: Rust writes it (`whatsapp.rs`) and this only reads it. It is
 * Kotlin without Android types so it can be tested with JUnit. Everything that leaves here for
 * the provider is validated: id and file by regular expression, listed in the index and with the
 * canonical path inside the pack folder.
 */
class WhatsappIndex(private val directory: File, val packs: List<IndexedPack>) {

  fun pack(id: String): IndexedPack? {
    if (!isValidId(id)) return null
    return packs.firstOrNull { it.id == id }
  }

  /**
   * The file served for `id/file`, or `null` if it breaks any rule. Symbolic links do not pass:
   * their canonical path ends up outside where it must be.
   */
  fun file(id: String, file: String): File? {
    val pack = pack(id) ?: return null
    if (!isValidFile(file)) return null
    val listed = file == pack.tray || pack.stickers.any { it.file == file }
    if (!listed) return null
    return try {
      val expected = File(File(directory.canonicalFile, id), file)
      val real = File(directory, "$id/$file").canonicalFile
      if (real == expected && real.isFile) real else null
    } catch (_: java.io.IOException) {
      null
    }
  }

  companion object {
    private val ID = Regex("^pack_[0-9]{1,9}$")
    private val FILE = Regex("^([0-9]{2}\\.webp|tray\\.png)$")
    private const val MAX_SIZE_BYTES = 1_000_000L
    private const val TAG = "WhatsappIndex"

    fun isValidId(id: String) = ID.matches(id)

    fun isValidFile(file: String) = FILE.matches(file)

    fun mimeType(file: String): String? =
      when {
        !isValidFile(file) -> null
        file.endsWith(".png") -> "image/png"
        else -> "image/webp"
      }

    /** Where Rust leaves the copy: on Android `app_data_dir` is `dataDir` itself. */
    fun folder(dataDir: File) = File(dataDir, "whatsapp")

    /** A missing or unreadable index is an empty index; the reason goes to the log. */
    fun read(directory: File): WhatsappIndex {
      val file = File(directory, "index.json")
      return try {
        if (file.length() > MAX_SIZE_BYTES) error("el índice es demasiado grande")
        parse(file.readText(), directory)
      } catch (failure: Exception) {
        if (file.exists()) log("índice ilegible: ${failure.javaClass.simpleName}")
        WhatsappIndex(directory, emptyList())
      }
    }

    fun parse(json: String, directory: File): WhatsappIndex {
      val root = JSONObject(json)
      require(root.getInt("version") == 1) { "versión de índice desconocida" }
      val list = root.getJSONArray("packs")
      val packs =
        (0 until list.length()).map { i ->
          val p = list.getJSONObject(i)
          val stickers = p.getJSONArray("stickers")
          IndexedPack(
            id = p.getString("id"),
            name = p.getString("name"),
            author = p.getString("author"),
            animated = p.getBoolean("animated"),
            imageVersion = p.getString("image_version"),
            tray = p.getString("tray"),
            stickers =
              (0 until stickers.length()).map { j ->
                val s = stickers.getJSONObject(j)
                val emojis = s.getJSONArray("emojis")
                IndexedSticker(
                  s.getString("file"),
                  (0 until emojis.length()).map { emojis.getString(it) },
                )
              },
          )
        }
      return WhatsappIndex(directory, packs)
    }

    // `android.util.Log` does not exist in JVM tests.
    private fun log(message: String) {
      try {
        android.util.Log.w(TAG, message)
      } catch (_: Throwable) {
        System.err.println("$TAG: $message")
      }
    }
  }
}
