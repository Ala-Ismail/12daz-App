//! Approximate offline ranking from user-supplied OpenStreetMap reference vectors.

use std::{fs::File, io::Read, path::Path};

use serde::Deserialize;
use serde_json::Value;

use super::{Coordinate, Projection, detection::Candidate};

const EXTRACT_LIMIT: u64 = 8 * 1024 * 1024;
const SEGMENT_LIMIT: usize = 50_000;
const METRES_PER_DEGREE: f64 = 111_000.0;
const MATCH_METRES: f64 = 15.0;
const PROBE_SPACING_METRES: f64 = 25.0;

pub type Segment = [[f64; 2]; 2];

#[derive(Deserialize)]
struct GeoJson {
    #[serde(rename = "type")]
    kind: String,
    // An explicit surveyed coverage area, not the bounding box of its features.
    coverage: [f64; 4],
    source_date: String,
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    properties: Option<Value>,
    geometry: Option<Geometry>,
}

#[derive(Deserialize)]
struct Geometry {
    #[serde(rename = "type")]
    kind: String,
    coordinates: Value,
}

pub struct Reference {
    coverage: [f64; 4],
    pub date: String,
    roads: Vec<Segment>,
    boundaries: Vec<Segment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grid {
    Mga(u8),
    Brisbane,
}

#[derive(Debug, PartialEq)]
pub struct Recommendation {
    pub grid: Grid,
    pub road_matches: usize,
    pub road_probes: usize,
    pub boundary_matches: usize,
}

pub fn load(path: &Path) -> Result<Reference, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut contents = Vec::new();
    file.take(EXTRACT_LIMIT + 1)
        .read_to_end(&mut contents)
        .map_err(|error| error.to_string())?;
    parse(&contents)
}

fn parse(contents: &[u8]) -> Result<Reference, String> {
    if contents.len() as u64 > EXTRACT_LIMIT {
        return Err("The reference file exceeds 8 MiB.".into());
    }
    let data: GeoJson = serde_json::from_slice(contents)
        .map_err(|error| format!("Invalid GeoJSON reference extract. {error}"))?;
    let [west, south, east, north] = data.coverage;
    if data.kind != "FeatureCollection"
        || !(-180.0..=180.0).contains(&west)
        || !(-180.0..=180.0).contains(&east)
        || !(-90.0..=90.0).contains(&south)
        || !(-90.0..=90.0).contains(&north)
        || west >= east
        || south >= north
        || data.source_date.trim().is_empty()
        || data.source_date.len() > 32
    {
        return Err(
            "Provide a FeatureCollection with valid WGS 84 coverage and a source date.".into(),
        );
    }
    let mut reference = Reference {
        coverage: data.coverage,
        date: data.source_date,
        roads: Vec::new(),
        boundaries: Vec::new(),
    };
    for feature in data.features {
        let (Some(tags), Some(geometry)) = (feature.properties, feature.geometry) else {
            continue;
        };
        let is_road = tags
            .get("highway")
            .and_then(Value::as_str)
            .is_some_and(|tag| !matches!(tag, "" | "construction" | "proposed"));
        let is_boundary = tags.get("boundary").and_then(Value::as_str) == Some("protected_area")
            || matches!(
                tags.get("leisure").and_then(Value::as_str),
                Some("park" | "nature_reserve")
            )
            || tags.get("landuse").and_then(Value::as_str) == Some("highway");
        let target = match (is_road, is_boundary) {
            (true, _) => &mut reference.roads,
            (false, true) => &mut reference.boundaries,
            _ => continue,
        };
        let invalid = |_| "A reference feature has invalid coordinates.".to_owned();
        let lines: Vec<Vec<Vec<f64>>> = match geometry.kind.as_str() {
            "LineString" => vec![serde_json::from_value(geometry.coordinates).map_err(invalid)?],
            "MultiLineString" => serde_json::from_value(geometry.coordinates).map_err(invalid)?,
            "Polygon" if !is_road => serde_json::from_value::<Vec<_>>(geometry.coordinates)
                .map_err(invalid)?
                .into_iter()
                .take(1)
                .collect(),
            "MultiPolygon" if !is_road => {
                serde_json::from_value::<Vec<Vec<_>>>(geometry.coordinates)
                    .map_err(invalid)?
                    .into_iter()
                    .filter_map(|polygon| polygon.into_iter().next())
                    .collect()
            }
            _ => continue,
        };
        for line in lines {
            let mut last = None;
            for values in line {
                let [lon, lat, ..] = values.as_slice() else {
                    return Err("A reference point needs longitude and latitude.".into());
                };
                let point = [*lon, *lat];
                if !(-180.0..=180.0).contains(lon) || !(-90.0..=90.0).contains(lat) {
                    return Err("A reference point is outside WGS 84 bounds.".into());
                }
                if let Some(previous) = last
                    && previous != point
                {
                    target.push([previous, point]);
                    if target.len() > SEGMENT_LIMIT {
                        return Err("The reference extract contains too many segments.".into());
                    }
                }
                last = Some(point);
            }
        }
    }
    Ok(reference)
}

