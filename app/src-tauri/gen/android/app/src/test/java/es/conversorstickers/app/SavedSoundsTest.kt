package es.conversorstickers.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SavedSoundsTest {
  @Test
  fun theNameKeepsOnlySafeCharacters() {
    assertEquals("Gato_bailando-sonido.m4a", SavedSounds.displayName("Gato bailando-sonido.m4a"))
    assertEquals("etcpasswd.m4a", SavedSounds.displayName("../../etc/passwd.m4a"))
  }

  @Test
  fun aLongNameIsTruncatedWithoutLosingTheExtension() {
    val clean = SavedSounds.displayName("a".repeat(120) + "-sonido.m4a")
    assertEquals("a".repeat(80) + ".m4a", clean)
    assertEquals("audio/mp4", SavedSounds.mime(clean))
  }

  @Test
  fun anEmptyNameBecomesSonidoM4a() {
    assertEquals("sonido.m4a", SavedSounds.displayName("///"))
  }

  @Test
  fun onlyKnownSoundExtensionsHaveAMime() {
    assertEquals("audio/mp4", SavedSounds.mime("a.M4A"))
    assertEquals("audio/webm", SavedSounds.mime("a.webm"))
    assertNull(SavedSounds.mime("a.mp4"))
    assertNull(SavedSounds.mime("m4a"))
  }
}
