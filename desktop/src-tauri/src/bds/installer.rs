//! Downloads the official server and unpacks it into a new folder.
//!
//! Everything happens in a staging folder next to the target, which is only
//! renamed into place once complete, so a failed or cancelled install leaves
//! nothing half-finished behind.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use super::EXECUTABLE;

// Both hosts serve the same undocumented list; the second is a fallback.
const LINK_ENDPOINTS: [&str; 2] = [
    "https://net-secondary.web.minecraft-services.net/api/v1.0/download/links",
    "https://net.web.minecraft-services.net/api/v1.0/download/links",
];
const WINDOWS_DOWNLOAD: &str = "serverBedrockWindows";
const OFFICIAL_DOMAIN: &str = "minecraft.net";
// The download host stalls requests with a curl-like User-Agent.
const USER_AGENT: &str = concat!("MCBedrockControl/", env!("CARGO_PKG_VERSION"));
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Download,
    Extract,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub url: Url,
    pub version: String,
}

#[derive(Deserialize)]
struct LinksResponse {
    result: Links,
}

#[derive(Deserialize)]
struct Links {
    links: Vec<Link>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Link {
    download_type: String,
    download_url: String,
}

fn cancelled() -> String {
    "Instalación cancelada.".to_owned()
}

fn is_official(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    url.scheme() == "https"
        && (host == OFFICIAL_DOMAIN || host.ends_with(&format!(".{OFFICIAL_DOMAIN}")))
}

fn parse_release(json: &str) -> Result<Release, String> {
    let response: LinksResponse =
        serde_json::from_str(json).map_err(|e| format!("Respuesta de descarga inválida: {e}"))?;
    let link = response
        .result
        .links
        .into_iter()
        .find(|link| link.download_type == WINDOWS_DOWNLOAD)
        .ok_or("Minecraft no ofrece una descarga para Windows.")?;

    let url = Url::parse(&link.download_url).map_err(|e| format!("Enlace inválido: {e}"))?;
    if !is_official(&url) {
        return Err(format!("El enlace de descarga no es oficial: {url}"));
    }
    let version = url
        .path_segments()
        .and_then(Iterator::last)
        .and_then(|file| file.strip_prefix("bedrock-server-"))
        .and_then(|file| file.strip_suffix(".zip"))
        .ok_or_else(|| format!("No se reconoce la versión en: {url}"))?
        .to_owned();
    Ok(Release { url, version })
}

fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("No se pudo preparar la descarga: {e}"))
}

async fn latest_release(client: &Client) -> Result<Release, String> {
    let mut last_error = String::new();
    for endpoint in LINK_ENDPOINTS {
        let body = async {
            client.get(endpoint).send().await?.error_for_status()?.text().await
        };
        match body.await {
            Ok(json) => return parse_release(&json),
            Err(e) => last_error = e.to_string(),
        }
    }
    Err(format!("No se pudo consultar la última versión: {last_error}"))
}

/// Installing only into a new or empty folder means an existing world or
/// `server.properties` can never be overwritten from here.
pub fn check_target(target: &Path) -> Result<(), String> {
    if target.file_name().is_none() {
        return Err("Elige una carpeta, no la raíz de un disco.".to_owned());
    }
    match std::fs::read_dir(target) {
        Ok(mut entries) => match entries.next() {
            Some(_) => Err("Esa carpeta ya tiene archivos. Elige una vacía.".to_owned()),
            None => Ok(()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("No se puede usar esa carpeta: {e}")),
    }
}

async fn download(
    client: &Client,
    url: Url,
    destination: &Path,
    cancel: &AtomicBool,
    report: &(impl Fn(Progress) + Sync),
) -> Result<(), String> {
    let failed = |e: reqwest::Error| format!("La descarga falló: {e}");
    let mut response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(failed)?;
    let total = response.content_length().unwrap_or(0);

    let write_failed = |e: std::io::Error| format!("No se pudo guardar la descarga: {e}");
    let mut file = tokio::fs::File::create(destination).await.map_err(write_failed)?;
    let mut done = 0;
    let mut last_report = Instant::now();
    report(Progress { phase: Phase::Download, done, total });

    while let Some(chunk) = response.chunk().await.map_err(failed)? {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        file.write_all(&chunk).await.map_err(write_failed)?;
        done += chunk.len() as u64;
        if last_report.elapsed() >= PROGRESS_INTERVAL {
            last_report = Instant::now();
            report(Progress { phase: Phase::Download, done, total });
        }
    }
    file.flush().await.map_err(write_failed)?;

    if total != 0 && done != total {
        return Err(format!("La descarga quedó incompleta ({done} de {total} bytes)."));
    }
    report(Progress { phase: Phase::Download, done, total });
    Ok(())
}

fn extract(
    archive: &Path,
    destination: &Path,
    cancel: &AtomicBool,
    report: impl Fn(Progress),
) -> Result<(), String> {
    let invalid = |e: zip::result::ZipError| format!("El archivo descargado no es válido: {e}");
    let write_failed = |e: std::io::Error| format!("No se pudo extraer el servidor: {e}");

    let file = File::open(archive).map_err(write_failed)?;
    let mut zip = zip::ZipArchive::new(file).map_err(invalid)?;
    let total = zip.len() as u64;
    let mut last_report = Instant::now();

    for index in 0..zip.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
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
            report(Progress { phase: Phase::Extract, done: index as u64 + 1, total });
        }
    }
    report(Progress { phase: Phase::Extract, done: total, total });

    if !destination.join(EXECUTABLE).is_file() {
        return Err(format!("El archivo descargado no contiene {EXECUTABLE}."));
    }
    Ok(())
}

fn staging_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".installing");
    target.with_file_name(name)
}