pub fn recommend(
    candidates: &[Candidate],
    roads: &[Segment],
    boundaries: &[Segment],
    reference: &Reference,
) -> Result<Recommendation, &'static str> {
    if candidates.is_empty() || roads.is_empty() || reference.roads.is_empty() {
        return Err("Comparable road geometry is unavailable.");
    }
    let mut scores: Vec<(f64, bool, Recommendation)> = Vec::new();
    for &candidate in candidates {
        let projection =
            Projection::new(candidate.crs).map_err(|_| "A projection could not be checked.")?;
        let project = |point: [f64; 2]| {
            Coordinate::new(point[0], point[1])
                .and_then(|p| candidate.units.project(p))
                .and_then(|p| projection.to_wgs84(p))
                .map(|p| [p.longitude(), p.latitude()])
                .map_err(|_| "A survey position could not be checked.")
        };
        let origin = project(roads[0][0])?;
        let local = |[lon, lat]: [f64; 2]| {
            [
                (lon - origin[0]) * METRES_PER_DEGREE * origin[1].to_radians().cos(),
                (lat - origin[1]) * METRES_PER_DEGREE,
            ]
        };
        let projected =
            |segments: &[Segment], check_coverage: bool| -> Result<Vec<Segment>, &'static str> {
                segments
                .iter()
                .map(|&[a, b]| {
                    let [a, b] = [project(a)?, project(b)?];
                    if check_coverage
                        && [a, b].into_iter().any(|p| !contains(reference.coverage, p))
                    {
                        return Err(
                            "The reference extract does not cover all candidate road locations.",
                        );
                    }
                    Ok([local(a), local(b)])
                })
                .collect()
            };
        let survey_roads = projected(roads, true)?;
        let survey_boundaries = projected(boundaries, false)?;
        let reference_roads = nearby_segments(&survey_roads, &reference.roads, local);
        let reference_boundaries =
            nearby_segments(&survey_boundaries, &reference.boundaries, local);
        let (road_matches, road_probes, shape) = measure(&survey_roads, &reference_roads);
        let (boundary_matches, boundary_probes, _) =
            measure(&survey_boundaries, &reference_boundaries);
        let road_fraction = road_matches as f64 / road_probes.max(1) as f64;
        let boundary_bonus = if road_matches >= 3 && shape && road_fraction >= 0.65 {
            0.05 * boundary_matches as f64 / boundary_probes.max(1) as f64
        } else {
            0.0
        };
        let grid = candidate
            .crs
            .mga_details()
            .map_or(Grid::Brisbane, |(_, zone)| Grid::Mga(zone));
        let score = (
            road_fraction + boundary_bonus,
            shape && road_matches >= 3 && road_fraction >= 0.65,
            Recommendation {
                grid,
                road_matches,
                road_probes,
                boundary_matches,
            },
        );
        if let Some(existing) = scores.iter_mut().find(|entry| entry.2.grid == grid) {
            if score.0 > existing.0 {
                *existing = score;
            }
        } else {
            scores.push(score);
        }
    }
    scores.sort_by(|a, b| b.0.total_cmp(&a.0));
    let next_score = scores.get(1).map(|s| s.0);
    let Some((best, true, recommendation)) = scores.into_iter().next() else {
        return Err("Too few distinctive matching road features.");
    };
    if next_score.is_some_and(|other| best - other < 0.15) {
        return Err("Several grids have similar road shapes.");
    }
    Ok(recommendation)
}

