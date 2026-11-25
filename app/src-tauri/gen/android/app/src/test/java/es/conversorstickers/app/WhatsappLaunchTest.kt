package es.conversorstickers.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class WhatsappLaunchTest {
  @Test
  fun regularWhatsappComesBeforeBusiness() {
    assertEquals(listOf("com.whatsapp", "com.whatsapp.w4b"), WhatsappLaunch.PACKAGES)
  }

  @Test
  fun theFirstPackageThatOpensWins() {
    val tried = mutableListOf<String>()
    val opened = WhatsappLaunch.firstOpened(WhatsappLaunch.PACKAGES) { pack ->
      tried += pack
      true
    }
    assertEquals("com.whatsapp", opened)
    assertEquals(listOf("com.whatsapp"), tried)
  }

  @Test
  fun fallsBackToBusinessWhenRegularCannotOpen() {
    val opened = WhatsappLaunch.firstOpened(WhatsappLaunch.PACKAGES) { pack -> pack == "com.whatsapp.w4b" }
    assertEquals("com.whatsapp.w4b", opened)
  }

  @Test
  fun nullWhenNeitherOpens() {
    assertNull(WhatsappLaunch.firstOpened(WhatsappLaunch.PACKAGES) { false })
  }

  @Test
  fun jidIsTheNumberOnTheWhatsappServer() {
    assertEquals("34612345678@s.whatsapp.net", WhatsappLaunch.jid("34612345678"))
  }

  @Test
  fun jidRejectsAnythingButEightToFifteenDigits() {
    assertNull(WhatsappLaunch.jid("1234567"))
    assertNull(WhatsappLaunch.jid("1234567890123456"))
    assertNull(WhatsappLaunch.jid("+34612345678"))
    assertNull(WhatsappLaunch.jid("34612345678@s.whatsapp.net"))
    assertNull(WhatsappLaunch.jid(""))
  }

  @Test
  fun sendFallsBackToBusinessWhenRegularCannotTakeIt() {
    val tried = mutableListOf<String>()
    val sent = WhatsappLaunch.firstOpened(WhatsappLaunch.PACKAGES) { pack ->
      tried += pack
      pack == "com.whatsapp.w4b"
    }
    assertEquals("com.whatsapp.w4b", sent)
    assertEquals(WhatsappLaunch.PACKAGES, tried)
  }
}
