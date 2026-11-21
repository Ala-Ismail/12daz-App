//! Approximate alignment from loaded OpenStreetMap Carto pixels.

use az12_converter::map::{
    LonLat, Zoom,
    detection::Candidate,
    road_match::{Grid, Segment},
};
use eframe::egui;

pub(super) const TILE_PIXELS: f64 = 256.0;
const MIN_ZOOM: u8 = 15;
const MIN_PROBES: usize = 6;
const MAX_PROBES: usize = 72;

pub(super) struct RoadMask {
    size: [usize; 2],
    fill: Vec<bool>,
}

impl RoadMask {
    pub fn new(image: &egui::ColorImage) -> Self {
        Self {
            size: image.size,
            fill: image
                .pixels
                .iter()
                .map(|pixel| {
                    let [r, g, b, a] = pixel.to_array();
                    // Pale local roads and warm/pink highway fills in OpenStreetMap Carto.
                    a == 255
                        && ((r >= 238 && g >= 238 && b >= 238 && r.abs_diff(b) <= 9)
                            || (r >= 232 && g >= 155 && b >= 110 && r.saturating_sub(b) >= 25)
                            || (r >= 222
                                && g >= 128
                                && b >= 140
                                && r.saturating_sub(g) >= 40
                                && b > g))
                })
                .collect(),
        }
    }

    pub fn pixel(&self, x: f64, y: f64) -> Option<bool> {
        if !(0.0..TILE_PIXELS).contains(&x) || !(0.0..TILE_PIXELS).contains(&y) {
            return None;
        }
        let [width, height] = self.size;
        let ix = (x / TILE_PIXELS * width as f64).floor() as usize;
        let iy = (y / TILE_PIXELS * height as f64).floor() as usize;
        if ix >= width || iy >= height {
            return None;
        }
        self.fill.get(iy * width + ix).copied()
    }
}

pub(super) fn world_pixel(point: LonLat, zoom: Zoom) -> [f64; 2] {
    let pixels = TILE_PIXELS * f64::from(1_u32 << zoom.value());
    let mercator = point.latitude().to_radians().tan().asinh();
    [
        (point.longitude() + 180.0) / 360.0 * pixels,
        (1.0 - mercator / std::f64::consts::PI) / 2.0 * pixels,
    ]
}

