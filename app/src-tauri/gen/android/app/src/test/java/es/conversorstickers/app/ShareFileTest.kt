package es.conversorstickers.app

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ShareFileTest {
  @get:Rule val temporary = TemporaryFolder()

  private fun inShareFolder(name: String): File {
    val folder = File(temporary.root, ShareFile.DIRECTORY).also { it.mkdirs() }
    return File(folder, name).also { it.writeText("webp") }
  }

  @Test
  fun aPreparedCopyCanBeShared() {
    assertTrue(ShareFile.isShareable(temporary.root, inShareFolder("sticker-3.webp")))
  }

  @Test
  fun otherNamesOrFoldersAreRejected() {
    assertFalse(ShareFile.isShareable(temporary.root, inShareFolder("library.json")))
    assertFalse(ShareFile.isShareable(temporary.root, inShareFolder("sticker-x.webp")))
    val outside = temporary.newFile("sticker-1.webp")
    assertFalse(ShareFile.isShareable(temporary.root, outside))
    val escaping = File(temporary.root, "${ShareFile.DIRECTORY}/../sticker-1.webp")
    assertFalse(ShareFile.isShareable(temporary.root, escaping))
    assertFalse(ShareFile.isShareable(temporary.root, File(temporary.root, "${ShareFile.DIRECTORY}/sticker-9.webp")))
  }
}
