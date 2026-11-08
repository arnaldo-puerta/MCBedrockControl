//! Resource and behavior packs: importing `.mcpack`, `.mcaddon` and `.zip`
//! files, and turning packs on for the active world.
//!
//! A pack is a folder with a `manifest.json`. Which packs a world uses is
//! listed in `world_resource_packs.json` and `world_behavior_packs.json`
//! inside the world folder, as `{"pack_id": uuid, "version": [x, y, z]}`.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{archive, worlds};

const MAX_NESTING: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PackKind {
    Resource,
    Behavior,
}

impl PackKind {
    fn folder(self) -> &'static str {
        match self {
            PackKind::Resource => "resource_packs",
            PackKind::Behavior => "behavior_packs",
        }
    }

    fn world_file(self) -> &'static str {
        match self {
            PackKind::Resource => "world_resource_packs.json",
            PackKind::Behavior => "world_behavior_packs.json",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub kind: PackKind,
    pub folder: String,
    pub name: String,
    pub description: String,
    pub uuid: String,
    pub version: Vec<u64>,
    pub enabled: bool,
    /// Uses beta scripting APIs, which only run with the world's
    /// experimental features turned on.
    pub needs_experiments: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Manifest {
    kind: PackKind,
    name: String,
    description: String,
    uuid: String,
    version: Vec<u64>,
    needs_experiments: bool,
}

/// Packs that ship with the server. They are never listed or removed here.
pub fn is_builtin(folder: &str) -> bool {
    ["vanilla", "chemistry", "experimental_", "server_"].iter().any(|prefix| folder.starts_with(prefix))
        || folder == "editor"
}

/// Pack names are often translation keys like `pack.name`; those aren't
/// readable, so the folder name is used instead.
fn readable(text: &str) -> Option<String> {
    let text = text.trim();
    let is_key = !text.contains(' ') && text.contains('.') && text.chars().all(|c| c.is_ascii_graphic());
    (!text.is_empty() && !is_key).then(|| text.to_owned())
}

fn parse_manifest(text: &str) -> Option<Manifest> {
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let header = &value["header"];
    let uuid = header["uuid"].as_str()?.trim().to_lowercase();
    let version: Vec<u64> = header["version"].as_array()?.iter().filter_map(Value::as_u64).collect();

    let module_types: Vec<&str> =
        value["modules"].as_array()?.iter().filter_map(|m| m["type"].as_str()).collect();
    let kind = if module_types.contains(&"resources") {
        PackKind::Resource
    } else if module_types.iter().any(|t| matches!(*t, "data" | "script" | "javascript")) {
        PackKind::Behavior
    } else {
        // Skin packs and world templates don't go on a server.
        return None;
    };

    let needs_experiments = value["dependencies"].as_array().into_iter().flatten().any(|dep| {
        dep["module_name"].as_str().is_some_and(|m| m.starts_with("@minecraft/"))
            && dep["version"].as_str().is_some_and(|v| v.contains("beta"))
    });

    Some(Manifest {
        kind,
        name: header["name"].as_str().and_then(readable).unwrap_or_default(),
        description: header["description"].as_str().and_then(readable).unwrap_or_default(),
        uuid,
        version,
        needs_experiments,
    })
}

fn read_manifest(pack_dir: &Path) -> Option<Manifest> {
    parse_manifest(&std::fs::read_to_string(pack_dir.join("manifest.json")).ok()?)
}

fn world_dir(server_dir: &Path) -> Option<PathBuf> {
    let folder = worlds::active(server_dir)?;
    let dir = worlds::worlds_dir(server_dir).join(folder);
    dir.join("level.dat").is_file().then_some(dir)
}

fn read_world_list(world_dir: &Path, kind: PackKind) -> Result<Vec<Value>, String> {
    let path = world_dir.join(kind.world_file());
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("{} no es válido: {e}", kind.world_file())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("No se pudo leer {}: {e}", kind.world_file())),
    }
}

fn write_world_list(world_dir: &Path, kind: PackKind, list: &[Value]) -> Result<(), String> {
    let path = world_dir.join(kind.world_file());
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    std::fs::write(&path, text + "\n").map_err(|e| format!("No se pudo guardar {}: {e}", kind.world_file()))
}

fn same_pack(entry: &Value, uuid: &str) -> bool {
    entry["pack_id"].as_str().is_some_and(|id| id.eq_ignore_ascii_case(uuid))
}

