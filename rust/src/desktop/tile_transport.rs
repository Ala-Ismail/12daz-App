use std::{
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use az12_converter::map::TileId;
use eframe::egui;
use serde::{Deserialize, Serialize};

const MAX_TILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 64 * 1024;
const MAX_DISK_TILES: usize = 384;
const OSM_DEFAULT_AGE: u64 = 7 * 24 * 60 * 60;
const USER_AGENT: &str = concat!(
    "12dAZ-Converter/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Ala-Ismail/12daz-App)"
);

#[derive(Clone, Copy)]
pub(super) enum Body {
    Metadata,
    Tile,
    OsmTile(TileId),
}

pub(super) enum Data {
    Json(Vec<u8>),
    Image(egui::ColorImage, Option<Duration>),
}

pub(super) struct Queue<K> {
    send: SyncSender<Job<K>>,
    receive: Receiver<(u64, K, Arc<AtomicBool>, Result<Data, String>)>,
    generation: Arc<AtomicU64>,
}

struct Job<K> {
    generation: u64,
    key: K,
    request: ehttp::Request,
    body: Body,
    ctx: egui::Context,
    cancelled: Arc<AtomicBool>,
}

impl<K: Send + 'static> Queue<K> {
    pub(super) fn new() -> Self {
        let (send, jobs) = mpsc::sync_channel::<Job<K>>(8);
        let (done, receive) = mpsc::channel();
        let jobs = Arc::new(Mutex::new(jobs));
        let generation = Arc::new(AtomicU64::new(0));
        for _ in 0..4 {
            let jobs = Arc::clone(&jobs);
            let done = done.clone();
            let active = Arc::clone(&generation);
            let _ = std::thread::Builder::new()
                .name("map tile".into())
                .spawn(move || {
                    loop {
                        let job = jobs.lock().ok().and_then(|queue| queue.recv().ok());
                        let Some(job) = job else { break };
                        if active.load(Ordering::Relaxed) != job.generation
                            || job.cancelled.load(Ordering::Relaxed)
                        {
                            continue;
                        }
                        let result = fetch(job.request, job.body);
                        if job.cancelled.load(Ordering::Relaxed) {
                            continue;
                        }
                        if done
                            .send((job.generation, job.key, job.cancelled, result))
                            .is_err()
                        {
                            break;
                        }
                        job.ctx.request_repaint();
                    }
                });
        }
        Self {
            send,
            receive,
            generation,
        }
    }

    pub(super) fn set_generation(&self, generation: u64) {
        self.generation.store(generation, Ordering::Relaxed);
    }

    pub(super) fn send(
        &self,
        generation: u64,
        key: K,
        request: ehttp::Request,
        body: Body,
        ctx: &egui::Context,
        cancelled: Arc<AtomicBool>,
    ) -> bool {
        self.send
            .try_send(Job {
                generation,
                key,
                request: request
                    .with_header("User-Agent", USER_AGENT)
                    .with_timeout(Some(Duration::from_secs(15))),
                body,
                ctx: ctx.clone(),
                cancelled,
            })
            .is_ok()
    }

    pub(super) fn results(
        &self,
    ) -> impl Iterator<Item = (u64, K, Arc<AtomicBool>, Result<Data, String>)> + '_ {
        self.receive.try_iter()
    }
}

#[derive(Default, Serialize, Deserialize)]
struct CacheInfo {
    expires: u64,
    etag: Option<String>,
    modified: Option<String>,
    no_store: bool,
}

