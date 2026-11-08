//! Renames a world inside `level.dat`, which is what Minecraft shows.
//!
//! The file is an 8-byte header (format version, then payload length, both
//! little-endian i32) followed by little-endian NBT. Only the root's
//! `LevelName` string tag changes, so the bytes are edited in place instead
//! of parsing and rewriting the whole tree.

use std::path::Path;

const HEADER_LEN: usize = 8;
const TAG_STRING: u8 = 0x08;
const NAME: &[u8] = b"LevelName";

/// Where the `LevelName` value's length prefix starts, and how long it is.
fn find_level_name(data: &[u8]) -> Result<(usize, usize), String> {
    let mut tag = vec![TAG_STRING];
    tag.extend_from_slice(&(NAME.len() as u16).to_le_bytes());
    tag.extend_from_slice(NAME);

    let mut matches = data.windows(tag.len()).enumerate().filter(|(_, window)| *window == tag.as_slice());
    let (at, _) = matches.next().ok_or("level.dat no tiene nombre de mundo.")?;
    if matches.next().is_some() {
        return Err("level.dat tiene un formato inesperado; no se tocó.".to_owned());
    }
    let length_at = at + tag.len();
    let length = data
        .get(length_at..length_at + 2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) as usize)
        .ok_or("level.dat está incompleto.")?;
    if length_at + 2 + length > data.len() {
        return Err("level.dat está incompleto.".to_owned());
    }
    Ok((length_at, length))
}

fn check_header(data: &[u8]) -> Result<(), String> {
    let payload = data
        .get(4..HEADER_LEN)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .ok_or("level.dat está incompleto.")?;
    if payload != data.len() - HEADER_LEN {
        return Err("level.dat tiene un formato inesperado; no se tocó.".to_owned());
    }
    Ok(())
}

pub fn read_name(data: &[u8]) -> Result<String, String> {
    check_header(data)?;
    let (at, length) = find_level_name(data)?;
    Ok(String::from_utf8_lossy(&data[at + 2..at + 2 + length]).into_owned())
}

pub fn with_name(data: &[u8], name: &str) -> Result<Vec<u8>, String> {
    check_header(data)?;
    let (at, old_length) = find_level_name(data)?;
    let new = name.as_bytes();
    let new_length = u16::try_from(new.len()).map_err(|_| "Ese nombre es demasiado largo.")?;

    let mut out = Vec::with_capacity(data.len() + new.len());
    out.extend_from_slice(&data[..at]);
    out.extend_from_slice(&new_length.to_le_bytes());
    out.extend_from_slice(new);
    out.extend_from_slice(&data[at + 2 + old_length..]);

    let payload = u32::try_from(out.len() - HEADER_LEN).map_err(|_| "level.dat demasiado grande.")?;
    out[4..HEADER_LEN].copy_from_slice(&payload.to_le_bytes());
    Ok(out)
}

/// Renames the world in both `level.dat` and `levelname.txt`. Written to a
/// temporary file and swapped in, so a crash can't leave it half-written.
pub fn rename_world(world_dir: &Path, name: &str) -> Result<(), String> {
    let path = world_dir.join("level.dat");
    let data = std::fs::read(&path).map_err(|e| format!("No se pudo leer level.dat: {e}"))?;
    let updated = with_name(&data, name)?;
    if read_name(&updated)? != name {
        return Err("No se pudo cambiar el nombre en level.dat.".to_owned());
    }

    let temp = path.with_extension("dat.tmp");
    std::fs::write(&temp, &updated).map_err(|e| format!("No se pudo guardar level.dat: {e}"))?;
    std::fs::rename(&temp, &path).map_err(|e| format!("No se pudo guardar level.dat: {e}"))?;
    std::fs::write(world_dir.join("levelname.txt"), name)
        .map_err(|e| format!("No se pudo guardar levelname.txt: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny level.dat: header plus a root compound with a few tags.
    fn sample(name: &str) -> Vec<u8> {
        let mut nbt = vec![0x0A, 0x00, 0x00]; // root compound, empty name
        nbt.extend_from_slice(&[0x03, 0x04, 0x00]);
        nbt.extend_from_slice(b"Time");
        nbt.extend_from_slice(&7i32.to_le_bytes());
        nbt.push(TAG_STRING);
        nbt.extend_from_slice(&(NAME.len() as u16).to_le_bytes());
        nbt.extend_from_slice(NAME);
        nbt.extend_from_slice(&(name.len() as u16).to_le_bytes());
        nbt.extend_from_slice(name.as_bytes());
        nbt.extend_from_slice(&[0x01, 0x04, 0x00]);
        nbt.extend_from_slice(b"Flag");
        nbt.push(1);
        nbt.push(0x00); // end of compound

        let mut data = 10i32.to_le_bytes().to_vec();
        data.extend_from_slice(&(nbt.len() as u32).to_le_bytes());
        data.extend_from_slice(&nbt);
        data
    }

    #[test]
    fn renames_and_fixes_the_header_length() {
        let data = sample("Bedrock level");
        assert_eq!(read_name(&data).unwrap(), "Bedrock level");

        let renamed = with_name(&data, "Mi mundo épico").unwrap();
        assert_eq!(read_name(&renamed).unwrap(), "Mi mundo épico");
        assert_eq!(renamed.len(), data.len() + "Mi mundo épico".len() - "Bedrock level".len());
        // Tags after the name are untouched.
        assert!(renamed.ends_with(&[0x01, 0x04, 0x00, b'F', b'l', b'a', b'g', 1, 0x00]));
    }

    #[test]
    fn refuses_files_it_does_not_understand() {
        assert!(read_name(b"corto").is_err());

        let mut wrong_length = sample("A");
        wrong_length[4] ^= 0xFF;
        assert!(with_name(&wrong_length, "B").is_err());

        let mut twice = sample("A");
        let tail = twice[HEADER_LEN + 3..].to_vec();
        twice.extend_from_slice(&tail);
        let payload = (twice.len() - HEADER_LEN) as u32;
        twice[4..8].copy_from_slice(&payload.to_le_bytes());
        assert!(with_name(&twice, "B").unwrap_err().contains("inesperado"));
    }

    #[test]
    fn rename_world_updates_both_files() {
        let dir = std::env::temp_dir().join(format!("mcbc-leveldat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("level.dat"), sample("Viejo")).unwrap();

        rename_world(&dir, "Nuevo").unwrap();

        assert_eq!(read_name(&std::fs::read(dir.join("level.dat")).unwrap()).unwrap(), "Nuevo");
        assert_eq!(std::fs::read_to_string(dir.join("levelname.txt")).unwrap(), "Nuevo");
        assert!(!dir.join("level.dat.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
