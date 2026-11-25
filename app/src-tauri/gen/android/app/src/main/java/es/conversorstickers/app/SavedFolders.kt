package es.conversorstickers.app

/** Folders the app saves into, which "Abrir carpeta" can open. `kind` comes from Rust, not trusted. */
object SavedFolders {
  /** `documentId` is what the external storage provider calls the folder; `label` is what the user sees. */
  data class Folder(val documentId: String, val label: String)

  private val FOLDERS =
    mapOf(
      "videos" to Folder("primary:${SavedVideos.FOLDER}", "Películas/ConversorStickers"),
      "sounds" to Folder("primary:${SavedSounds.FOLDER}", "Música/ConversorStickers"),
    )

  /** `null` if `kind` is not one of the folders above. */
  fun of(kind: String): Folder? = FOLDERS[kind]

  fun failure(folder: Folder): String = "No se pudo abrir la carpeta. Está en ${folder.label}."
}