fn contains([west, south, east, north]: [f64; 4], [lon, lat]: [f64; 2]) -> bool {
    (west..=east).contains(&lon) && (south..=north).contains(&lat)
}

fn offset(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [b[0] - a[0], b[1] - a[1]]
}

fn nearby_segments(
    survey: &[Segment],
    reference: &[Segment],
    local: impl Fn([f64; 2]) -> [f64; 2],
) -> Vec<Segment> {
    if survey.is_empty() {
        return Vec::new();
    }
    let [left, bottom, right, top] = survey.iter().flatten().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |[left, bottom, right, top], &[x, y]| {
            [left.min(x), bottom.min(y), right.max(x), top.max(y)]
        },
    );
    reference
        .iter()
        .filter_map(|&[a, b]| {
            let [a, b] = [local(a), local(b)];
            (a[0].min(b[0]) <= right + MATCH_METRES
                && a[0].max(b[0]) >= left - MATCH_METRES
                && a[1].min(b[1]) <= top + MATCH_METRES
                && a[1].max(b[1]) >= bottom - MATCH_METRES)
                .then_some([a, b])
        })
        .collect()
}

fn measure(survey: &[Segment], reference: &[Segment]) -> (usize, usize, bool) {
    let mut probes: Vec<[f64; 2]> = Vec::new();
    let mut matches = 0;
    let mut heading = None::<[f64; 2]>;
    let mut shape = false;
    for &[a, b] in survey {
        let [dx, dy] = offset(a, b);
        let length = dx.hypot(dy);
        if length < 2.0 {
            continue;
        }
        let direction = [dx / length, dy / length];
        for fraction in [0.25, 0.5, 0.75] {
            let point = [a[0] + dx * fraction, a[1] + dy * fraction];
            if probes.iter().any(|&other| {
                let [x, y] = offset(other, point);
                x.hypot(y) < PROBE_SPACING_METRES
            }) {
                continue;
            }
            probes.push(point);
            if reference.iter().any(|&[start, end]| {
                let [x, y] = offset(start, end);
                let length = x.hypot(y);
                if length < 2.0 || (x * direction[0] + y * direction[1]).abs() < 0.85 * length {
                    return false;
                }
                let [px, py] = offset(start, point);
                let t = ((px * x + py * y) / (length * length)).clamp(0.0, 1.0);
                (px - t * x).hypot(py - t * y) <= MATCH_METRES
            }) {
                matches += 1;
                let first = heading.get_or_insert(direction);
                shape |= (first[0] * direction[1] - first[1] * direction[0]).abs() >= 0.25;
            }
        }
    }
    (matches, probes.len(), shape)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::map::{CrsId, SourceUnit};

    const A: Candidate = Candidate {
        crs: CrsId::Gda2020MgaZone55,
        units: SourceUnit::Metres,
    };
    const A94: Candidate = Candidate {
        crs: CrsId::Gda94MgaZone55,
        units: SourceUnit::Metres,
    };
    const B: Candidate = Candidate {
        crs: CrsId::Gda2020MgaZone56,
        units: SourceUnit::Metres,
    };
    const ROAD: [Segment; 3] = [
        [[500_000.0, 7_000_000.0], [500_160.0, 7_000_000.0]],
        [[500_160.0, 7_000_000.0], [500_160.0, 7_000_160.0]],
        [[500_160.0, 7_000_160.0], [500_300.0, 7_000_210.0]],
    ];
    const BOUNDARY: [Segment; 1] = [[[500_000.0, 7_000_050.0], [500_125.0, 7_000_050.0]]];

    fn geo(candidate: Candidate, segments: &[Segment]) -> Vec<Segment> {
        let projection = Projection::new(candidate.crs).unwrap();
        segments
            .iter()
            .map(|segment| {
                segment.map(|[x, y]| {
                    let point = projection.to_wgs84(Coordinate::new(x, y).unwrap()).unwrap();
                    [point.longitude(), point.latitude()]
                })
            })
            .collect()
    }

    fn fixture() -> Reference {
        Reference {
            coverage: [146.0, -28.0, 154.0, -26.0],
            date: "2026-11-21".into(),
            roads: geo(A, &ROAD),
            boundaries: Vec::new(),
        }
    }

    #[test]
    fn mga_detect_20_and_23_road_shapes_choose_zone_with_secondary_boundary_evidence() {
        let mut reference = fixture();
        reference.roads.extend(geo(
            B,
            &[[[500_000.0, 7_000_080.0], [500_300.0, 7_000_080.0]]],
        ));
        let plain = recommend(&[A, A94, B], &ROAD, &BOUNDARY, &reference).unwrap();
        assert_eq!(plain.grid, Grid::Mga(55));
        assert_eq!(plain.boundary_matches, 0);
        reference.boundaries = geo(B, &BOUNDARY);
        assert_eq!(
            recommend(&[A, A94, B], &ROAD, &BOUNDARY, &reference).unwrap(),
            plain
        );
        reference.boundaries.extend(geo(A, &BOUNDARY));
        let supported = recommend(&[A, A94, B], &ROAD, &BOUNDARY, &reference).unwrap();
        assert_eq!(supported.grid, Grid::Mga(55));
        assert!(supported.boundary_matches >= 3);
        reference.roads.clear();
        assert!(recommend(&[A, B], &ROAD, &BOUNDARY, &reference).is_err());
    }

    #[test]
    fn mga_detect_21_and_22_ties_partial_coverage_and_sparse_shapes_are_inconclusive() {
        let mut reference = fixture();
        assert!(recommend(&[A, B], &[], &[], &reference).is_err());
        assert!(recommend(&[A, B], &ROAD[..1], &[], &reference).is_err());
        reference.roads.extend(geo(B, &ROAD));
        assert_eq!(
            recommend(&[A, B], &ROAD, &[], &reference),
            Err("Several grids have similar road shapes.")
        );
        reference.coverage[2] = 150.0;
        assert!(
            recommend(&[A, B], &ROAD, &[], &reference)
                .unwrap_err()
                .contains("coverage")
                || recommend(&[A, B], &ROAD, &[], &reference)
                    .unwrap_err()
                    .contains("cover")
        );
        assert!(recommend(&[A], &ROAD, &[], &reference).is_ok());
    }

    #[test]
    fn mga_detect_22_reads_local_geojson_without_network_and_requires_coverage() {
        let reference = parse(br#"{
            "type":"FeatureCollection", "coverage":[146,-28,154,-26], "source_date":"2026-11-21",
            "features":[
                {"properties":{"highway":"residential"},"geometry":{"type":"LineString","coordinates":[[147,-27],[147.001,-27]]}},
                {"properties":{"leisure":"park"},"geometry":{"type":"Polygon","coordinates":[[[147,-27],[147.001,-27],[147.001,-27.001],[147,-27]]]}}
            ]}"#).unwrap();
        assert_eq!(reference.roads.len(), 1);
        assert_eq!(reference.boundaries.len(), 3);
        assert!(parse(br#"{"type":"FeatureCollection","features":[]}"#).is_err());
    }
}
