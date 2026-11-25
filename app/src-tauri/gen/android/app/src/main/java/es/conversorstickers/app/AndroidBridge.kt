package es.conversorstickers.app

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.util.Log
import android.webkit.WebView
import androidx.activity.result.ActivityResult
import androidx.annotation.RequiresApi
import androidx.core.content.FileProvider
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.io.IOException
import org.json.JSONArray
import org.json.JSONObject

@InvokeArg
class WhatsappPack {
  lateinit var id: String
  lateinit var name: String
  /** Open WhatsApp when the pack is added, so the user only has to pick a chat. */
  var open_after: Boolean = false
}

@InvokeArg
class DownloadRequest {
  lateinit var link: String
  lateinit var destination: String
  var mode: String = "sticker"
}

@InvokeArg
class SaveVideoRequest {
  lateinit var path: String
  lateinit var name: String
}

@InvokeArg
class ShareRequest {
  lateinit var path: String
}

@InvokeArg
class FolderRequest {
  lateinit var kind: String
}

/** What only Android knows, for Rust. Rust calls it with `run_mobile_plugin`; the webview cannot. */
@TauriPlugin
class AndroidBridge(private val activity: Activity) : Plugin(activity) {
  private var webView: WebView? = null
  private val pending = Pending()
  /** Package to open once `addResult` says "added"; null if WhatsApp should stay as it is. */
  private var openAfterAdd: String? = null
  private val downloads by lazy { AndroidDownloads(PythonEngine(activity.applicationContext)) }

  /** What was shared and Rust has not collected yet. */
  private class Pending {
    private var content = SharedIntent.NOTHING

    @Synchronized
    fun add(new: SharedIntent.Shared) {
      content =
        SharedIntent.Shared(
          (content.links + new.links).distinct(),
          content.media + new.media,
        )
    }

    @Synchronized
    fun take(): SharedIntent.Shared = content.also { content = SharedIntent.NOTHING }
  }

  override fun load(webView: WebView) {
    this.webView = webView
    // If the app was opened from "Share", the interface asks for it on startup.
    receive(activity.intent)
  }

  override fun onNewIntent(intent: Intent) {
    if (!receive(intent).isEmpty) {
      webView?.post { webView?.evaluateJavascript(SHARED_EVENT, null) }
    }
  }

  private fun receive(intent: Intent?): SharedIntent.Shared {
    if (intent == null) return SharedIntent.NOTHING
    val uris = uris(intent)
    val mimes = uris.map { activity.contentResolver.getType(Uri.parse(it)) ?: intent.type }
    val shared =
      SharedIntent.translate(intent.action, intent.getStringExtra(Intent.EXTRA_TEXT), uris, mimes)
    pending.add(shared)
    // An intent that was already received is not read again if Android recreates the activity.
    intent.action = null
    return shared
  }

