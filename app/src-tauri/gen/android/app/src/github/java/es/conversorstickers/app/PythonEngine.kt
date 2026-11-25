package es.conversorstickers.app

import com.chaquo.python.Python
import com.chaquo.python.android.AndroidPlatform

class PythonEngine(context: android.content.Context) : DownloadEngine {
  private val module by lazy {
    if (!Python.isStarted()) Python.start(AndroidPlatform(context))
    Python.getInstance().getModule("link_download")
  }

  override fun download(job: DownloadJob): String =
    module
      .callAttr("download", job.link, job.destination, job.mode, job.nativeLibraries, job.linksFolder)
      .toString()

  override fun progress(): String = module.callAttr("progress").toString()

  override fun cancel() {
    module.callAttr("cancel")
  }
}