impl CacheInfo {
    fn from_headers(headers: &ehttp::Headers, now: u64, fallback: u64) -> Self {
        let control = headers.get("Cache-Control").unwrap_or_default();
        let directives: Vec<_> = control.split(',').map(str::trim).collect();
        let age = headers
            .get("Age")
            .and_then(|age| age.parse::<u64>().ok())
            .unwrap_or(0);
        let max_age = directives.iter().find_map(|directive| {
            directive.split_once('=').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("max-age")
                    .then(|| value.trim_matches('"').parse::<u64>().ok())
                    .flatten()
            })
        });
        let expires = headers
            .get("Expires")
            .and_then(|value| chrono::DateTime::parse_from_rfc2822(value).ok())
            .and_then(|value| u64::try_from(value.timestamp()).ok());
        let lifetime = max_age
            .map(|age_limit| age_limit.saturating_sub(age))
            .or_else(|| expires.map(|expiry| expiry.saturating_sub(now)))
            .unwrap_or(fallback);
        let required = directives
            .iter()
            .any(|directive| directive.eq_ignore_ascii_case("no-cache"));
        Self {
            expires: now.saturating_add(if required { 0 } else { lifetime }),
            etag: headers
                .get("ETag")
                .filter(|value| value.len() <= 256)
                .map(str::to_owned),
            modified: headers
                .get("Last-Modified")
                .filter(|value| value.len() <= 256)
                .map(str::to_owned),
            no_store: directives
                .iter()
                .any(|directive| directive.eq_ignore_ascii_case("no-store")),
        }
    }
}

fn fetch(mut request: ehttp::Request, body: Body) -> Result<Data, String> {
    let cache = match body {
        Body::OsmTile(tile) => disk_path(tile),
        _ => None,
    };
    let now = unix_now();
    let cached = cache.as_deref().and_then(read_disk);
    if let Some((info, pixels)) = &cached {
        if info.expires > now
            && let Ok(pixels) = decode(pixels)
        {
            return Ok(Data::Image(pixels, age(info, now)));
        }
        if let Some(etag) = &info.etag {
            request = request.with_header("If-None-Match", etag);
        } else if let Some(date) = &info.modified {
            request = request.with_header("If-Modified-Since", date);
        }
    }
    let response = request
        .fetch_raw_native(true)
        .map_err(|_| "Could not reach the map provider.".to_owned())?;
    let (response, mut payload) = response.into_parts();
    if !response.status.is_success() && response.status.as_u16() != 304 {
        return Err(format!(
            "Map provider returned HTTP {}.",
            response.status.as_u16()
        ));
    }
    let headers = ehttp::Headers {
        headers: response
            .headers
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.to_string(), value.to_owned()))
            })
            .collect(),
    };
    let mut bytes = Vec::new();
    let maximum = match body {
        Body::Metadata => MAX_METADATA_BYTES,
        _ => MAX_TILE_BYTES,
    };
    payload
        .as_reader()
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the map response.".to_owned())?;
    if bytes.len() as u64 > maximum {
        return Err("Map provider response exceeded the size limit.".into());
    }
    if matches!(body, Body::Metadata) {
        return Ok(Data::Json(bytes));
    }
    let mut info = CacheInfo::from_headers(
        &headers,
        now,
        if cache.is_some() { OSM_DEFAULT_AGE } else { 0 },
    );
    if response.status.as_u16() == 304 {
        let (prior, bytes_from_cache) = cached.ok_or("Map server returned an empty tile.")?;
        bytes = bytes_from_cache;
        info.etag = info.etag.or(prior.etag);
        info.modified = info.modified.or(prior.modified);
    }
    let image = decode(&bytes)?;
    if let Some(path) = cache {
        if info.no_store {
            let _ = fs::remove_file(path);
        } else {
            write_disk(&path, &info, &bytes);
        }
    }
    Ok(Data::Image(image, age(&info, now)))
}

fn age(info: &CacheInfo, now: u64) -> Option<Duration> {
    (!info.no_store && info.expires > now).then(|| Duration::from_secs(info.expires - now))
}

fn decode(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    let error = || "Map provider returned an invalid or unsupported tile.".to_owned();
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| error())?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        return Err(error());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| error())?.into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        size,
        image.as_raw(),
    ))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn disk_path(tile: TileId) -> Option<PathBuf> {
    let root = PathBuf::from(
        #[cfg(debug_assertions)]
        std::env::var_os("AZ12_MAP_CAPTURE_CACHE").or_else(|| std::env::var_os("LOCALAPPDATA"))?,
        #[cfg(not(debug_assertions))]
        std::env::var_os("LOCALAPPDATA")?,
    );
    root.is_absolute()
        .then(|| {
            root.join("12dAZ-Converter")
                .join("map-tiles")
                .join("openstreetmap")
        })
        .map(|root| {
            root.join(format!(
                "{}-{}-{}.tile",
                tile.zoom().value(),
                tile.x(),
                tile.y()
            ))
        })
}