  @Suppress("DEPRECATION")
  private fun uris(intent: Intent): List<String> =
    when (intent.action) {
      Intent.ACTION_SEND -> listOfNotNull(intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)?.toString())
      Intent.ACTION_SEND_MULTIPLE ->
        intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM).orEmpty().map { it.toString() }
      else -> emptyList()
    }

  /** What other apps shared: links and `content://` URIs. Delivered only once. */
  @Command
  fun take_shared(invoke: Invoke) {
    val shared = pending.take()
    val response = JSObject()
    response.put("links", JSONArray(shared.links))
    val media = JSONArray()
    shared.media.forEach {
      media.put(JSONObject().put("uri", it.uri).put("mime", it.mime ?: JSONObject.NULL))
    }
    response.put("media", media)
    invoke.resolve(response)
  }

  /**
   * Starts downloading a link with yt-dlp into `destination`, which has to be inside the app
   * cache. It does not wait: Rust polls `download_status`.
   */
  @Command
  fun start_download(invoke: Invoke) {
    val request = invoke.parseArgs(DownloadRequest::class.java)
    val cache = activity.cacheDir.canonicalFile
    val destination = File(request.destination).canonicalFile
    if (!request.link.startsWith("https://") && !request.link.startsWith("http://")) {
      invoke.reject("enlace no válido")
      return
    }
    if (!destination.path.startsWith(cache.path + File.separator) || !destination.isDirectory) {
      invoke.reject("destino fuera de la caché")
      return
    }
    if (request.mode != "sticker" && request.mode != "save") {
      invoke.reject("modo de descarga no válido")
      return
    }
    val job =
      DownloadJob(
        request.link,
        destination.path,
        request.mode,
        activity.applicationInfo.nativeLibraryDir,
        File(cache, "ffmpeg-links").path,
      )
    if (!downloads.start(job)) {
      invoke.reject("ya hay una descarga en curso")
      return
    }
    invoke.resolve()
  }

  /**
   * Copies a downloaded video (inside the app cache) to `Movies/ConversorStickers` through
   * MediaStore, which needs no permission from Android 10. Answers the final file name.
   */
  @Command
  fun save_to_movies(invoke: Invoke) {
    val request = invoke.parseArgs(SaveVideoRequest::class.java)
    val file = File(request.path).canonicalFile
    if (!file.path.startsWith(activity.cacheDir.canonicalFile.path + File.separator) || !file.isFile) {
      invoke.reject("el vídeo no está en la caché de la app")
      return
    }
    val name = SavedVideos.displayName(request.name)
    val mime = SavedVideos.mime(name)
    if (mime == null) {
      invoke.reject("tipo de vídeo no admitido")
      return
    }
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
      invoke.reject("guardar vídeos necesita Android 10 o superior")
      return
    }
    // Up to 500 MB: not on the thread that dispatches commands.
    Thread {
        try {
          invoke.resolve(JSObject().put("name", copyToMovies(file, name, mime)))
        } catch (e: IOException) {
          invoke.reject("no se pudo guardar el vídeo: ${e.message}")
        }
      }
      .start()
  }

  @RequiresApi(Build.VERSION_CODES.Q)
  private fun copyToMovies(file: File, name: String, mime: String): String {
    val resolver = activity.contentResolver
    val values =
      ContentValues().apply {
        put(MediaStore.Video.Media.DISPLAY_NAME, name)
        put(MediaStore.Video.Media.MIME_TYPE, mime)
        put(MediaStore.Video.Media.RELATIVE_PATH, SavedVideos.FOLDER)
        put(MediaStore.Video.Media.IS_PENDING, 1)
      }
    val uri =
      resolver.insert(MediaStore.Video.Media.EXTERNAL_CONTENT_URI, values)
        ?: throw IOException("Android no creó el archivo")
    try {
      val output = resolver.openOutputStream(uri) ?: throw IOException("no se pudo abrir el destino")
      output.use { out -> file.inputStream().use { it.copyTo(out) } }
      resolver.update(
        uri,
        ContentValues().apply { put(MediaStore.Video.Media.IS_PENDING, 0) },
        null,
        null,
      )
    } catch (e: IOException) {
      resolver.delete(uri, null, null)
      throw e
    }
    // MediaStore renames on a clash ("name (1).mp4"); the real name is what the user will see.
    return resolver.query(uri, arrayOf(MediaStore.Video.Media.DISPLAY_NAME), null, null, null)?.use {
      if (it.moveToFirst()) it.getString(0) else null
    } ?: name
  }

  /**
   * Copies a sound cut from a video (inside the app cache) to `Music/ConversorStickers` through
   * MediaStore. Answers the final file name.
   */
  @Command
  fun save_to_music(invoke: Invoke) {
    val request = invoke.parseArgs(SaveVideoRequest::class.java)
    val file = File(request.path).canonicalFile
    if (!file.path.startsWith(activity.cacheDir.canonicalFile.path + File.separator) || !file.isFile) {
      invoke.reject("el sonido no está en la caché de la app")
      return
    }
    val name = SavedSounds.displayName(request.name)
    val mime = SavedSounds.mime(name)
    if (mime == null) {
      invoke.reject("tipo de sonido no admitido")
      return
    }
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
      invoke.reject("guardar sonidos necesita Android 10 o superior")
      return
    }
    // At most 10 seconds of audio, but still not on the thread that dispatches commands.
    Thread {
        try {
          invoke.resolve(JSObject().put("name", copyToMusic(file, name, mime)))
        } catch (e: IOException) {
          invoke.reject("no se pudo guardar el sonido: ${e.message}")
        }
      }
      .start()
  }

  @RequiresApi(Build.VERSION_CODES.Q)
  private fun copyToMusic(file: File, name: String, mime: String): String {
    val resolver = activity.contentResolver
    val values =
      ContentValues().apply {
        put(MediaStore.Audio.Media.DISPLAY_NAME, name)
        put(MediaStore.Audio.Media.MIME_TYPE, mime)
        put(MediaStore.Audio.Media.RELATIVE_PATH, SavedSounds.FOLDER)
        put(MediaStore.Audio.Media.IS_PENDING, 1)
      }
    val uri =
      resolver.insert(MediaStore.Audio.Media.EXTERNAL_CONTENT_URI, values)
        ?: throw IOException("Android no creó el archivo")
    try {
      val output = resolver.openOutputStream(uri) ?: throw IOException("no se pudo abrir el destino")
      output.use { out -> file.inputStream().use { it.copyTo(out) } }
      resolver.update(
        uri,
        ContentValues().apply { put(MediaStore.Audio.Media.IS_PENDING, 0) },
        null,
        null,
      )
    } catch (e: IOException) {
      resolver.delete(uri, null, null)
      throw e
    }
    return resolver.query(uri, arrayOf(MediaStore.Audio.Media.DISPLAY_NAME), null, null, null)?.use {
      if (it.moveToFirst()) it.getString(0) else null
    } ?: name
  }

  @Command
  fun download_status(invoke: Invoke) {
    invoke.resolve(JSObject(downloads.status()))
  }

  @Command
  fun cancel_download(invoke: Invoke) {
    downloads.cancel()
    invoke.resolve()
  }

  companion object {
    /** The interface listens to this event and asks for `receive_shared`. */
    const val SHARED_EVENT = "window.dispatchEvent(new Event('android-shared'))"
  }

  /** Directory where Android extracted `libffmpeg.so` and `libffprobe.so`. */
  @Command
  fun paths(invoke: Invoke) {
    val libraries = activity.applicationInfo.nativeLibraryDir
    val hasFfmpeg = File(libraries, "libffmpeg.so").isFile
    val hasFfprobe = File(libraries, "libffprobe.so").isFile
    Log.d("AndroidBridge", "paths: $libraries ffmpeg=$hasFfmpeg ffprobe=$hasFfprobe")
    val response = JSObject()
    response.put("native_libraries", libraries)
    invoke.resolve(response)
  }

  /**
   * Launches the WhatsApp intent to add the pack. The authority is our own provider's and does
   * not come from outside. Answers `not_installed` if there is no WhatsApp; otherwise answers
   * when WhatsApp gives control back (`addResult`).
   */
  @Command
  fun add_whatsapp_pack(invoke: Invoke) {
    val pack = invoke.parseArgs(WhatsappPack::class.java)
    if (!WhatsappIndex.isValidId(pack.id)) {
      invoke.reject("identificador de paquete no válido")
      return
    }
    val installed =
      listOf(BridgeResponses.WHATSAPP, BridgeResponses.WHATSAPP_BUSINESS).filter(::isInstalled)
    val target = BridgeResponses.target(installed)
    if (target is BridgeResponses.Target.None) {
      invoke.resolve(response(BridgeResponses.AddResponse("not_installed")))
      return
    }
    val intent =
      Intent("com.whatsapp.intent.action.ENABLE_STICKER_PACK")
        .putExtra("sticker_pack_id", pack.id)
        .putExtra("sticker_pack_authority", "${activity.packageName}.stickers")
        .putExtra("sticker_pack_name", pack.name)
    openAfterAdd = BridgeResponses.packageToOpen(target, pack.open_after)
    val launched =
      if (target is BridgeResponses.Target.One) intent.setPackage(target.pack)
      else Intent.createChooser(intent, "Añadir a WhatsApp")
    try {
      startActivityForResult(invoke, launched, "addResult")
    } catch (_: ActivityNotFoundException) {
      invoke.resolve(response(BridgeResponses.AddResponse("not_installed")))
    }
  }

  /** Brings WhatsApp to the front; if it cannot be opened the user just sees the usual notice. */
  private fun openApp(pack: String) {
    val launch = activity.packageManager.getLaunchIntentForPackage(pack) ?: return
    try {
      activity.startActivity(launch)
    } catch (_: ActivityNotFoundException) {}
  }

  /**
   * Brings WhatsApp to the front, so the user can pick the saved video under Stickers. Tries the
   * regular app first, then Business. Rejects if neither can be launched.
   */
  @Command
  fun open_whatsapp(invoke: Invoke) {
    val opened =
      WhatsappLaunch.firstOpened(WhatsappLaunch.PACKAGES) { pack ->
        activity.packageManager.getLaunchIntentForPackage(pack)?.let { start(it) } ?: false
      }
    if (opened != null) invoke.resolve() else invoke.reject("No se encontró WhatsApp instalado")
  }

  @ActivityCallback
  fun addResult(invoke: Invoke, result: ActivityResult) {
    val error = result.data?.getStringExtra("validation_error")
    val translated = BridgeResponses.translateResult(result.resultCode, error)
    val toOpen = openAfterAdd.takeIf { translated.result == "added" }
    openAfterAdd = null
    if (toOpen != null) openApp(toOpen)
    invoke.resolve(response(translated))
  }

  /**
   * Opens the system share sheet with a sticker copy that Rust left in the share folder of the
   * cache. The other app gets read access to that one URI only.
   */
  @Command
  fun share_file(invoke: Invoke) {
    val request = invoke.parseArgs(ShareRequest::class.java)
    val file = File(request.path)
    if (!ShareFile.isShareable(activity.cacheDir, file)) {
      invoke.reject("archivo no permitido para compartir")
      return
    }
    val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
    val send =
      Intent(Intent.ACTION_SEND)
        .setType(ShareFile.MIME)
        .putExtra(Intent.EXTRA_STREAM, uri)
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    // The chooser passes the read grant on to the app picked only through the clip.
    send.clipData = ClipData.newRawUri("", uri)
    try {
      activity.startActivity(Intent.createChooser(send, "Compartir sticker"))
      invoke.resolve()
    } catch (failure: ActivityNotFoundException) {
      invoke.reject("no hay ninguna app para compartir")
    }
  }

  /**
   * Opens a folder the app saved into in the file manager. Tries the folder itself; if no app takes
   * it, the file manager at its root; if neither works it says where the folder is.
   */
  @Command
  fun open_saved_folder(invoke: Invoke) {
    val request = invoke.parseArgs(FolderRequest::class.java)
    val folder = SavedFolders.of(request.kind)
    if (folder == null) {
      invoke.reject("carpeta no válida")
      return
    }
    val uri =
      DocumentsContract.buildDocumentUri("com.android.externalstorage.documents", folder.documentId)
    val view =
      Intent(Intent.ACTION_VIEW)
        .setDataAndType(uri, DocumentsContract.Document.MIME_TYPE_DIR)
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
    if (start(view) || openFileManager()) invoke.resolve() else invoke.reject(SavedFolders.failure(folder))
  }

  private fun start(intent: Intent): Boolean =
    try {
      activity.startActivity(intent)
      true
    } catch (_: ActivityNotFoundException) {
      false
    }

  private fun openFileManager(): Boolean =
    listOf("com.google.android.documentsui", "com.android.documentsui").any { pack ->
      activity.packageManager.getLaunchIntentForPackage(pack)?.let { start(it) } ?: false
    }

  /** The image or video on the clipboard, if any. Only read when "Paste" is pressed. */
  @Command
  fun read_clipboard(invoke: Invoke) {
    val clipboard = activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
    val clip = clipboard.primaryClip
    val candidates =
      (0 until (clip?.itemCount ?: 0)).map { i ->
        val uri = clip!!.getItemAt(i).uri
        val mime =
          uri?.let { activity.contentResolver.getType(it) }
            ?: clip.description.takeIf { it.mimeTypeCount > i }?.getMimeType(i)
        uri?.toString() to mime
      }
    val media = BridgeResponses.pickMedia(candidates)
    val response = JSObject()
    response.put("uri", media?.first ?: JSONObject.NULL)
    response.put("mime", media?.second ?: JSONObject.NULL)
    invoke.resolve(response)
  }

  private fun isInstalled(pack: String): Boolean =
    try {
      activity.packageManager.getPackageInfo(pack, 0)
      true
    } catch (_: PackageManager.NameNotFoundException) {
      false
    }

  private fun response(r: BridgeResponses.AddResponse): JSObject {
    val result = JSObject()
    result.put("result", r.result)
    if (r.reason != null) result.put("reason", r.reason)
    return result
  }
}
