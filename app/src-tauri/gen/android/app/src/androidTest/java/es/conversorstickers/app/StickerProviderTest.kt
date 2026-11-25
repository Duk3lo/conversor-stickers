package es.conversorstickers.app

import android.content.ContentValues
import android.net.Uri
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** With the published index; the app calls its own provider, so the permission does not get in the way. */
class StickerProviderTest {
  private val context = InstrumentationRegistry.getInstrumentation().targetContext
  private val resolver = context.contentResolver
  private val folder = WhatsappIndex.folder(context.dataDir)
  private val backup = File(folder.parentFile, "whatsapp.backup")
  private val base = "content://${context.packageName}.stickers"

  @Before
  fun prepare() {
    backup.deleteRecursively()
    if (folder.exists()) check(folder.renameTo(backup))
    val pack = File(folder, "pack_3").apply { mkdirs() }
    File(pack, "01.webp").writeText("RIFFwebp")
    File(pack, "tray.png").writeText("png")
    File(folder, "index.json").writeText(
      """{"version":1,"packs":[{"id":"pack_3","name":"Gatos",
      "author":"Conversor de stickers","animated":true,"image_version":"4",
      "tray":"tray.png","stickers":[{"file":"01.webp","emojis":["🙂"]}]}]}"""
    )
    File(folder, "library.json").writeText("secreto")
  }

  @After
  fun cleanUp() {
    folder.deleteRecursively()
    if (backup.exists()) backup.renameTo(folder)
  }

  private fun uri(path: String) = Uri.parse("$base/$path")

  @Test
  fun metadataGivesColumnsAndValues() {
    resolver.query(uri("metadata"), null, null, null, null)!!.use {
      assertEquals(1, it.count)
      it.moveToFirst()
      assertEquals("pack_3", it.getString(it.getColumnIndexOrThrow("sticker_pack_identifier")))
      assertEquals("Gatos", it.getString(it.getColumnIndexOrThrow("sticker_pack_name")))
      assertEquals("tray.png", it.getString(it.getColumnIndexOrThrow("sticker_pack_icon")))
      assertEquals("4", it.getString(it.getColumnIndexOrThrow("image_data_version")))
      assertEquals(1, it.getInt(it.getColumnIndexOrThrow("animated_sticker_pack")))
      assertEquals(0, it.getInt(it.getColumnIndexOrThrow("whatsapp_will_not_cache_stickers")))
      assertEquals(13, it.columnCount)
    }
  }

  @Test
  fun stickersListsFilesWithTheirEmoji() {
    resolver.query(uri("stickers/pack_3"), null, null, null, null)!!.use {
      assertEquals(1, it.count)
      it.moveToFirst()
      assertEquals("01.webp", it.getString(0))
      assertEquals("🙂", it.getString(1))
    }
  }

  @Test
  fun opensBytesAndGivesTheType() {
    val file = uri("stickers_asset/pack_3/01.webp")
    resolver.openInputStream(file)!!.use { assertEquals("RIFFwebp", it.readBytes().decodeToString()) }
    assertEquals("image/webp", resolver.getType(file))
    assertEquals("image/png", resolver.getType(uri("stickers_asset/pack_3/tray.png")))
  }

  @Test
  fun rejectsTraversalBadIdsAndUnlistedFiles() {
    val bad =
      listOf(
        "stickers_asset/pack_3/..%2Flibrary.json",
        "stickers_asset/pack_3/%2E%2E%2Flibrary.json",
        "stickers_asset/..%2Fwhatsapp/library.json",
        "stickers_asset/pack_3/../../whatsapp/library.json",
        "stickers_asset/pack_3/library.json",
        "stickers_asset/pack_3/02.webp",
        "stickers_asset/pack_1234567890/01.webp",
        "stickers_asset/pack_3/01.webp/extra",
        "stickers/..%2Fx",
        "metadata/pack_",
      )
    for (path in bad) {
      val opened = runCatching { resolver.openInputStream(uri(path))?.use { it.readBytes() } }
      assertTrue("$path: ${opened.getOrNull()}", opened.isFailure || opened.getOrNull() == null)
      val query = runCatching { resolver.query(uri(path), null, null, null, null)?.use { it.count } }
      assertTrue("$path query", query.isFailure || query.getOrNull() == null)
    }
  }

  @Test
  fun opensOnlyForReading() {
    val file = uri("stickers_asset/pack_3/01.webp")
    for (mode in listOf("w", "rw", "wt", "wa")) {
      assertTrue(mode, runCatching { resolver.openFileDescriptor(file, mode)?.close() }.isFailure)
    }
    assertEquals("RIFFwebp", File(folder, "pack_3/01.webp").readText())
  }

  @Test
  fun insertUpdateAndDeleteThrow() {
    assertThrows(UnsupportedOperationException::class.java) {
      resolver.insert(uri("metadata"), ContentValues())
    }
    assertThrows(UnsupportedOperationException::class.java) {
      resolver.update(uri("metadata"), ContentValues(), null, null)
    }
    assertThrows(UnsupportedOperationException::class.java) { resolver.delete(uri("metadata"), null, null) }
  }

  @Test
  fun aBrokenIndexGivesNoPacks() {
    File(folder, "index.json").writeText("{ roto")
    resolver.query(uri("metadata"), null, null, null, null)!!.use { assertEquals(0, it.count) }
    assertNull(runCatching { resolver.openInputStream(uri("stickers_asset/pack_3/01.webp")) }.getOrNull())
  }
}
