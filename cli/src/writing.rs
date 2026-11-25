use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use tracing::warn;

pub(crate) fn preview_path(output: &Path) -> PathBuf {
    output.with_extension("preview.webp")
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_by_temp(path, bytes, |temp| fs::rename(temp, path))
}

pub(crate) fn write_without_overwrite(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_by_temp(path, bytes, |temp| fs::hard_link(temp, path))
}

fn write_by_temp(
    path: &Path,
    bytes: &[u8],
    publish: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "la salida no es un nombre de archivo",
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
        publish(&temp)
    })();
    if let Err(error) = fs::remove_file(&temp)
        && error.kind() != ErrorKind::NotFound
    {
        warn!(%error, "no se pudo borrar el temporal de la salida");
    }
    result
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn logs_of(task: impl FnOnce()) -> String {
        #[derive(Clone)]
        struct Memory(Arc<Mutex<Vec<u8>>>);
        impl Write for Memory {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let memory = Memory(Arc::default());
        let writer = memory.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, task);
        String::from_utf8(memory.0.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn if_not_can_delete_temp_log_not_carries_name_of_output() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Mi_cumpleanos_con_Ana.webp");
        let logs = logs_of(|| {
            write_by_temp(&path, b"x", |temp| {
                fs::remove_file(temp)?;
                fs::create_dir(temp)
            })
            .unwrap();
        });
        assert!(logs.contains("no se pudo borrar el temporal"), "{logs}");
        assert!(!logs.contains("Mi_cumpleanos_con_Ana"), "{logs}");
    }

    #[test]
    fn path_of_preview_goes_together_to_output_with_preview_webp() {
        assert_eq!(
            preview_path(Path::new("/tmp/gato.webp")),
            Path::new("/tmp/gato.preview.webp")
        );
        assert_eq!(
            preview_path(Path::new("gato.gif")),
            Path::new("gato.preview.webp")
        );
        assert_eq!(
            preview_path(Path::new("dir/gato")),
            Path::new("dir/gato.preview.webp")
        );
    }

    #[test]
    fn write_atomic_leaves_only_file_final() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gato.webp");
        write_atomic(&path, b"primero").unwrap();
        write_atomic(&path, b"segundo").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"segundo");
        let names: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .map(|input| input.unwrap().file_name())
            .collect();
        assert_eq!(names, ["gato.webp"]);
    }

    #[test]
    fn write_in_directory_that_does_not_exist_returns_error_and_leaves_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("no-existe").join("gato.webp");
        let error = write_atomic(&path, b"x").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn write_without_overwrite_creates_file_if_not_exists() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gato.gif");
        write_without_overwrite(&path, b"nuevo").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"nuevo");
        let names: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .map(|input| input.unwrap().file_name())
            .collect();
        assert_eq!(names, ["gato.gif"]);
    }

    #[test]
    fn destination_that_appears_before_of_write_does_not_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gato.gif");
        fs::write(&path, b"de otro").unwrap();
        let error = write_without_overwrite(&path, b"nuevo").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&path).unwrap(), b"de otro");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn path_without_name_of_file_returns_error() {
        let error = write_atomic(Path::new("/"), b"x").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
    }
}
