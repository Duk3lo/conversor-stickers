package es.conversorstickers.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ExternalLinksTest {
  @Test
  fun onlyHttpsToKnownSitesOpens() {
    for (url in
      listOf(
        "https://github.com/Duk3lo/conversor-stickers-comunidad",
        "https://developers.giphy.com",
        "https://partner.klipy.com/x?a=1#b",
        "https://wa.me/34612345678",
      )) {
      assertEquals(url, ExternalLinks.openable(url))
    }
  }

  @Test
  fun anythingElseIsRefused() {
    for (url in
      listOf(
        "",
        "http://github.com/x",
        "https://example.com",
        "https://github.com.evil.example/",
        "https://github.com@evil.example/",
        "https://github.com:8443/",
        "https://github.com/a b",
        "intent://github.com#Intent;end",
        "javascript:alert(1)",
        "file:///etc/passwd",
      )) {
      assertNull(url, ExternalLinks.openable(url))
    }
    assertNull(ExternalLinks.openable("https://github.com/" + "a".repeat(2048)))
  }

  @Test
  fun aBotLinkComesOnlyFromAViewIntent() {
    val link = "conversorstickers://bot#u=https%3A%2F%2Fequipo.red.ts.net&t=abc"
    assertEquals(link, ExternalLinks.botLink("android.intent.action.VIEW", link))
    assertNull(ExternalLinks.botLink("android.intent.action.SEND", link))
    assertNull(ExternalLinks.botLink(null, link))
    assertNull(ExternalLinks.botLink("android.intent.action.VIEW", null))
    assertNull(ExternalLinks.botLink("android.intent.action.VIEW", "https://github.com"))
    assertNull(ExternalLinks.botLink("android.intent.action.VIEW", link + "a".repeat(1024)))
  }
}
