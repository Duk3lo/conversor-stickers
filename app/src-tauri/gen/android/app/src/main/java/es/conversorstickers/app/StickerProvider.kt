package es.conversorstickers.app

import android.content.ContentProvider
import android.content.ContentValues
import android.content.UriMatcher
import android.content.res.AssetFileDescriptor
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor

/**
 * Serves WhatsApp the published copy of the packs. Read-only: the manifest requires
 * `com.whatsapp.sticker.READ`, and here `insert`, `update` and `delete` throw and files are only
 * opened in `"r"` mode. Anything that breaks the `WhatsappIndex` rules gives `null` or
 * `IllegalArgumentException`, without paths.
 */
class StickerProvider : ContentProvider() {
  private val authority by lazy { "${context!!.packageName}.stickers" }
  private val matches by lazy {
    UriMatcher(UriMatcher.NO_MATCH).apply {
      addURI(authority, "metadata", ALL)
      addURI(authority, "metadata/*", ONE)
      addURI(authority, "stickers/*", STICKERS)
      addURI(authority, "stickers_asset/*/*", FILE)
    }
  }

  private fun index(): WhatsappIndex {
    val appContext = context!!
    return WhatsappIndex.read(WhatsappIndex.folder(appContext.dataDir))
  }

  override fun onCreate() = true

  private fun segments(uri: Uri, count: Int): List<String> {
    val parts = uri.pathSegments
    require(parts.size == count) { "URI no válida" }
    return parts
  }

  override fun query(
    uri: Uri,
    projection: Array<String>?,
    selection: String?,
    selectionArgs: Array<String>?,
    sortOrder: String?,
  ): Cursor =
    when (matches.match(uri)) {
      ALL -> {
        segments(uri, 1)
        metadata(index().packs)
      }
      ONE -> {
        val id = segments(uri, 2)[1]
        require(WhatsappIndex.isValidId(id)) { "URI no válida" }
        metadata(listOfNotNull(index().pack(id)))
      }
      STICKERS -> {
        val id = segments(uri, 2)[1]
        require(WhatsappIndex.isValidId(id)) { "URI no válida" }
        stickers(index().pack(id))
      }
      else -> throw IllegalArgumentException("URI no válida")
    }

  private fun metadata(packs: List<IndexedPack>): Cursor {
    val cursor = MatrixCursor(METADATA_COLUMNS)
    for (p in packs) {
      cursor.newRow()
        .add(p.id)
        .add(p.name)
        .add(p.author)
        .add(p.tray)
        .add("")
        .add("")
        .add("")
        .add("")
        .add("")
        .add("")
        .add(p.imageVersion)
        .add(0)
        .add(if (p.animated) 1 else 0)
    }
    return cursor
  }

  private fun stickers(pack: IndexedPack?): Cursor {
    val cursor = MatrixCursor(STICKER_COLUMNS)
    for (s in pack?.stickers.orEmpty()) {
      cursor.newRow().add(s.file).add(s.emojis.joinToString(",")).add("")
    }
    return cursor
  }

  override fun getType(uri: Uri): String? =
    when (matches.match(uri)) {
      ALL -> "vnd.android.cursor.dir/vnd.$authority.metadata"
      ONE -> "vnd.android.cursor.item/vnd.$authority.metadata"
      STICKERS -> "vnd.android.cursor.dir/vnd.$authority.stickers"
      FILE -> {
        val parts = uri.pathSegments
        if (parts.size == 3) WhatsappIndex.mimeType(parts[2]) else null
      }
      else -> null
    }

  override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
    require(mode == "r") { "solo lectura" }
    require(matches.match(uri) == FILE) { "URI no válida" }
    val parts = segments(uri, 3)
    val file =
      index().file(parts[1], parts[2]) ?: throw IllegalArgumentException("URI no válida")
    return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
  }

  override fun openAssetFile(uri: Uri, mode: String): AssetFileDescriptor =
    AssetFileDescriptor(openFile(uri, mode), 0, AssetFileDescriptor.UNKNOWN_LENGTH)

  override fun insert(uri: Uri, values: ContentValues?): Uri? =
    throw UnsupportedOperationException("solo lectura")

  override fun update(
    uri: Uri,
    values: ContentValues?,
    selection: String?,
    selectionArgs: Array<String>?,
  ): Int = throw UnsupportedOperationException("solo lectura")

  override fun delete(uri: Uri, selection: String?, selectionArgs: Array<String>?): Int =
    throw UnsupportedOperationException("solo lectura")

  private companion object {
    const val ALL = 1
    const val ONE = 2
    const val STICKERS = 3
    const val FILE = 4

    val METADATA_COLUMNS =
      arrayOf(
        "sticker_pack_identifier",
        "sticker_pack_name",
        "sticker_pack_publisher",
        "sticker_pack_icon",
        "android_play_store_link",
        "ios_app_download_link",
        "sticker_pack_publisher_email",
        "sticker_pack_publisher_website",
        "sticker_pack_privacy_policy_website",
        "sticker_pack_license_agreement_website",
        "image_data_version",
        "whatsapp_will_not_cache_stickers",
        "animated_sticker_pack",
      )
    val STICKER_COLUMNS = arrayOf("sticker_file_name", "sticker_emoji", "sticker_accessibility_text")
  }
}
