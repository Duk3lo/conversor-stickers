//! Atomic writes: a saved file is never left half-written. Copy of `cli/src/writing.rs`: the core
//! does not write files and a shared crate just for this would be one more piece.

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use tracing::warn;

/// Tells apart the temp files of several writes in the same process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to a temp file in the same directory as `path` and renames it: rename is atomic
/// within one filesystem. An existing file is replaced (the save dialog already asked).
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "la ruta no es un nombre de archivo",
        )
    })?;
    let directory = match path.parent() {
        Some(directory) if !directory.as_os_str().is_empty() => directory.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let temp = directory.join(format!(
        ".{}.{}-{}.tmp",
        name.to_string_lossy(),
        process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if let Err(source) = result {
        if let Err(error) = fs::remove_file(&temp)
            && error.kind() != ErrorKind::NotFound
        {
            warn!(
                file = %name.to_string_lossy(),
                %error,
                "no se pudo borrar el temporal del archivo guardado"
            );
        }
        return Err(source);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_leaves_only_the_final_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gato.webp");
        write_atomic(&path, b"primero").unwrap();
        write_atomic(&path, b"segundo").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"segundo");
        let names: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["gato.webp"]);
    }

    #[test]
    fn write_in_missing_directory_returns_error_and_leaves_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("no-existe").join("gato.webp");
        let error = write_atomic(&path, b"x").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn path_without_file_name_returns_error() {
        let error = write_atomic(Path::new("/"), b"x").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
    }
}
