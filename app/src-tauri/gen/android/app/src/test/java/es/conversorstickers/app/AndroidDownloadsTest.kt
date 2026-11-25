package es.conversorstickers.app

import java.util.concurrent.CountDownLatch
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidDownloadsTest {
  private class FakeEngine(val response: String) : DownloadEngine {
    val inside = CountDownLatch(1)
    val release = CountDownLatch(1)
    var cancelled = false

    var lastJob: DownloadJob? = null

    override fun download(job: DownloadJob): String {
      lastJob = job
      inside.countDown()
      release.await()
      return response
    }

    override fun progress(): String = """{"bytes":7,"total":null}"""

    override fun cancel() {
      cancelled = true
      release.countDown()
    }
  }

  private fun phase(downloads: AndroidDownloads) = JSONObject(downloads.status()).getString("phase")

  @Test
  fun withoutDownloadTheStateIsIdle() {
    assertEquals("idle", phase(AndroidDownloads(FakeEngine("{}"))))
  }

  @Test
  fun inProgressGivesProgressAndWhenFinishedTheResult() {
    val engine = FakeEngine("""{"result":"ready","path":"/c/v.mp4"}""")
    val downloads = AndroidDownloads(engine)
    downloads.start(DownloadJob("https://a.example/v", "/c"))
    engine.inside.await()
    val running = JSONObject(downloads.status())
    assertEquals("in_progress", running.getString("phase"))
    assertEquals(7, running.getInt("bytes"))
    engine.release.countDown()
    downloads.awaitFinish()
    val finished = JSONObject(downloads.status())
    assertEquals("ready", finished.getString("phase"))
    assertEquals("/c/v.mp4", finished.getString("path"))
  }

  @Test
  fun doesNotAllowTwoDownloadsAtOnce() {
    val engine = FakeEngine("""{"result":"cancelled"}""")
    val downloads = AndroidDownloads(engine)
    assertTrue(downloads.start(DownloadJob("https://a.example/1", "/c")))
    engine.inside.await()
    assertFalse(downloads.start(DownloadJob("https://a.example/2", "/c")))
    engine.release.countDown()
    downloads.awaitFinish()
  }

  @Test
  fun theJobWithItsModeAndFfmpegFoldersReachesTheEngine() {
    val engine = FakeEngine("""{"result":"cancelled"}""")
    val downloads = AndroidDownloads(engine)
    val job = DownloadJob("https://a.example/v", "/c", "save", "/lib", "/c/links")
    downloads.start(job)
    engine.inside.await()
    assertEquals(job, engine.lastJob)
    engine.release.countDown()
    downloads.awaitFinish()
  }

  @Test
  fun cancelNotifiesTheEngineAndTheResultIsCancelled() {
    val engine = FakeEngine("""{"result":"cancelled"}""")
    val downloads = AndroidDownloads(engine)
    downloads.start(DownloadJob("https://a.example/1", "/c"))
    engine.inside.await()
    downloads.cancel()
    downloads.awaitFinish()
    assertTrue(engine.cancelled)
    assertEquals("cancelled", phase(downloads))
  }

  @Test
  fun anUnexpectedEngineFailureGivesError() {
    val engine =
      object : DownloadEngine {
        override fun download(job: DownloadJob): String =
          throw IllegalStateException("python no arrancó")

        override fun progress() = "{}"

        override fun cancel() {}
      }
    val downloads = AndroidDownloads(engine)
    downloads.start(DownloadJob("https://a.example/1", "/c"))
    downloads.awaitFinish()
    val finished = JSONObject(downloads.status())
    assertEquals("error", finished.getString("phase"))
    assertTrue(finished.getString("message").contains("python no arrancó"))
  }
}