fn move_into_place(unpacked: &Path, target: &Path) -> Result<(), String> {
    let failed = |e: std::io::Error| format!("No se pudo finalizar la instalación: {e}");
    std::fs::write(unpacked.join("eula.txt"), "eula=true\n").map_err(failed)?;
    if target.exists() {
        std::fs::remove_dir(target).map_err(failed)?;
    }
    std::fs::rename(unpacked, target).map_err(failed)
}

/// Returns the installed version.
pub async fn install(
    target: &Path,
    cancel: Arc<AtomicBool>,
    report: impl Fn(Progress) + Send + Sync + 'static,
) -> Result<String, String> {
    check_target(target)?;

    let staging = staging_path(target);
    let _ = tokio::fs::remove_dir_all(&staging).await;
    tokio::fs::create_dir_all(&staging)
        .await
        .map_err(|e| format!("No se puede escribir en esa carpeta: {e}"))?;

    let result = install_into(&staging, target, cancel, report).await;
    let _ = tokio::fs::remove_dir_all(&staging).await;
    result
}

async fn install_into(
    staging: &Path,
    target: &Path,
    cancel: Arc<AtomicBool>,
    report: impl Fn(Progress) + Send + Sync + 'static,
) -> Result<String, String> {
    let client = client()?;
    let release = latest_release(&client).await?;

    let archive = staging.join("server.zip");
    download(&client, release.url, &archive, &cancel, &report).await?;

    let unpacked = staging.join("server");
    let extraction = {
        let unpacked = unpacked.clone();
        tokio::task::spawn_blocking(move || extract(&archive, &unpacked, &cancel, report))
    };
    extraction
        .await
        .map_err(|e| format!("La extracción se interrumpió: {e}"))??;

    move_into_place(&unpacked, target)?;
    Ok(release.version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    const LINKS: &str = r#"{"result":{"links":[
        {"downloadType":"serverBedrockLinux","downloadUrl":"https://www.minecraft.net/bedrockdedicatedserver/bin-linux/bedrock-server-1.26.61.1.zip"},
        {"downloadType":"serverBedrockWindows","downloadUrl":"https://www.minecraft.net/bedrockdedicatedserver/bin-win/bedrock-server-1.26.61.1.zip"}
    ]}}"#;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("mcbc-install-{name}-{}", std::process::id()));
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

    fn write_zip(path: &Path, entries: &[&str]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for name in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"data").unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn picks_the_windows_build_and_its_version() {
        let release = parse_release(LINKS).unwrap();
        assert_eq!(release.version, "1.26.61.1");
        assert!(release.url.path().contains("bin-win"));
    }

    #[test]
    fn rejects_links_outside_minecraft_net() {
        for url in [
            "https://evil.example/bedrock-server-1.0.zip",
            "https://notminecraft.net/bedrock-server-1.0.zip",
            "https://minecraft.net.evil.example/bedrock-server-1.0.zip",
            "http://www.minecraft.net/bedrock-server-1.0.zip",
        ] {
            let json = format!(
                r#"{{"result":{{"links":[{{"downloadType":"serverBedrockWindows","downloadUrl":"{url}"}}]}}}}"#
            );
            assert!(parse_release(&json).unwrap_err().contains("no es oficial"), "{url}");
        }
    }

    #[test]
    fn target_must_be_new_or_empty() {
        let dir = TempDir::new("target");
        assert!(check_target(&dir.0.join("nueva")).is_ok());
        assert!(check_target(&dir.0).is_ok());

        std::fs::write(dir.0.join("server.properties"), "x").unwrap();
        assert!(check_target(&dir.0).unwrap_err().contains("ya tiene archivos"));
        assert!(check_target(Path::new("C:\\")).is_err());
    }

    #[test]
    fn extracts_nested_files_and_requires_the_executable() {
        let dir = TempDir::new("extract");
        let archive = dir.0.join("a.zip");
        write_zip(&archive, &[EXECUTABLE, "behavior_packs/vanilla/manifest.json"]);

        let out = dir.0.join("ok");
        extract(&archive, &out, &AtomicBool::new(false), |_| {}).unwrap();
        assert!(out.join("behavior_packs/vanilla/manifest.json").is_file());

        write_zip(&archive, &["otra_cosa.txt"]);
        let error = extract(&archive, &dir.0.join("mal"), &AtomicBool::new(false), |_| {});
        assert!(error.unwrap_err().contains(EXECUTABLE));
    }

    #[test]
    fn refuses_paths_that_escape_the_folder() {
        let dir = TempDir::new("slip");
        let archive = dir.0.join("a.zip");
        write_zip(&archive, &["../fuera.txt", EXECUTABLE]);

        let error = extract(&archive, &dir.0.join("out"), &AtomicBool::new(false), |_| {});
        assert!(error.unwrap_err().contains("Ruta insegura"));
        assert!(!dir.0.join("fuera.txt").exists());
    }

    #[test]
    fn extraction_stops_when_cancelled() {
        let dir = TempDir::new("cancel");
        let archive = dir.0.join("a.zip");
        write_zip(&archive, &[EXECUTABLE]);

        let error = extract(&archive, &dir.0.join("out"), &AtomicBool::new(true), |_| {});
        assert_eq!(error.unwrap_err(), cancelled());
    }

    #[test]
    fn finishing_replaces_an_empty_target_and_records_the_eula() {
        let dir = TempDir::new("finish");
        let unpacked = dir.0.join("unpacked");
        let target = dir.0.join("servidor");
        std::fs::create_dir_all(&unpacked).unwrap();
        std::fs::create_dir_all(&target).unwrap();

        move_into_place(&unpacked, &target).unwrap();
        assert_eq!(std::fs::read_to_string(target.join("eula.txt")).unwrap(), "eula=true\n");
        assert!(!unpacked.exists());
    }
}