fn read_disk(path: &Path) -> Option<(CacheInfo, Vec<u8>)> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(MAX_TILE_BYTES + MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    let end = bytes.iter().position(|byte| *byte == b'\n')?;
    if end as u64 > MAX_METADATA_BYTES || (bytes.len() - end - 1) as u64 > MAX_TILE_BYTES {
        return None;
    }
    let info = serde_json::from_slice(&bytes[..end]).ok()?;
    Some((info, bytes.split_off(end + 1)))
}

fn write_disk(path: &Path, info: &CacheInfo, bytes: &[u8]) {
    let result = || -> std::io::Result<()> {
        let directory = path.parent().ok_or(std::io::ErrorKind::NotFound)?;
        fs::create_dir_all(directory)?;
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer(&mut file, info)?;
        file.write_all(b"\n")?;
        file.write_all(bytes)?;
        file.persist(path)?;
        static WRITES: AtomicUsize = AtomicUsize::new(0);
        if WRITES.fetch_add(1, Ordering::Relaxed).is_multiple_of(16) {
            prune(directory)?;
        }
        Ok(())
    };
    let _ = result();
}

fn prune(directory: &Path) -> std::io::Result<()> {
    let mut tiles: Vec<_> = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let file = entry.metadata().ok()?;
            (path.extension()?.to_str()? == "tile" && file.is_file())
                .then(|| (file.modified().unwrap_or(UNIX_EPOCH), path))
        })
        .take(MAX_DISK_TILES * 2)
        .collect();
    tiles.sort_unstable();
    for (_, path) in tiles
        .iter()
        .take(tiles.len().saturating_sub(MAX_DISK_TILES))
    {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_09_cache_honours_origin_headers_and_default_retention() {
        let now = 100;
        let cached = CacheInfo::from_headers(
            &ehttp::Headers::new(&[
                ("Cache-Control", "max-age=60"),
                ("Age", "15"),
                ("ETag", "fixture"),
            ]),
            now,
            OSM_DEFAULT_AGE,
        );
        assert_eq!(cached.expires, 145);
        assert_eq!(cached.etag.as_deref(), Some("fixture"));
        let fallback = CacheInfo::from_headers(&ehttp::Headers::default(), now, OSM_DEFAULT_AGE);
        assert_eq!(fallback.expires, now + OSM_DEFAULT_AGE);
        let restricted = CacheInfo::from_headers(
            &ehttp::Headers::new(&[("Cache-Control", "no-store, max-age=60")]),
            now,
            0,
        );
        assert!(restricted.no_store);
        assert!(age(&restricted, now).is_none());
    }

    #[test]
    fn map_12_rejects_non_image_and_corrupt_tiles() {
        assert!(decode(b"<html>failure</html>").is_err());
        assert!(decode(b"\x89PNG\r\n\x1a\n").is_err());
    }

    #[test]
    #[ignore = "contacts OpenStreetMap for one public synthetic test location"]
    fn map_13_openstreetmap_downloads_one_public_tile() {
        use az12_converter::map::{LonLat, OpenStreetMapStandard, TileProvider, Zoom, tile_at};

        let location = LonLat::new(144.96, -37.81).unwrap();
        let id = tile_at(location, Zoom::new(16).unwrap());
        let request = ehttp::Request::get(OpenStreetMapStandard.tile_url(id).unwrap())
            .with_header("User-Agent", USER_AGENT)
            .with_timeout(Some(Duration::from_secs(15)));
        let result = fetch(request, Body::Tile).unwrap();
        let Data::Image(image, _) = result else {
            panic!("expected a tile image")
        };
        assert_eq!(image.size, [256, 256]);
    }
}
