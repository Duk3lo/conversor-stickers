package es.conversorstickers.app

import es.conversorstickers.app.BridgeResponses.AddResponse
import es.conversorstickers.app.BridgeResponses.Target
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class BridgeResponsesTest {
  private val resultOk = -1

  @Test
  fun fourIntentCases() {
    assertEquals(AddResponse("added"), BridgeResponses.translateResult(resultOk, null))
    assertEquals(AddResponse("added"), BridgeResponses.translateResult(resultOk, "ignored"))
    assertEquals(AddResponse("cancelled"), BridgeResponses.translateResult(0, null))
    assertEquals(AddResponse("cancelled"), BridgeResponses.translateResult(0, " "))
    assertEquals(
      AddResponse("validation_error", "nombre largo"),
      BridgeResponses.translateResult(0, "nombre largo"),
    )
  }

  @Test
  fun targetDependsOnWhatIsInstalled() {
    assertEquals(Target.None, BridgeResponses.target(emptyList()))
    assertEquals(Target.None, BridgeResponses.target(listOf("com.other")))
    assertEquals(Target.One("com.whatsapp.w4b"), BridgeResponses.target(listOf("com.whatsapp.w4b")))
    assertEquals(Target.One("com.whatsapp"), BridgeResponses.target(listOf("com.whatsapp", "com.whatsapp")))
    assertEquals(Target.Choose, BridgeResponses.target(listOf("com.whatsapp", "com.whatsapp.w4b")))
  }

  @Test
  fun opensWhatsappOnlyWhenAskedAndUnambiguous() {
    assertEquals("com.whatsapp.w4b", BridgeResponses.packageToOpen(Target.One("com.whatsapp.w4b"), true))
    assertNull(BridgeResponses.packageToOpen(Target.One("com.whatsapp"), false))
    assertNull(BridgeResponses.packageToOpen(Target.Choose, true))
    assertNull(BridgeResponses.packageToOpen(Target.None, true))
  }

  @Test
  fun theClipboardGivesTheFirstImageOrVideo() {
    assertNull(BridgeResponses.pickMedia(emptyList()))
    assertNull(BridgeResponses.pickMedia(listOf(null to "text/plain", "content://a/1" to "text/plain")))
    assertNull(BridgeResponses.pickMedia(listOf("content://a/1" to null)))
    assertEquals(
      "content://a/2" to "video/mp4",
      BridgeResponses.pickMedia(
        listOf("content://a/1" to "text/uri-list", "content://a/2" to "video/mp4", "content://a/3" to "image/png")
      ),
    )
    assertEquals("content://a/3" to "image/png", BridgeResponses.pickMedia(listOf("content://a/3" to "image/png")))
  }
}
