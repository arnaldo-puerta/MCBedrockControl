//! ZIP helpers shared by installs, world imports and backups.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

pub const CANCELLED: &str = "Operación cancelada.";
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Unpacks `archive` into `destination`, refusing entries that would land
/// outside it. `report` gets (entries done, total entries).
pub fn extract(
    archive: &Path,
    destination: &Path,
    cancel: &AtomicBool,
    report: impl Fn(u64, u64),
) -> Result<(), String> {
    let invalid = |e: zip::result::ZipError| format!("El archivo no es un ZIP válido: {e}");
    let write_failed = |e: std::io::Error| format!("No se pudo extraer el archivo: {e}");

    let file = File::open(archive).map_err(write_failed)?;
    let mut zip = zip::ZipArchive::new(file).map_err(invalid)?;
    let total = zip.len() as u64;
    let mut last_report = Instant::now();

    for index in 0..zip.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.to_owned());
        }
        let mut entry = zip.by_index(index).map_err(invalid)?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("Ruta insegura en el archivo: {}", entry.name()))?;
        let path = destination.join(relative);

        if entry.is_dir() {
            std::fs::create_dir_all(&path).map_err(write_failed)?;
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(write_failed)?;
            }
            let mut output = File::create(&path).map_err(write_failed)?;
            std::io::copy(&mut entry, &mut output).map_err(write_failed)?;
        }

        if last_report.elapsed() >= PROGRESS_INTERVAL {
            last_report = Instant::now();
            report(index as u64 + 1, total);
        }
    }
    report(total, total);
    Ok(())
}

/// Every file under `dir`, relative to it, with its size.
pub fn list_files(dir: &Path) -> Result<Vec<(PathBuf, u64)>, String> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let entries = std::fs::read_dir(&current)
            .map_err(|e| format!("No se pudo leer {}: {e}", current.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let metadata = entry.metadata().map_err(|e| e.to_string())?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                let relative = entry.path().strip_prefix(dir).expect("walked from dir").to_owned();
                files.push((relative, metadata.len()));
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Zips the first `length` bytes of each listed file under `root`. The
/// archive is written beside `destination` and renamed in when complete.
pub fn write(root: &Path, files: &[(PathBuf, u64)], destination: &Path) -> Result<(), String> {
    let failed = |e: std::io::Error| format!("No se pudo crear el ZIP: {e}");
    let zip_failed = |e: zip::result::ZipError| format!("No se pudo crear el ZIP: {e}");
    let temp = destination.with_extension("tmp");

    let result = (|| {
        let mut zip = zip::ZipWriter::new(File::create(&temp).map_err(failed)?);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(true);
        for (relative, length) in files {
            let name = relative.to_string_lossy().replace('\\', "/");
            zip.start_file(name, options).map_err(zip_failed)?;
            let source = File::open(root.join(relative)).map_err(failed)?;
            std::io::copy(&mut source.take(*length), &mut zip).map_err(failed)?;
        }
        zip.finish().map_err(zip_failed)?;
        std::fs::rename(&temp, destination).map_err(failed)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("mcbc-archive-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn round_trips_nested_files_and_honours_lengths() {
        let dir = TempDir::new("roundtrip");
        let source = dir.0.join("mundo");
        std::fs::create_dir_all(source.join("db")).unwrap();
        std::fs::write(source.join("level.dat"), "nivel").unwrap();
        std::fs::write(source.join("db").join("000001.ldb"), "0123456789").unwrap();

        let mut files = list_files(&source).unwrap();
        assert_eq!(files.len(), 2);
        // Like `save query`, copy only the part of a file that is complete.
        files.iter_mut().find(|(p, _)| p.ends_with("000001.ldb")).unwrap().1 = 4;

        let archive = dir.0.join("copia.mcworld");
        write(&source, &files, &archive).unwrap();
        assert!(!archive.with_extension("tmp").exists());

        let out = dir.0.join("out");
        extract(&archive, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(std::fs::read_to_string(out.join("level.dat")).unwrap(), "nivel");
        assert_eq!(std::fs::read_to_string(out.join("db").join("000001.ldb")).unwrap(), "0123");
    }

    #[test]
    fn a_failed_write_leaves_no_partial_archive() {
        let dir = TempDir::new("partial");
        let archive = dir.0.join("copia.mcworld");
        let missing = [(PathBuf::from("no-existe.dat"), 10)];

        assert!(write(&dir.0, &missing, &archive).is_err());
        assert!(!archive.exists());
        assert!(!archive.with_extension("tmp").exists());
    }
}
