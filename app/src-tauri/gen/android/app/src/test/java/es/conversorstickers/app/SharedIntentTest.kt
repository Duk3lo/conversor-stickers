package es.conversorstickers.app

import es.conversorstickers.app.SharedIntent.Media
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SharedIntentTest {
  private val send = SharedIntent.ACTION_SEND

  @Test
  fun aTextWithALinkGivesTheLink() {
    val r = SharedIntent.translate(send, "Mira esto https://v.redd.it/abc, mola", emptyList(), emptyList())
    assertEquals(listOf("https://v.redd.it/abc"), r.links)
    assertTrue(r.media.isEmpty())
  }

  @Test
  fun severalLinksWithoutRepeatsAndWithoutOtherSchemes() {
    val text = "http://a.example/1\nhttps://b.example/2 https://b.example/2 ftp://c.example/3 javascript:alert(1)"
    val r = SharedIntent.translate(send, text, emptyList(), emptyList())
    assertEquals(listOf("http://a.example/1", "https://b.example/2"), r.links)
  }

  @Test
  fun textWithoutLinksIsNothing() {
    assertTrue(SharedIntent.translate(send, "hola", emptyList(), emptyList()).isEmpty)
    assertTrue(SharedIntent.translate(send, null, emptyList(), emptyList()).isEmpty)
    assertTrue(SharedIntent.translate(send, "https://", emptyList(), emptyList()).isEmpty)
  }

  @Test
  fun filesOnlyIfTheyAreContent() {
    val r =
      SharedIntent.translate(
        SharedIntent.ACTION_SEND_MULTIPLE,
        null,
        listOf("content://media/1", "file:///data/x.mp4", "content://media/2"),
        listOf("video/mp4", "video/mp4", "video/*"),
      )
    assertEquals(listOf(Media("content://media/1", "video/mp4"), Media("content://media/2", null)), r.media)
  }

  @Test
  fun anotherActionIsNotTranslated() {
    val r = SharedIntent.translate("android.intent.action.VIEW", "https://a.example/x", listOf("content://m/1"), listOf("image/png"))
    assertTrue(r.isEmpty)
    assertTrue(SharedIntent.translate(null, "https://a.example/x", emptyList(), emptyList()).isEmpty)
  }
}
