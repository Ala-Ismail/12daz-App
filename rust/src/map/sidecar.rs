use std::{
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{MAP_SIDECAR_SCHEMA_VERSION, MapRegistration};

const MAX_BYTES: u64 = 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sidecar {
    version: u32,
    source_sha256: String,
    registration: MapRegistration,
    local_grid: bool,
}

fn path(archive: &Path) -> PathBuf {
    let mut name = archive.as_os_str().to_os_string();
    name.push(".12daz-map.json");
    name.into()
}

pub fn load(archive: &Path, source_hash: &str) -> io::Result<Option<(MapRegistration, bool)>> {
    let file = match File::open(path(archive)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Map sidecar is too large.",
        ));
    }
    let saved: Sidecar = serde_json::from_slice(&bytes)?;
    if saved.version != MAP_SIDECAR_SCHEMA_VERSION
        || saved.source_sha256 != source_hash
        || !saved.registration.crs().is_survey_grid()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Map sidecar version or survey fingerprint differs.",
        ));
    }
    Ok(Some((saved.registration, saved.local_grid)))
}

pub fn save(
    archive: &Path,
    source_hash: &str,
    registration: MapRegistration,
    local_grid: bool,
) -> io::Result<()> {
    let destination = path(archive);
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut replacement = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(
        &mut replacement,
        &Sidecar {
            version: MAP_SIDECAR_SCHEMA_VERSION,
            source_sha256: source_hash.to_owned(),
            registration,
            local_grid,
        },
    )?;
    replacement.write_all(b"\n")?;
    replacement.as_file().sync_all()?;
    replacement.persist(destination)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::map::{CrsId, SourceUnit};

    #[test]
    fn map_06_sidecar_is_bounded_versioned_and_bound_to_its_archive() {
        let folder = tempfile::tempdir().unwrap();
        let archive = folder.path().join("survey.12daz");
        std::fs::write(&archive, b"synthetic test archive").unwrap();
        let registration =
            MapRegistration::assign(CrsId::Gda2020MgaZone55, SourceUnit::Millimetres).unwrap();
        assert!(load(&archive, "source-a").unwrap().is_none());
        save(&archive, "source-a", registration, false).unwrap();
        assert_eq!(
            load(&archive, "source-a").unwrap(),
            Some((registration, false))
        );
        assert!(load(&archive, "source-b").is_err());
        let path = path(&archive);
        let saved = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, saved.replace("\"version\": 1", "\"version\": 2")).unwrap();
        assert!(load(&archive, "source-a").is_err());
        std::fs::write(&path, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        assert!(load(&archive, "source-a").is_err());
        save(&archive, "source-b", registration, true).unwrap();
        assert_eq!(
            load(&archive, "source-b").unwrap(),
            Some((registration, true))
        );
        assert_eq!(std::fs::read(&archive).unwrap(), b"synthetic test archive");
    }
}
