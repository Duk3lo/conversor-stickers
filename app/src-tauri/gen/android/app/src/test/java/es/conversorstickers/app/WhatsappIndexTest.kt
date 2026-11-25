package es.conversorstickers.app

import java.io.File
import java.nio.file.Files
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class WhatsappIndexTest {
  @get:Rule val temp = TemporaryFolder()
  private lateinit var folder: File

  /** The `index` section of the JSON that the Rust test also reads. */
  private val example: String by lazy {
    val examples = File("../../../../src/contract-mobile.examples.json").readText()
    JSONObject(examples).getJSONObject("index").toString()
  }

  @Before
  fun prepare() {
    folder = temp.newFolder("whatsapp")
    val pack = File(folder, "pack_3").apply { mkdir() }
    for (name in listOf("01.webp", "02.webp", "03.webp", "tray.png")) {
      File(pack, name).writeText("x")
    }
    File(pack, "99.webp").writeText("sin listar")
    File(folder, "library.json").writeText("secreto")
  }

  private fun index() = WhatsappIndex.parse(example, folder)

  @Test
  fun readsTheRustExample() {
    val pack = index().pack("pack_3")!!
    assertEquals("Gatos", pack.name)
    assertTrue(pack.animated)
    assertEquals("4", pack.imageVersion)
    assertEquals(listOf("01.webp", "02.webp", "03.webp"), pack.stickers.map { it.file })
    assertEquals(listOf("🙂"), pack.stickers[0].emojis)
  }

  @Test
  fun acceptsOnlyIdsOfTheRightShape() {
    assertNotNull(index().pack("pack_3"))
    for (bad in listOf("../x", "pack_3/..", "pack_", "pack_1234567890", "pack_3 ", "pack_9")) {
      assertNull(bad, index().pack(bad))
    }
    assertTrue(WhatsappIndex.isValidId("pack_123456789"))
    assertTrue(!WhatsappIndex.isValidId("pack_1234567890"))
  }

  @Test
  fun acceptsOnlyListedFiles() {
    val index = index()
    for (good in listOf("01.webp", "tray.png")) {
      assertNotNull(good, index.file("pack_3", good))
    }
    for (bad in listOf("../library.json", "1.webp", "01.webp/", "99.webp", "04.webp", "01.webp\u0000", "..", "")) {
      assertNull(bad, index.file("pack_3", bad))
    }
  }

  @Test
  fun symbolicLinksAreNotServed() {
    val pack = File(folder, "pack_3")
    File(pack, "02.webp").delete()
    Files.createSymbolicLink(File(pack, "02.webp").toPath(), File(folder, "library.json").toPath())
    File(pack, "03.webp").delete()
    Files.createSymbolicLink(File(pack, "03.webp").toPath(), File(pack, "01.webp").toPath())
    assertNull(index().file("pack_3", "02.webp"))
    assertNull(index().file("pack_3", "03.webp"))
    assertNotNull(index().file("pack_3", "01.webp"))
  }

  @Test
  fun aLinkedPackDirectoryIsNotServed() {
    val outside = temp.newFolder("outside")
    for (name in listOf("01.webp", "tray.png")) File(outside, name).writeText("x")
    File(folder, "pack_3").deleteRecursively()
    Files.createSymbolicLink(File(folder, "pack_3").toPath(), outside.toPath())
    assertNull(index().file("pack_3", "01.webp"))
  }

  @Test
  fun aBrokenIndexIsAnEmptyIndex() {
    for (broken in listOf("{ roto", "", "[]", """{"version":2,"packs":[]}""", """{"version":1}""")) {
      File(folder, "index.json").writeText(broken)
      assertTrue(broken, WhatsappIndex.read(folder).packs.isEmpty())
    }
    File(folder, "index.json").delete()
    assertTrue(WhatsappIndex.read(folder).packs.isEmpty())
  }

  @Test
  fun readsTheIndexFromDisk() {
    File(folder, "index.json").writeText(example)
    assertEquals(1, WhatsappIndex.read(folder).packs.size)
  }

  @Test
  fun mimeTypes() {
    assertEquals("image/webp", WhatsappIndex.mimeType("01.webp"))
    assertEquals("image/png", WhatsappIndex.mimeType("tray.png"))
    assertNull(WhatsappIndex.mimeType("../01.webp"))
  }

  @Test
  fun theFolderIsWhatsappInsideDataDir() {
    assertEquals(
      File("/data/user/0/es.conversorstickers.app/whatsapp"),
      WhatsappIndex.folder(File("/data/user/0/es.conversorstickers.app")),
    )
  }
}
