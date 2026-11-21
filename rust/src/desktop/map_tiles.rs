// Requests and attribution are adapted from CADCraft. See rust/licenses/CADCraft-MIT.txt.
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use az12_converter::map::{
    EsriWorldImagery, GoogleMapsSatellite, LonLat, MapMode, OpenStreetMapStandard, TileBounds,
    TileId, TileProvider, Zoom, tile_at, tile_bounds,
};
use eframe::egui;
use serde::Deserialize;

use super::road_image::{RoadMask, TILE_PIXELS};
use super::tile_transport::{Body, Data, Queue};

const TARGET_TILES: u64 = 32;
const MAX_PENDING: usize = 8;
const RETRY_DELAY: Duration = Duration::from_secs(20);
const GOOGLE_HYBRID_SESSION: &[u8] = br#"{"mapType":"satellite","layerTypes":["layerRoadmap"],"overlay":false,"language":"en","region":"AU"}"#;
const ESRI_ATTRIBUTION: &str = "Powered by Esri · Imagery: Esri, Vantor, Earthstar Geographics, and the GIS User Community · Labels: Esri, TomTom, Garmin, FAO, NOAA, USGS, © OpenStreetMap contributors, and the GIS User Community";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct TileRange {
    zoom: Zoom,
    west: u32,
    north: u32,
    east: u32,
    south: u32,
}

impl TileRange {
    pub(super) fn zoom(self) -> Zoom {
        self.zoom
    }
    pub(super) fn covering(corners: &[LonLat; 4], max_zoom: u8) -> Option<Self> {
        for level in (0..=max_zoom).rev() {
            let zoom = Zoom::new(level).ok()?;
            let tiles = corners.map(|point| tile_at(point, zoom));
            let west = tiles.iter().map(|tile| tile.x()).min()?;
            let east = tiles.iter().map(|tile| tile.x()).max()?;
            let north = tiles.iter().map(|tile| tile.y()).min()?;
            let south = tiles.iter().map(|tile| tile.y()).max()?;
            if u64::from(east - west + 1) * u64::from(south - north + 1) <= TARGET_TILES {
                return Some(Self {
                    zoom,
                    west,
                    north,
                    east,
                    south,
                });
            }
        }
        None
    }

    pub(super) fn tiles(self) -> impl Iterator<Item = TileId> {
        (self.north..=self.south).flat_map(move |y| {
            (self.west..=self.east).filter_map(move |x| TileId::new(self.zoom, x, y).ok())
        })
    }

    fn contains(self, tile: TileId) -> bool {
        tile.zoom() == self.zoom
            && (self.west..=self.east).contains(&tile.x())
            && (self.north..=self.south).contains(&tile.y())
    }