pub(super) fn inside_view(corners: [[f64; 2]; 4], point: [f64; 2]) -> bool {
    // Accept either orientation. In the supported region the street-level viewport is convex.
    let mut sign = None;
    for (a, b) in corners.into_iter().zip(corners.into_iter().cycle().skip(1)) {
        let cross = (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
        if cross.abs() <= 1e-9 {
            continue;
        }
        if *sign.get_or_insert(cross.is_sign_positive()) != cross.is_sign_positive() {
            return false;
        }
    }
    sign.is_some()
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Score {
    pub aligned: usize,
    pub probes: Vec<usize>,
}

pub(super) struct Reading {
    pub candidate: Candidate,
    pub model: String,
    pub zoom: Zoom,
    pub drawing_view: [[f64; 2]; 4],
    pub score: Score,
}

pub(super) fn likely_grid(
    readings: &[Reading],
    current: &Reading,
    allowed: impl Fn(Candidate) -> bool,
) -> Option<Grid> {
    let grid = |candidate: Candidate| {
        candidate
            .crs
            .mga_details()
            .map_or(Grid::Brisbane, |(_, zone)| Grid::Mga(zone))
    };
    let comparable = |reading: &&Reading| {
        allowed(reading.candidate)
            && reading.model == current.model
            && reading.zoom == current.zoom
            && reading.drawing_view == current.drawing_view
            && reading.candidate.units == current.candidate.units
            && reading.score.probes == current.score.probes
    };
    let best = readings
        .iter()
        .filter(comparable)
        .max_by_key(|reading| reading.score.aligned)?;
    let best_grid = grid(best.candidate);
    let other = readings
        .iter()
        .filter(comparable)
        .filter(|reading| grid(reading.candidate) != best_grid)
        .max_by_key(|reading| reading.score.aligned)?;
    best.score.leads(&other.score).then_some(best_grid)
}

impl Score {
    pub fn evaluable(&self) -> usize {
        self.probes.len()
    }

    pub fn leads(&self, other: &Self) -> bool {
        self.probes == other.probes
            && self.aligned * 10 >= self.evaluable() * 7
            && self.aligned.saturating_sub(other.aligned) * 5 >= self.evaluable()
    }
}

pub(super) fn check(
    zoom: Zoom,
    segments: impl IntoIterator<Item = (usize, Segment)>,
    pixel: impl Fn([f64; 2]) -> Option<bool>,
) -> Result<Score, &'static str> {
    if zoom.value() < MIN_ZOOM {
        return Err("Zoom to street detail (tile zoom 15 or closer).");
    }
    let mut used: Vec<[f64; 2]> = Vec::new();
    let mut first_heading = None;
    let mut bends = false;
    let mut result = Score {
        aligned: 0,
        probes: Vec::new(),
    };
    for (index, [a, b]) in segments {
        let [dx, dy] = [b[0] - a[0], b[1] - a[1]];
        let length = dx.hypot(dy);
        if length < 2.0 || !length.is_finite() {
            continue;
        }
        let direction = [dx / length, dy / length];
        for (i, t) in [0.25, 0.5, 0.75].into_iter().enumerate() {
            let point = [a[0] + dx * t, a[1] + dy * t];
            if used
                .iter()
                .any(|&[x, y]| (point[0] - x).hypot(point[1] - y) < 12.0)
            {
                continue;
            }
            if let Some(aligned) = probe(point, direction, &pixel) {
                used.push(point);
                let [x, y] = *first_heading.get_or_insert(direction);
                bends |= (x * direction[1] - y * direction[0]).abs() >= 0.25;
                result.probes.push(index * 3 + i);
                result.aligned += usize::from(aligned);
                if result.evaluable() == MAX_PROBES {
                    break;
                }
            }
        }
        if result.evaluable() == MAX_PROBES {
            break;
        }
    }
    if result.evaluable() < MIN_PROBES || !bends {
        return Err("Too few loaded road probes or distinct road headings.");
    }
    Ok(result)
}

fn probe(
    point: [f64; 2],
    [dx, dy]: [f64; 2],
    pixel: &impl Fn([f64; 2]) -> Option<bool>,
) -> Option<bool> {
    let mut usable = false;
    for across in [0.0, -3.0, 3.0] {
        let mut aligned = 0;
        let mut loaded = 0;
        for along in [-6.0, 0.0, 6.0] {
            let at = |side| {
                pixel([
                    point[0] + dx * along - dy * side,
                    point[1] + dy * along + dx * side,
                ])
            };
            let (Some(core), Some(_), Some(_)) = (at(across), at(across - 16.0), at(across + 16.0))
            else {
                continue;
            };
            loaded += 1;
            if core
                && [3.0, 6.0, 10.0, 16.0].into_iter().any(|width| {
                    at(across - width) == Some(false) && at(across + width) == Some(false)
                })
            {
                aligned += 1;
            }
        }
        if loaded == 3 {
            usable = true;
            if aligned >= 2 {
                return Some(true);
            }
        }
    }
    usable.then_some(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use az12_converter::map::{CrsId, SourceUnit, TileId, tile_coordinate};

    const ROADS: [Segment; 2] = [
        [[40.0, 64.0], [164.0, 64.0]],
        [[164.0, 64.0], [164.0, 184.0]],
    ];
    const VIEW: [[f64; 2]; 4] = [[0.0, 0.0], [256.0, 0.0], [256.0, 256.0], [0.0, 256.0]];

    fn street_tile() -> RoadMask {
        let mut image =
            egui::ColorImage::filled([256, 256], egui::Color32::from_rgb(242, 239, 233));
        for y in 0..256 {
            for x in 0..256 {
                if ((28..=169).contains(&x) && (60..=68).contains(&y))
                    || ((160..=168).contains(&x) && (60..=194).contains(&y))
                {
                    image.pixels[y * 256 + x] = egui::Color32::WHITE;
                }
            }
        }
        RoadMask::new(&image)
    }

    #[test]
    fn mga_detect_31_bent_roads_match_and_displaced_roads_do_not() {
        let tile = street_tile();
        let zoom = Zoom::new(17).unwrap();
        let pixel = |[x, y]: [f64; 2]| tile.pixel(x, y);
        let aligned = check(zoom, ROADS.into_iter().enumerate(), pixel).unwrap();
        let displaced = ROADS.map(|segment| segment.map(|[x, y]| [x + 28.0, y + 28.0]));
        let wrong = check(zoom, displaced.into_iter().enumerate(), pixel).unwrap();
        assert_eq!(aligned.aligned, 6);
        assert_eq!(wrong.aligned, 0);
        assert!(aligned.leads(&wrong));
        assert_eq!(aligned.probes, wrong.probes);
    }

    #[test]
    fn mga_detect_32_33_uniform_labels_low_zoom_and_missing_pixels_are_not_matches() {
        let zoom = Zoom::new(17).unwrap();
        let coloured_white = |_: [f64; 2]| Some(true);
        assert_eq!(
            check(zoom, ROADS.into_iter().enumerate(), coloured_white)
                .unwrap()
                .aligned,
            0
        );
        let dots =
            |[x, y]: [f64; 2]| Some(x.floor() as i32 % 31 == 0 && y.floor() as i32 % 30 == 0);
        assert_eq!(
            check(zoom, ROADS.into_iter().enumerate(), dots)
                .unwrap()
                .aligned,
            0
        );
        assert!(check(zoom, [(0, ROADS[0])], coloured_white).is_err());
        assert!(
            check(
                Zoom::new(14).unwrap(),
                ROADS.into_iter().enumerate(),
                coloured_white
            )
            .is_err()
        );
        let tile = street_tile();
        let missing = |[x, y]: [f64; 2]| if x >= 150.0 { None } else { tile.pixel(x, y) };
        assert!(check(zoom, ROADS.into_iter().enumerate(), missing).is_err());
        let cropped = [[0.0, 0.0], [150.0, 0.0], [150.0, 256.0], [0.0, 256.0]];
        assert!(
            check(zoom, ROADS.into_iter().enumerate(), |p| {
                inside_view(cropped, p)
                    .then(|| tile.pixel(p[0], p[1]))
                    .flatten()
            })
            .is_err()
        );
        let tile = TileId::new(zoom, 118_000, 77_000).unwrap();
        let corner = tile_coordinate(tile, 0.25, 0.75).unwrap();
        let [x, y] = world_pixel(corner, zoom);
        assert!((x / TILE_PIXELS - 118_000.25).abs() < 1e-7);
        assert!((y / TILE_PIXELS - 77_000.75).abs() < 1e-7);
    }

    #[test]
    fn mga_detect_34_ranks_only_comparable_views_and_does_not_rank_datums() {
        let reading = |crs, zoom, aligned, probes| Reading {
            candidate: Candidate {
                crs,
                units: SourceUnit::Metres,
            },
            model: "Roads".into(),
            zoom: Zoom::new(zoom).unwrap(),
            drawing_view: VIEW,
            score: Score { aligned, probes },
        };
        let scope = |others| {
            let mut all = vec![reading(CrsId::Gda2020MgaZone54, 17, 6, (0..6).collect())];
            all.extend(others);
            likely_grid(&all, &all[0], |_| true)
        };
        assert_eq!(
            scope(vec![reading(
                CrsId::Gda2020MgaZone55,
                17,
                2,
                (0..6).collect()
            )]),
            Some(Grid::Mga(54))
        );
        for other in [
            reading(CrsId::Gda94MgaZone54, 17, 0, (0..6).collect()),
            reading(CrsId::Gda2020MgaZone55, 17, 6, (0..6).collect()),
            reading(CrsId::Gda2020MgaZone55, 18, 0, (0..6).collect()),
            reading(CrsId::Gda2020MgaZone55, 17, 0, (1..7).collect()),
        ] {
            assert_eq!(scope(vec![other]), None);
        }
    }
}
