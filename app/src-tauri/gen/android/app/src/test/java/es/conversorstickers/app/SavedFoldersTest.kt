package es.conversorstickers.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SavedFoldersTest {
  @Test
  fun videosAndSoundsMapToTheFoldersTheyAreSavedIn() {
    assertEquals("primary:Movies/ConversorStickers", SavedFolders.of("videos")?.documentId)
    assertEquals("primary:Music/ConversorStickers", SavedFolders.of("sounds")?.documentId)
  }

  @Test
  fun anyOtherKindIsRejected() {
    for (kind in listOf("", "stickers", "Videos", "../Download", "primary:DCIM")) {
      assertNull(kind, SavedFolders.of(kind))
    }
  }

  @Test
  fun theFailureNamesTheFolderInSpanish() {
    assertEquals(
      "No se pudo abrir la carpeta. Está en Películas/ConversorStickers.",
      SavedFolders.failure(SavedFolders.of("videos")!!),
    )
    assertEquals(
      "No se pudo abrir la carpeta. Está en Música/ConversorStickers.",
      SavedFolders.failure(SavedFolders.of("sounds")!!),
    )
  }
}