    fn bounds(self) -> Option<TileBounds> {
        let nw = tile_bounds(TileId::new(self.zoom, self.west, self.north).ok()?);
        let se = tile_bounds(TileId::new(self.zoom, self.east, self.south).ok()?);
        Some(TileBounds {
            west: nw.west,
            north: nw.north,
            east: se.east,
            south: se.south,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Layer {
    Streets,
    Google,
    EsriImagery,
    EsriLabels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Request {
    GoogleSession,
    GoogleViewport(TileRange),
    Tile(TileId, Layer),
}

impl Request {
    fn is_visible(self, range: TileRange) -> bool {
        match self {
            Self::Tile(tile, _) => range.contains(tile),
            Self::GoogleViewport(viewport) => viewport == range,
            Self::GoogleSession => true,
        }
    }
}

struct Texture {
    handle: egui::TextureHandle,
    expires: Option<Instant>,
    road_mask: Option<RoadMask>,
}

#[derive(Deserialize)]
struct Session {
    session: String,
    expiry: String,
}

impl Session {
    fn live(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.expiry
            .parse::<u64>()
            .is_ok_and(|expiry| expiry > now.saturating_add(10))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Viewport {
    copyright: String,
    #[serde(default)]
    max_zoom_rects: Vec<ZoomRect>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ZoomRect {
    max_zoom: u8,
    west: f64,
    south: f64,
    east: f64,
    north: f64,
}

impl Viewport {
    fn coverage_zoom(&self, bounds: TileBounds) -> Option<u8> {
        let mut longitudes = vec![bounds.west, bounds.east];
        let mut coverage = Vec::new();
        for rect in &self.max_zoom_rects {
            // A provider rectangle may cross the antimeridian.
            let parts = if rect.west <= rect.east {
                [(rect.west, rect.east), (0.0, 0.0)]
            } else {
                [(rect.west, 180.0), (-180.0, rect.east)]
            };
            for (west, east) in parts {
                let clipped = ZoomRect {
                    max_zoom: rect.max_zoom,
                    west: west.max(bounds.west),
                    east: east.min(bounds.east),
                    south: rect.south.max(bounds.south),
                    north: rect.north.min(bounds.north),
                };
                if clipped.west < clipped.east && clipped.south < clipped.north {
                    longitudes.extend([clipped.west, clipped.east]);
                    coverage.push(clipped);
                }
            }
        }
        longitudes.sort_by(f64::total_cmp);
        longitudes.dedup();
        coverage.sort_by(|a, b| a.south.total_cmp(&b.south));

        // Each strip has the same set of covering rectangles across its width.
        // Find its finest complete coverage, then use the coarsest of those strip limits.
        let mut limit = 22;
        for strip in longitudes.windows(2) {
            let (left, right) = (strip[0], strip[1]);
            limit = (0..=limit).rev().find(|&zoom| {
                let mut north = bounds.south;
                for rect in &coverage {
                    if rect.south > north {
                        break;
                    }
                    if rect.max_zoom >= zoom && rect.west <= left && rect.east >= right {
                        north = north.max(rect.north);
                        if north >= bounds.north {
                            return true;
                        }
                    }
                }
                false
            })?;
        }
        Some(limit)
    }
}

#[derive(Default)]
pub(super) struct Tiles {
    mode: MapMode,
    analyse_streets: bool,
    google_key: String,
    esri_key: String,
    generation: u64,
    transport: Option<Queue<Request>>,
    pending: HashMap<Request, Arc<AtomicBool>>,
    failed: HashMap<Request, (Instant, String)>,
    textures: HashMap<(TileId, Layer), Texture>,
    visible: Option<TileRange>,
    requested: Option<TileRange>,
    google_session: Option<Session>,
    google_viewport: Option<(TileRange, Viewport)>,
    google_limit: Option<(TileRange, u8)>,
}

impl Tiles {
    pub(super) fn analyse_streets(&mut self, enabled: bool) {
        if self.analyse_streets != enabled {
            self.analyse_streets = enabled;
            if self.mode == MapMode::Street {
                self.invalidate();
            }
        }
    }

    pub(super) fn road_pixel(
        &self,
        zoom: Zoom,
        pixel: [f64; 2],
        painted: &[TileId],
    ) -> Option<bool> {
        let [x, y] = pixel;
        if !self.analyse_streets || !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
            return None;
        }
        let tile = TileId::new(zoom, (x / TILE_PIXELS) as u32, (y / TILE_PIXELS) as u32).ok()?;
        if !painted.contains(&tile) {
            return None;
        }
        self.textures
            .get(&(tile, Layer::Streets))?
            .road_mask
            .as_ref()?
            .pixel(x % TILE_PIXELS, y % TILE_PIXELS)
    }

    #[cfg(debug_assertions)]
    pub(super) fn capture_complete(&self) -> bool {
        self.visible
            .is_some_and(|range| range.tiles().all(|tile| self.textures(tile).0.is_some()))
    }

    pub(super) fn mode(&self) -> MapMode {
        self.mode
    }

    pub(super) fn select(&mut self, mode: MapMode) {
        if mode != self.mode {
            self.mode = mode;
            self.invalidate();
        }
    }

    pub(super) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(queue) = &self.transport {
            queue.set_generation(self.generation);
        }
        for cancel in self.pending.values() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.pending.clear();
        self.failed.clear();
        self.textures.clear();
        self.google_session = None;
        self.google_viewport = None;
        self.google_limit = None;
        self.visible = None;
        self.requested = None;
    }

    pub(super) fn configure_key(&mut self, mode: MapMode, key: String) -> Result<(), String> {
        let key = key.trim().to_owned();
        match mode {
            MapMode::Satellite => {
                if !key.is_empty() {
                    GoogleMapsSatellite::new(&key, None)
                        .map_err(|_| "Enter a valid Google Maps API key.")?;
                }
                self.google_key = key;
            }
            MapMode::EsriSatellite => {
                if !key.is_empty() {
                    EsriWorldImagery::new(&key).map_err(|_| "Enter a valid Esri API key.")?;
                }
                self.esri_key = key;
            }
            _ => return Err("This provider needs no API key.".into()),
        }
        self.invalidate();
        Ok(())
    }

    pub(super) fn has_key(&self, mode: MapMode) -> bool {
        match mode {
            MapMode::Satellite => !self.google_key.is_empty(),
            MapMode::EsriSatellite => !self.esri_key.is_empty(),
            _ => false,
        }
    }

    pub(super) fn credits(&self) -> Option<&str> {
        match self.mode {
            MapMode::Off => None,
            MapMode::Street => Some("© OpenStreetMap contributors"),
            MapMode::Satellite => self
                .google_viewport
                .as_ref()
                .filter(|(range, _)| Some(*range) == self.visible)
                .map(|(_, view)| view.copyright.as_str()),
            MapMode::EsriSatellite => Some(ESRI_ATTRIBUTION),
        }
    }

    pub(super) fn maximum_zoom(&self) -> u8 {
        if self.mode == MapMode::Satellite {
            22
        } else {
            19
        }
    }

    pub(super) fn update(
        &mut self,
        ctx: &egui::Context,
        mut range: TileRange,
        corners: &[LonLat; 4],
    ) -> TileRange {
        if self.mode == MapMode::Off {
            return range;
        }
        self.requested = Some(range);
        if let Some((previous_range, available)) = self.google_limit
            && previous_range == range
            && available < range.zoom.value()
            && let Some(lower) = TileRange::covering(corners, available)
        {
            range = lower;
        }
        self.visible = Some(range);
        self.pending.retain(|key, cancel| {
            let keep = key.is_visible(range);
            if !keep {
                cancel.store(true, Ordering::Relaxed);
            }
            keep
        });
        let now = Instant::now();
        self.textures.retain(|(tile, _), image| {
            range.contains(*tile) && image.expires.is_none_or(|expiry| expiry > now)
        });
        self.failed
            .retain(|key, (until, _)| *until > now && key.is_visible(range));
        self.finish(ctx);
        if self.mode == MapMode::Satellite {
            self.update_google(ctx, range);
        } else {
            for tile in range.tiles() {
                self.update_tile(ctx, tile);
            }
        }
        range
    }

    fn update_google(&mut self, ctx: &egui::Context, range: TileRange) {
        let Ok(provider) = GoogleMapsSatellite::new(&self.google_key, None) else {
            return;
        };
        if !self.google_session.as_ref().is_some_and(Session::live) {
            let url = provider.session_url();
            if self.google_session.is_some() {
                self.invalidate();
                self.visible = Some(range);
            }
            self.start(
                ctx,
                Request::GoogleSession,
                ehttp::Request::post(url, GOOGLE_HYBRID_SESSION.to_vec())
                    .with_header("Content-Type", "application/json"),
                Body::Metadata,
            );
            return;
        }
        if self.credits().is_none() {
            let Some(session) = self.google_session.as_ref() else {
                return;
            };
            if let Some(bounds) = range.bounds()
                && let Ok(url) = provider.viewport_url(range.zoom, bounds, &session.session)
            {
                self.start(
                    ctx,
                    Request::GoogleViewport(range),
                    ehttp::Request::get(url),
                    Body::Metadata,
                );
            }
            return;
        }
        if self.google_limit.is_some_and(|(requested, limit)| {
            Some(requested) == self.requested && limit < range.zoom.value()
        }) {
            ctx.request_repaint();
            return;
        }
        for tile in range.tiles() {
            self.update_tile(ctx, tile);
        }
    }

    fn update_tile(&mut self, ctx: &egui::Context, tile: TileId) {
        match self.mode {
            MapMode::Off => {}
            MapMode::Street => {
                if let Ok(url) = OpenStreetMapStandard.tile_url(tile) {
                    self.start(
                        ctx,
                        Request::Tile(tile, Layer::Streets),
                        ehttp::Request::get(url),
                        Body::OsmTile(tile),
                    );
                }
            }
            MapMode::Satellite => {
                let Some(session) = self.google_session.as_ref() else {
                    return;
                };
                if let Ok(provider) =
                    GoogleMapsSatellite::new(&self.google_key, Some(&session.session))
                    && let Ok(url) = provider.tile_url(tile)
                {
                    self.start(
                        ctx,
                        Request::Tile(tile, Layer::Google),
                        ehttp::Request::get(url),
                        Body::Tile,
                    );
                }
            }
            MapMode::EsriSatellite => {
                let Ok(provider) = EsriWorldImagery::new(&self.esri_key) else {
                    return;
                };
                let imagery = provider.imagery_tile_url(tile);
                let labels = provider.labels_tile_url(tile);
                if let Ok(url) = imagery {
                    self.start(
                        ctx,
                        Request::Tile(tile, Layer::EsriImagery),
                        ehttp::Request::get(url),
                        Body::Tile,
                    );
                }
                if let Ok(url) = labels {
                    let request = ehttp::Request::get(url)
                        .with_header("Authorization", format!("Bearer {}", self.esri_key));
                    self.start(
                        ctx,
                        Request::Tile(tile, Layer::EsriLabels),
                        request,
                        Body::Tile,
                    );
                }
            }
        }
    }

    fn start(&mut self, ctx: &egui::Context, key: Request, request: ehttp::Request, body: Body) {
        if self.pending.len() >= MAX_PENDING
            || self.pending.contains_key(&key)
            || self.failed.contains_key(&key)
            || matches!(key, Request::Tile(tile, layer) if self.textures.contains_key(&(tile, layer)))
        {
            return;
        }
        let queue = self.transport.get_or_insert_with(Queue::new);
        queue.set_generation(self.generation);
        let cancelled = Arc::new(AtomicBool::new(false));
        if queue.send(
            self.generation,
            key,
            request,
            body,
            ctx,
            Arc::clone(&cancelled),
        ) {
            self.pending.insert(key, cancelled);
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn finish(&mut self, ctx: &egui::Context) {
        let results: Vec<_> = self.transport.iter().flat_map(Queue::results).collect();
        for (generation, key, token, result) in results {
            if self
                .pending
                .get(&key)
                .is_some_and(|current| Arc::ptr_eq(current, &token))
            {
                self.accept(ctx, generation, key, result);
            }
        }
    }

    fn accept(
        &mut self,
        ctx: &egui::Context,
        generation: u64,
        key: Request,
        result: Result<Data, String>,
    ) {
        if generation != self.generation || self.mode == MapMode::Off {
            return;
        }
        self.pending.remove(&key);
        if matches!(key, Request::Tile(tile, _) if !self.visible.is_some_and(|range| range.contains(tile)))
            || matches!(key, Request::GoogleViewport(range) if self.visible != Some(range))
        {
            return;
        }
        let error = match (key, result) {
            (_, Err(error)) => error,
            (Request::Tile(tile, layer), Ok(Data::Image(image, lifetime))) => {
                let road_mask = (self.analyse_streets && layer == Layer::Streets)
                    .then(|| RoadMask::new(&image));
                let texture = ctx.load_texture("map tile", image, egui::TextureOptions::LINEAR);
                let expires = lifetime.and_then(|span| Instant::now().checked_add(span));
                self.textures.insert(
                    (tile, layer),
                    Texture {
                        handle: texture,
                        expires,
                        road_mask,
                    },
                );
                return;
            }
            (Request::GoogleSession, Ok(Data::Json(bytes))) => {
                if let Ok(session) = serde_json::from_slice::<Session>(&bytes)
                    && session.live()
                    && GoogleMapsSatellite::new(&self.google_key, Some(&session.session)).is_ok()
                {
                    self.google_session = Some(session);
                    return;
                }
                "Google Maps returned an invalid tile session.".into()
            }
            (Request::GoogleViewport(range), Ok(Data::Json(bytes))) => {
                if let Ok(viewport) = serde_json::from_slice::<Viewport>(&bytes)
                    && !viewport.copyright.trim().is_empty()
                {
                    let extent = self.requested.unwrap_or(range);
                    let available = extent
                        .bounds()
                        .and_then(|bounds| viewport.coverage_zoom(bounds));
                    #[cfg(debug_assertions)]
                    if std::env::var_os("AZ12_MAP_CHECK_ZOOM").is_some() {
                        eprintln!(
                            "Google viewport zoom {} with complete coverage up to {available:?}",
                            range.zoom.value()
                        );
                    }
                    if let Some(limit) = available
                        && limit < range.zoom.value()
                    {
                        self.google_limit = Some((extent, limit));
                    }
                    self.google_viewport = Some((range, viewport));
                    return;
                }
                "Google Maps did not return viewport attribution.".into()
            }
            _ => "Map provider returned an unexpected response.".into(),
        };
        self.failed
            .insert(key, (Instant::now() + RETRY_DELAY, error));
        ctx.request_repaint_after(RETRY_DELAY);
    }

    pub(super) fn textures(
        &self,
        tile: TileId,
    ) -> (Option<&egui::TextureHandle>, Option<&egui::TextureHandle>) {
        let get = |layer| self.textures.get(&(tile, layer)).map(|entry| &entry.handle);
        match self.mode {
            MapMode::Off => (None, None),
            MapMode::Street => (get(Layer::Streets), None),
            MapMode::Satellite if self.credits().is_some() => (get(Layer::Google), None),
            MapMode::Satellite => (None, None),
            MapMode::EsriSatellite => match (get(Layer::EsriImagery), get(Layer::EsriLabels)) {
                (Some(imagery), Some(labels)) => (Some(imagery), Some(labels)),
                _ => (None, None),
            },
        }
    }

    pub(super) fn status(&self) -> Option<&str> {
        match self.mode {
            MapMode::Off => None,
            MapMode::Satellite if self.google_key.is_empty() => {
                Some("Enter a Google Maps API key.")
            }
            MapMode::EsriSatellite if self.esri_key.is_empty() => Some("Enter an Esri API key."),
            _ => self
                .failed
                .values()
                .map(|(_, error)| error.as_str())
                .next()
                .or_else(|| (!self.pending.is_empty()).then_some("Loading map tiles…")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn footprint() -> [LonLat; 4] {
        [
            LonLat::new(144.96, -37.81).unwrap(),
            LonLat::new(144.961, -37.81).unwrap(),
            LonLat::new(144.96, -37.811).unwrap(),
            LonLat::new(144.961, -37.811).unwrap(),
        ]
    }

    fn marker() -> Data {
        Data::Image(
            egui::ColorImage::filled([1, 1], egui::Color32::GREEN),
            Some(Duration::from_secs(60)),
        )
    }

    #[test]
    fn mga_detect_33_35_raster_sampling_is_read_only_and_ignores_unpainted_or_stale_tiles() {
        let ctx = egui::Context::default();
        let range = TileRange::covering(&footprint(), 17).unwrap();
        let tile = range.tiles().next().unwrap();
        let pixel = [
            f64::from(tile.x()) * TILE_PIXELS + 50.0,
            f64::from(tile.y()) * TILE_PIXELS + 50.0,
        ];
        let mut tiles = Tiles::default();
        tiles.select(MapMode::Street);
        tiles.analyse_streets(true);
        tiles.visible = Some(range);
        let generation = tiles.generation;
        let key = Request::Tile(tile, Layer::Streets);
        let data = || {
            Data::Image(
                egui::ColorImage::filled([256, 256], egui::Color32::WHITE),
                Some(Duration::from_secs(60)),
            )
        };
        assert_eq!(tiles.road_pixel(range.zoom(), pixel, &[tile]), None);
        tiles.accept(&ctx, generation, key, Ok(data()));
        assert_eq!(tiles.road_pixel(range.zoom(), pixel, &[]), None);
        for _ in 0..2 {
            assert_eq!(tiles.road_pixel(range.zoom(), pixel, &[tile]), Some(true));
        }
        assert!(tiles.transport.is_none() && tiles.pending.is_empty());
        assert_eq!(tiles.generation, generation);
        tiles.analyse_streets(false);
        tiles.visible = Some(range);
        tiles.accept(&ctx, generation, key, Ok(data()));
        assert!(tiles.textures.is_empty());
        assert_eq!(tiles.road_pixel(range.zoom(), pixel, &[tile]), None);
    }

    #[test]
    fn map_08_off_and_missing_credentials_never_start_network() {
        let ctx = egui::Context::default();
        let corners = footprint();
        let range = TileRange::covering(&corners, 19).unwrap();
        let mut tiles = Tiles::default();
        for mode in [MapMode::Off, MapMode::Satellite, MapMode::EsriSatellite] {
            tiles.select(mode);
            tiles.update(&ctx, range, &corners);
            assert!(tiles.transport.is_none());
            assert!(tiles.pending.is_empty());
        }
    }

    #[test]
    fn map_10_google_requires_current_viewport_credits_and_limits_zoom() {
        let ctx = egui::Context::default();
        let corners = footprint();
        let range = TileRange::covering(&corners, 22).unwrap();
        let base = range.tiles().next().unwrap();
        let mut tiles = Tiles::default();
        tiles.select(MapMode::Satellite);
        tiles
            .configure_key(MapMode::Satellite, "test-key".into())
            .unwrap();
        tiles.visible = Some(range);
        tiles.requested = Some(range);
        tiles.accept(
            &ctx,
            tiles.generation,
            Request::Tile(base, Layer::Google),
            Ok(marker()),
        );
        assert!(tiles.textures(base).0.is_none());
        tiles.accept(
            &ctx,
            tiles.generation,
            Request::GoogleViewport(range),
            Ok(Data::Json(
                r#"{"copyright":"Map data © test","maxZoomRects":[{"maxZoom":9,"west":-180,"east":180,"south":-90,"north":90},{"maxZoom":18,"west":-180,"east":180,"south":-90,"north":90}]}"#
                    .as_bytes()
                    .to_vec(),
            )),
        );
        assert_eq!(tiles.credits(), Some("Map data © test"));
        assert_eq!(tiles.google_limit, Some((range, 18)));
        assert!(tiles.textures(base).0.is_some());
        let lower = TileRange::covering(&corners, 18).unwrap();
        tiles.visible = Some(lower);
        assert!(tiles.credits().is_none());
        assert!(tiles.textures(base).0.is_none());
        tiles.accept(
            &ctx,
            tiles.generation,
            Request::GoogleViewport(lower),
            Ok(Data::Json(
                r#"{"copyright":"Map data © lower","maxZoomRects":[{"maxZoom":17,"west":-180,"east":180,"south":-90,"north":90}]}"#
                    .as_bytes()
                    .to_vec(),
            )),
        );
        assert_eq!(tiles.google_limit, Some((range, 17)));
        assert_eq!(tiles.credits(), Some("Map data © lower"));
    }

    #[test]
    fn map_10_google_prefers_local_detail_and_detects_coverage_gaps() {
        let bounds = TileBounds {
            west: 150.0,
            east: 152.0,
            south: -35.0,
            north: -33.0,
        };
        let viewport: Viewport = serde_json::from_str(
            r#"{
            "copyright":"Map data",
            "maxZoomRects":[
                {"maxZoom":9,"west":-180,"east":180,"south":-90,"north":90},
                {"maxZoom":20,"west":150,"east":151,"south":-35,"north":-33},
                {"maxZoom":20,"west":151,"east":152,"south":-35,"north":-34},
                {"maxZoom":19,"west":151,"east":152,"south":-34,"north":-33},
                {"maxZoom":22,"west":140,"east":145,"south":-35,"north":-33}
            ]
        }"#,
        )
        .unwrap();
        assert_eq!(viewport.coverage_zoom(bounds), Some(19));
        let mut missing_strip = viewport;
        missing_strip.max_zoom_rects[3].south = -33.5;
        assert_eq!(missing_strip.coverage_zoom(bounds), Some(9));
    }

    #[test]
    fn map_11_and_12_esri_composites_matching_layers_and_drops_old_responses() {
        let ctx = egui::Context::default();
        let range = TileRange::covering(&footprint(), 19).unwrap();
        let tile = range.tiles().next().unwrap();
        let mut tiles = Tiles::default();
        tiles.select(MapMode::EsriSatellite);
        tiles.visible = Some(range);
        let active = tiles.generation;
        tiles.accept(
            &ctx,
            active,
            Request::Tile(tile, Layer::EsriLabels),
            Ok(marker()),
        );
        assert!(tiles.textures(tile).0.is_none());
        tiles.accept(
            &ctx,
            active,
            Request::Tile(tile, Layer::EsriImagery),
            Ok(marker()),
        );
        assert!(tiles.textures(tile).0.is_some());
        assert!(tiles.textures(tile).1.is_some());
        assert!(tiles.credits().unwrap().contains("TomTom"));
        tiles.select(MapMode::Street);
        tiles.visible = Some(range);
        tiles.accept(
            &ctx,
            active,
            Request::Tile(tile, Layer::EsriImagery),
            Ok(marker()),
        );
        assert!(tiles.textures.is_empty());
        assert!(tiles.textures(tile).0.is_none());
    }

    #[test]
    fn map_09_visible_tiles_are_bounded_when_fitted_to_a_large_area() {
        let corners = [
            LonLat::new(110.0, -10.0).unwrap(),
            LonLat::new(155.0, -10.0).unwrap(),
            LonLat::new(110.0, -45.0).unwrap(),
            LonLat::new(155.0, -45.0).unwrap(),
        ];
        let range = TileRange::covering(&corners, 19).unwrap();
        assert!(range.zoom.value() < 19);
        assert!(range.tiles().count() <= TARGET_TILES as usize);
    }
}