/// Turns a pack on or off for the active world. Turning on an enabled pack
/// updates its version, which is what a pack update needs.
pub fn set_enabled(server_dir: &Path, kind: PackKind, uuid: &str, version: &[u64], enabled: bool) -> Result<(), String> {
    let world = world_dir(server_dir).ok_or("No hay un mundo activo. Inicia el servidor una vez para crearlo.")?;
    let mut list = read_world_list(&world, kind)?;
    list.retain(|entry| !same_pack(entry, uuid));
    if enabled {
        list.push(json!({ "pack_id": uuid, "version": version }));
    }
    write_world_list(&world, kind, &list)
}

pub fn list(server_dir: &Path) -> Result<Vec<PackInfo>, String> {
    let world = world_dir(server_dir);
    let mut packs = Vec::new();
    for kind in [PackKind::Resource, PackKind::Behavior] {
        let enabled = match &world {
            Some(world) => read_world_list(world, kind)?,
            None => Vec::new(),
        };
        let Ok(entries) = std::fs::read_dir(server_dir.join(kind.folder())) else { continue };
        for entry in entries.flatten() {
            let folder = entry.file_name().to_string_lossy().into_owned();
            if is_builtin(&folder) {
                continue;
            }
            let Some(manifest) = read_manifest(&entry.path()) else { continue };
            packs.push(PackInfo {
                kind,
                name: if manifest.name.is_empty() { folder.clone() } else { manifest.name },
                enabled: enabled.iter().any(|e| same_pack(e, &manifest.uuid)),
                folder,
                description: manifest.description,
                uuid: manifest.uuid,
                version: manifest.version,
                needs_experiments: manifest.needs_experiments,
            });
        }
    }
    packs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(packs)
}

/// Every folder holding a `manifest.json`, without descending into packs
/// already found (their subpacks are part of them).
fn find_pack_roots(dir: &Path, roots: &mut Vec<PathBuf>) {
    if dir.join("manifest.json").is_file() {
        roots.push(dir.to_owned());
        return;
    }
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.path().is_dir() {
            find_pack_roots(&entry.path(), roots);
        }
    }
}

