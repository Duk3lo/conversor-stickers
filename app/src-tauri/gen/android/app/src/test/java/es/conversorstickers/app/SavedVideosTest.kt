package es.conversorstickers.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SavedVideosTest {
  @Test
  fun theNameKeepsOnlySafeCharacters() {
    assertEquals("Gato_bailando.mp4", SavedVideos.displayName("Gato bailando.mp4"))
    assertEquals("etcpasswd.mp4", SavedVideos.displayName("../../etc/passwd.mp4"))
    assertEquals("a.mp4", SavedVideos.displayName("--a\u0000.mp4"))
  }

  @Test
  fun anEmptyNameBecomesVideoMp4() {
    assertEquals("video.mp4", SavedVideos.displayName("///"))
    assertEquals("video.mp4", SavedVideos.displayName(""))
  }

  @Test
  fun theNameIsShort() {
    assertEquals(80, SavedVideos.displayName("a".repeat(200) + ".mp4").length)
  }

  @Test
  fun onlyKnownVideoExtensionsHaveAMime() {
    assertEquals("video/mp4", SavedVideos.mime("a.MP4"))
    assertEquals("video/webm", SavedVideos.mime("a.webm"))
    assertEquals("video/x-matroska", SavedVideos.mime("a.mkv"))
    assertNull(SavedVideos.mime("a.exe"))
    assertNull(SavedVideos.mime("mp4"))
  }
}