/// `.mcaddon` files often hold `.mcpack` files instead of folders, so
/// archives found inside are unpacked too.
fn unpack_nested(dir: &Path, depth: usize) -> Result<(), String> {
    if depth >= MAX_NESTING {
        return Ok(());
    }
    let mut pending = vec![dir.to_owned()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
            if matches!(extension.as_str(), "mcpack" | "mcaddon" | "zip") {
                let out = path.with_extension(format!("{extension}-contenido"));
                archive::extract(&path, &out, &AtomicBool::new(false), |_, _| {})?;
                let _ = std::fs::remove_file(&path);
                unpack_nested(&out, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn existing_folder(server_dir: &Path, kind: PackKind, uuid: &str) -> Option<String> {
    std::fs::read_dir(server_dir.join(kind.folder()))
        .ok()?
        .flatten()
        .find(|entry| read_manifest(&entry.path()).is_some_and(|m| m.uuid == uuid))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
}

/// Installs every pack in `file` and turns it on for the active world.
/// A pack already installed (same uuid) is replaced by the new version.
pub fn import(server_dir: &Path, file: &Path) -> Result<Vec<PackInfo>, String> {
    let staging = server_dir.join(format!(".mcbc-packs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let result = (|| {
        archive::extract(file, &staging, &AtomicBool::new(false), |_, _| {})?;
        unpack_nested(&staging, 0)?;

        let mut roots = Vec::new();
        find_pack_roots(&staging, &mut roots);
        let packs: Vec<(PathBuf, Manifest)> =
            roots.into_iter().filter_map(|root| read_manifest(&root).map(|m| (root, m))).collect();
        if packs.is_empty() {
            return Err("Ese archivo no contiene packs de texturas ni de comportamiento.".to_owned());
        }

        let has_world = world_dir(server_dir).is_some();
        let mut installed = Vec::new();
        for (root, manifest) in packs {
            let parent = server_dir.join(manifest.kind.folder());
            std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
            let folder = match existing_folder(server_dir, manifest.kind, &manifest.uuid) {
                Some(folder) => {
                    std::fs::remove_dir_all(parent.join(&folder))
                        .map_err(|e| format!("No se pudo reemplazar el pack {folder}: {e}"))?;
                    folder
                }
                None => {
                    let fallback = root.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    let base = if manifest.name.is_empty() { fallback } else { manifest.name.clone() };
                    let folder = worlds::unique_folder(&parent, &base);
                    if is_builtin(&folder) { worlds::unique_folder(&parent, &format!("Pack {folder}")) } else { folder }
                }
            };
            std::fs::rename(&root, parent.join(&folder)).map_err(|e| format!("No se pudo instalar el pack: {e}"))?;
            if has_world {
                set_enabled(server_dir, manifest.kind, &manifest.uuid, &manifest.version, true)?;
            }
            installed.push(manifest.uuid);
        }
        let all = list(server_dir)?;
        Ok(all.into_iter().filter(|pack| installed.contains(&pack.uuid)).collect())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

pub fn remove(server_dir: &Path, kind: PackKind, folder: &str) -> Result<(), String> {
    let folder = worlds::plain_name(folder)?;
    if is_builtin(folder) {
        return Err("Ese pack viene con el servidor y no se puede quitar.".to_owned());
    }
    let dir = server_dir.join(kind.folder()).join(folder);
    if let Some(manifest) = read_manifest(&dir) {
        if world_dir(server_dir).is_some() {
            set_enabled(server_dir, kind, &manifest.uuid, &manifest.version, false)?;
        }
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("No se pudo quitar el pack: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    struct TempServer(PathBuf);

    impl TempServer {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("mcbc-packs-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("worlds").join("Mi mundo")).unwrap();
            std::fs::write(dir.join("worlds").join("Mi mundo").join("level.dat"), "nivel").unwrap();
            std::fs::write(dir.join("server.properties"), "level-name=Mi mundo\n").unwrap();
            std::fs::create_dir_all(dir.join("resource_packs").join("vanilla")).unwrap();
            std::fs::write(
                dir.join("resource_packs").join("vanilla").join("manifest.json"),
                manifest("resources", "resourcePack.vanilla.name", "0575c61f-a5da-4b7f-9961-ffda2908861e", [1, 26, 61]),
            )
            .unwrap();
            Self(dir)
        }

        fn world_list(&self, kind: PackKind) -> Vec<Value> {
            read_world_list(&self.0.join("worlds").join("Mi mundo"), kind).unwrap()
        }
    }

    impl Drop for TempServer {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn manifest(module: &str, name: &str, uuid: &str, version: [u64; 3]) -> String {
        json!({
            "format_version": 2,
            "header": { "name": name, "description": "Prueba", "uuid": uuid, "version": version },
            "modules": [{ "type": module, "uuid": "11111111-1111-1111-1111-111111111111", "version": [1, 0, 0] }],
        })
        .to_string()
    }

    fn zip_with(path: &Path, files: &[(&str, Vec<u8>)]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, bytes) in files {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn mcpack(dir: &Path, name: &str, module: &str, uuid: &str, version: [u64; 3]) -> PathBuf {
        let path = dir.join(format!("{name}.mcpack"));
        zip_with(&path, &[("manifest.json", manifest(module, name, uuid, version).into_bytes())]);
        path
    }

    const TEXTURES: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    const ADDON_RP: &str = "aaaaaaaa-0000-0000-0000-000000000002";
    const ADDON_BP: &str = "aaaaaaaa-0000-0000-0000-000000000003";

    #[test]
    fn importing_a_mcpack_installs_and_enables_it() {
        let server = TempServer::new("mcpack");
        let file = mcpack(&server.0, "Texturas HD", "resources", TEXTURES, [1, 0, 0]);

        let installed = import(&server.0, &file).unwrap();

        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].kind, PackKind::Resource);
        assert_eq!(installed[0].folder, "Texturas HD");
        assert!(installed[0].enabled);
        assert!(server.0.join("resource_packs/Texturas HD/manifest.json").is_file());
        assert_eq!(server.world_list(PackKind::Resource), [json!({ "pack_id": TEXTURES, "version": [1, 0, 0] })]);
    }

    #[test]
    fn a_mcaddon_with_nested_mcpacks_installs_both_halves() {
        let server = TempServer::new("mcaddon");
        let rp = mcpack(&server.0, "Dragones RP", "resources", ADDON_RP, [1, 0, 0]);
        let bp = mcpack(&server.0, "Dragones BP", "data", ADDON_BP, [1, 0, 0]);
        let addon = server.0.join("Dragones.mcaddon");
        zip_with(&addon, &[
            ("Dragones RP.mcpack", std::fs::read(&rp).unwrap()),
            ("Dragones BP.mcpack", std::fs::read(&bp).unwrap()),
        ]);

        let installed = import(&server.0, &addon).unwrap();

        assert_eq!(installed.len(), 2);
        assert!(server.0.join("resource_packs/Dragones RP").is_dir());
        assert!(server.0.join("behavior_packs/Dragones BP").is_dir());
        assert_eq!(server.world_list(PackKind::Behavior).len(), 1);
        assert_eq!(server.world_list(PackKind::Resource).len(), 1);
    }

    #[test]
    fn a_zip_with_pack_folders_works_too() {
        let server = TempServer::new("folders");
        let file = server.0.join("addon.zip");
        zip_with(&file, &[
            ("Mi Addon/RP/manifest.json", manifest("resources", "pack.name", ADDON_RP, [1, 0, 0]).into_bytes()),
            ("Mi Addon/RP/textures/a.png", b"png".to_vec()),
            ("Mi Addon/BP/manifest.json", manifest("data", "Comportamiento", ADDON_BP, [1, 0, 0]).into_bytes()),
        ]);

        let installed = import(&server.0, &file).unwrap();

        // "pack.name" is a translation key, so the folder name is used.
        assert!(installed.iter().any(|p| p.folder == "RP"));
        assert!(server.0.join("resource_packs/RP/textures/a.png").is_file());
        assert!(installed.iter().any(|p| p.folder == "Comportamiento"));
    }

    #[test]
    fn importing_a_newer_version_replaces_the_pack() {
        let server = TempServer::new("update");
        import(&server.0, &mcpack(&server.0, "Texturas HD", "resources", TEXTURES, [1, 0, 0])).unwrap();
        let newer = server.0.join("v2");
        std::fs::create_dir_all(&newer).unwrap();

        import(&server.0, &mcpack(&newer, "Texturas HD", "resources", TEXTURES, [2, 0, 0])).unwrap();

        let packs = list(&server.0).unwrap();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].version, [2, 0, 0]);
        assert_eq!(server.world_list(PackKind::Resource), [json!({ "pack_id": TEXTURES, "version": [2, 0, 0] })]);
    }

    #[test]
    fn lists_only_the_players_packs_and_toggles_them() {
        let server = TempServer::new("toggle");
        import(&server.0, &mcpack(&server.0, "Texturas HD", "resources", TEXTURES, [1, 0, 0])).unwrap();

        let packs = list(&server.0).unwrap();
        assert_eq!(packs.len(), 1, "vanilla must stay hidden");

        set_enabled(&server.0, PackKind::Resource, TEXTURES, &[1, 0, 0], false).unwrap();
        assert!(!list(&server.0).unwrap()[0].enabled);
        assert!(server.world_list(PackKind::Resource).is_empty());

        remove(&server.0, PackKind::Resource, "Texturas HD").unwrap();
        assert!(list(&server.0).unwrap().is_empty());
        assert!(remove(&server.0, PackKind::Resource, "vanilla").is_err());
    }

    #[test]
    fn keeps_other_entries_in_the_world_list() {
        let server = TempServer::new("keep");
        let world = server.0.join("worlds/Mi mundo");
        std::fs::write(world.join("world_resource_packs.json"), r#"[{"pack_id":"otro","version":[1,0,0]}]"#).unwrap();

        import(&server.0, &mcpack(&server.0, "Texturas HD", "resources", TEXTURES, [1, 0, 0])).unwrap();

        assert_eq!(server.world_list(PackKind::Resource).len(), 2);
    }

    #[test]
    fn rejects_files_without_packs_and_skin_packs() {
        let server = TempServer::new("none");
        let file = server.0.join("nada.zip");
        zip_with(&file, &[("leeme.txt", b"hola".to_vec())]);
        assert!(import(&server.0, &file).unwrap_err().contains("no contiene packs"));

        let skins = mcpack(&server.0, "Skins", "skin_pack", TEXTURES, [1, 0, 0]);
        assert!(import(&server.0, &skins).is_err());
    }

    #[test]
    fn flags_packs_that_need_experiments() {
        let text = json!({
            "header": { "name": "Script", "uuid": ADDON_BP, "version": [1, 0, 0] },
            "modules": [{ "type": "script" }],
            "dependencies": [{ "module_name": "@minecraft/server", "version": "2.1.0-beta" }],
        })
        .to_string();
        let manifest = parse_manifest(&text).unwrap();
        assert_eq!(manifest.kind, PackKind::Behavior);
        assert!(manifest.needs_experiments);
    }

    #[test]
    fn recognises_the_servers_own_packs() {
        for folder in ["vanilla", "vanilla_1.26.61", "chemistry_1.21.0", "editor", "experimental_poi", "server_library"] {
            assert!(is_builtin(folder), "{folder}");
        }
        assert!(!is_builtin("Mis texturas"));
    }
}
