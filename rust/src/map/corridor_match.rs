//! Offline corridor comparison using explicit OpenStreetMap extract coverage.

use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use road_boundary_engine::corridor::{Section, measure_alignment};
use serde_json::{Value, json};

use super::{
    Coordinate, Projection, SourceUnit,
    detection::Candidate,
    road_corridor::{Corridor, RoadEvidence},
    road_match::Grid,
};

const EXTRACT_LIMIT: u64 = 8 * 1024 * 1024;
const FEATURE_LIMIT: usize = 10_000;

#[derive(Clone, Debug)]
pub struct Reference {
    coverage: [f64; 4],
    pub date: String,
    features: Vec<Value>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Coverage {
    pub design_centre_m: f64,
    pub covered_centre_m: f64,
    pub design_edge_m: f64,
    pub covered_edge_m: f64,
}

impl Coverage {
    pub fn centre_fraction(self) -> f64 {
        self.covered_centre_m / self.design_centre_m
    }
    pub fn edge_fraction(self) -> f64 {
        self.covered_edge_m / self.design_edge_m
    }
    pub fn pooled_fraction(self) -> f64 {
        (self.covered_centre_m + self.covered_edge_m) / (self.design_centre_m + self.design_edge_m)
    }
    pub fn matches(self) -> bool {
        self.pooled_fraction() > 0.70
    }

    fn add(&mut self, fit: road_boundary_engine::corridor::Fit) {
        self.design_centre_m += fit.design_centre_m;
        self.covered_centre_m += fit.covered_centre_m;
        self.design_edge_m += fit.design_edge_m;
        self.covered_edge_m += fit.covered_edge_m;
    }
}

#[derive(Debug)]
pub enum Status {
    Unchecked,
    Inconclusive(String),
    Measured {
        coverage: Coverage,
        by_model: BTreeMap<String, Coverage>,
        date: String,
    },
}

#[derive(Debug)]
pub struct Assessment {
    pub candidate: Candidate,
    pub status: Status,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepStep {
    pub candidate: Candidate,
    pub index: usize,
    pub total: usize,
}

pub struct RankedGrid<'a> {
    pub grid: Grid,
    /// Measured datum variants in descending score order.
    pub variants: Vec<(&'a Assessment, Coverage)>,
}

pub fn ranked_grids(results: &[Assessment]) -> Vec<RankedGrid<'_>> {
    let mut measured: Vec<_> = results
        .iter()
        .filter_map(|result| match result.status {
            Status::Measured { coverage, .. } if coverage.pooled_fraction() > 0.0 => {
                Some((result, coverage))
            }
            _ => None,
        })
        .collect();
    measured.sort_by(|a, b| b.1.pooled_fraction().total_cmp(&a.1.pooled_fraction()));
    let mut grids: Vec<RankedGrid<'_>> = Vec::new();
    for (result, coverage) in measured {
        let grid = result
            .candidate
            .crs
            .mga_details()
            .map_or(Grid::Brisbane, |(_, zone)| Grid::Mga(zone));
        if let Some(entry) = grids.iter_mut().find(|entry| entry.grid == grid) {
            entry.variants.push((result, coverage));
        } else {
            grids.push(RankedGrid {
                grid,
                variants: vec![(result, coverage)],
            });
        }
    }
    grids
}

pub fn companion_path(archive: &Path) -> PathBuf {
    let mut name = archive.as_os_str().to_owned();
    name.push(".osm-roads.geojson");
    name.into()
}

pub fn load(path: &Path) -> Result<Reference, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut data = Vec::new();
    file.take(EXTRACT_LIMIT + 1)
        .read_to_end(&mut data)
        .map_err(|error| error.to_string())?;
    if data.len() as u64 > EXTRACT_LIMIT {
        return Err("The reference file exceeds 8 MiB.".into());
    }
    let document: Value =
        serde_json::from_slice(data.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&data))
            .map_err(|error| format!("Invalid reference JSON. {error}"))?;
    parse_reference(document)
}

fn parse_reference(document: Value) -> Result<Reference, String> {
    let geojson = document["type"] == "FeatureCollection";
    if !geojson && document["input_crs"] != "EPSG:4326" {
        return Err(
            "Use a WGS 84 GeoJSON FeatureCollection or a WGS 84 road-engine input file.".into(),
        );
    }
    let (bounds, date) = if geojson {
        (&document["coverage"], &document["source_date"])
    } else {
        (
            &document["reference"]["coverage"],
            &document["reference"]["date"],
        )
    };
    let coverage: [f64; 4] = serde_json::from_value(bounds.clone())
        .map_err(|_| "Reference needs explicit WGS 84 coverage.")?;
    let [west, south, east, north] = coverage;
    let Some(date) = date
        .as_str()
        .filter(|date| !date.trim().is_empty() && date.len() <= 32)
    else {
        return Err("Reference needs a source date.".into());
    };
    if !(-180.0..=180.0).contains(&west)
        || !(-180.0..=180.0).contains(&east)
        || !(-90.0..=90.0).contains(&south)
        || !(-90.0..=90.0).contains(&north)
        || west >= east
        || south >= north
    {
        return Err("Reference coverage must be valid ordered WGS 84 bounds.".into());
    }
    let Some(input) = document["features"]
        .as_array()
        .filter(|features| features.len() <= FEATURE_LIMIT)
    else {
        return Err("Reference must supply up to 10,000 features.".into());
    };
    let mut features = Vec::new();
    for (i, feature) in input.iter().enumerate() {
        let property_key = if geojson { "properties" } else { "tags" };
        let Some(properties) = feature[property_key].as_object() else {
            continue;
        };
        let tags: BTreeMap<_, _> = properties
            .iter()
            .filter_map(|(key, value)| {
                if let Some(value) = value.as_str() {
                    Some((key.as_str(), value.to_owned()))
                } else if value.is_number() || value.is_boolean() {
                    Some((key.as_str(), value.to_string()))
                } else {
                    None
                }
            })
            .collect();
        let road = |key: &str| {
            tags.get(key).is_some_and(|value| {
                matches!(
                    value.as_str(),
                    "motorway"
                        | "trunk"
                        | "primary"
                        | "secondary"
                        | "tertiary"
                        | "unclassified"
                        | "residential"
                        | "living_street"
                        | "service"
                        | "road"
                        | "track"
                        | "motorway_link"
                        | "trunk_link"
                        | "primary_link"
                        | "secondary_link"
                        | "tertiary_link"
                )
            })
        };
        let geometry = &feature["geometry"];
        let (kind, parts) = match geometry["type"].as_str() {
            Some("LineString") if road("highway") => ("LineString", vec![&geometry["coordinates"]]),
            Some("MultiLineString") if road("highway") => (
                "LineString",
                geometry["coordinates"]
                    .as_array()
                    .ok_or("Invalid MultiLineString.")?
                    .iter()
                    .collect(),
            ),
            Some("Polygon") if road("area:highway") => ("Polygon", vec![&geometry["coordinates"]]),
            Some("MultiPolygon") if road("area:highway") => (
                "Polygon",
                geometry["coordinates"]
                    .as_array()
                    .ok_or("Invalid MultiPolygon.")?
                    .iter()
                    .collect(),
            ),
            _ => continue,
        };
        for (part, coordinates) in parts.into_iter().enumerate() {
            let coordinates = if kind == "Polygon" {
                let rings = coordinates.as_array().ok_or("Invalid road polygon.")?;
                json!(
                    rings
                        .iter()
                        .map(planar_points)
                        .collect::<Result<Vec<_>, _>>()?
                )
            } else {
                json!(planar_points(coordinates)?)
            };
            features.push(json!({"id": format!("feature/{i}/{part}"), "tags": tags, "geometry": {"type": kind, "coordinates": coordinates}}));
            if features.len() > FEATURE_LIMIT {
                return Err("Reference exceeds the road geometry limit.".into());
            }
        }
    }
    if features.is_empty() {
        return Err("The reference contains no usable road vectors.".into());
    }
    Ok(Reference {
        coverage,
        date: date.to_owned(),
        features,
    })
}

/// Parse a completed Overpass response and keep road geometry local to each requested box.
pub fn overpass_references(data: &[u8], boxes: &[[f64; 4]]) -> Result<Vec<Reference>, String> {
    let document: Value =
        serde_json::from_slice(data).map_err(|_| "Invalid OpenStreetMap road response.")?;
    if document.get("remark").is_some() {
        return Err("OpenStreetMap returned an incomplete road response.".into());
    }
    let date = document["osm3s"]["timestamp_osm_base"]
        .as_str()
        .ok_or("OpenStreetMap road response has no source date.")?;
    let elements = document["elements"]
        .as_array()
        .filter(|elements| elements.len() <= FEATURE_LIMIT)
        .ok_or("OpenStreetMap returned too many road features or invalid data.")?;
    let mut roads = Vec::new();
    for way in elements.iter().filter(|way| way["type"] == "way") {
        let points: Vec<_> = way["geometry"]
            .as_array()
            .ok_or("OpenStreetMap returned an incomplete road way.")?
            .iter()
            .map(|point| {
                point["lon"]
                    .as_f64()
                    .zip(point["lat"].as_f64())
                    .map(|(x, y)| [x, y])
                    .ok_or("OpenStreetMap returned an invalid road point.")
            })
            .collect::<Result<_, _>>()?;
        if points.len() < 2 {
            continue;
        }
        let kind = if way["tags"]["area:highway"].is_string()
            && points.len() >= 4
            && points.first() == points.last()
        {
            "Polygon"
        } else if way["tags"]["area"] != "yes" {
            "LineString"
        } else {
            continue;
        };
        let coordinates = if kind == "Polygon" {
            json!([points])
        } else {
            json!(points)
        };
        let extent = points.iter().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |[w, s, e, n], &[x, y]| [w.min(x), s.min(y), e.max(x), n.max(y)],
        );
        roads.push((
            extent,
            json!({"tags": way["tags"], "geometry": {"type":kind, "coordinates":coordinates}}),
        ));
    }
    let mut references = Vec::new();
    for &[w, s, e, n] in boxes {
        let features: Vec<_> = roads
            .iter()
            .filter(|(box_, _)| box_[0] <= e && box_[2] >= w && box_[1] <= n && box_[3] >= s)
            .map(|(_, feature)| feature.clone())
            .collect();
        let result = parse_reference(
            json!({"input_crs": "EPSG:4326", "reference": { "date": date, "coverage": [w,s,e,n]}, "features":features}),
        );
        match result {
            Ok(reference) => references.push(reference),
            Err(error) if error == "The reference contains no usable road vectors." => {}
            Err(error) => return Err(error),
        }
    }
    Ok(references)
}

fn planar_points(value: &Value) -> Result<Vec<[f64; 2]>, &'static str> {
    value
        .as_array()
        .ok_or("Invalid road coordinate sequence.")?
        .iter()
        .map(|point| {
            let lon = point[0]
                .as_f64()
                .filter(|lon| (-180.0..=180.0).contains(lon));
            let lat = point[1].as_f64().filter(|lat| (-90.0..=90.0).contains(lat));
            lon.zip(lat)
                .map(|(x, y)| [x, y])
                .ok_or("A road point needs valid WGS 84 longitude and latitude.")
        })
        .collect()
}

pub fn assess(
    candidates: &[Candidate],
    units: SourceUnit,
    evidence: &RoadEvidence,
    references: &[Reference],
) -> Result<Vec<Assessment>, String> {
    assess_with_progress(candidates, units, evidence, references, |_| true)
}

/// Stops at the next candidate when progress is no longer accepted.
pub fn assess_with_progress(
    candidates: &[Candidate],
    units: SourceUnit,
    evidence: &RoadEvidence,
    references: &[Reference],
    progress: impl FnMut(SweepStep) -> bool,
) -> Result<Vec<Assessment>, String> {
    Sweep::prepare(candidates, units, evidence)?.assess(references, progress)
}

pub struct Sweep {
    candidates: Vec<(Candidate, Option<[f64; 4]>)>,
    corridors: Vec<Corridor>,
    window: [f64; 4],
}

impl Sweep {
    pub fn prepare(
        candidates: &[Candidate],
        units: SourceUnit,
        evidence: &RoadEvidence,
    ) -> Result<Self, String> {
        let candidates: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|candidate| candidate.units == units)
            .collect();
        if candidates.is_empty() {
            return Err("Detect grids again with the current source units.".into());
        }
        let corridors = evidence.pair(units)?;
        if corridors.is_empty() {
            return Err("No reliable opposing road edges were found across the archive.".into());
        }
        let bounds = corridors.iter().flat_map(|c| &c.stations).flatten().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |[west, south, east, north], &[x, y]| {
                [west.min(x), south.min(y), east.max(x), north.max(y)]
            },
        );
        let [west, south, east, north] = bounds;
        let window = [west - 20.0, south - 20.0, east + 20.0, north + 20.0];
        let candidates = candidates
            .into_iter()
            .map(|candidate| (candidate, geographic_window(candidate, window)))
            .collect();
        Ok(Self {
            candidates,
            corridors,
            window,
        })
    }

    pub fn missing_boxes(&self, references: &[Reference]) -> Vec<[f64; 4]> {
        let mut boxes = Vec::new();
        for bounds in self.candidates.iter().filter_map(|(_, bounds)| *bounds) {
            if references
                .iter()
                .any(|reference| contains(reference.coverage, bounds))
            {
                continue;
            }
            let [w, s, e, n] = bounds;
            // Snap and pad outward to share requests between nearby datum variants.
            let low = |x: f64| ((x - 0.0005) * 200.0).floor() / 200.0;
            let high = |x: f64| ((x + 0.0005) * 200.0).ceil() / 200.0;
            let region = [low(w), low(s), high(e), high(n)];
            if region[2] - region[0] <= 0.06
                && region[3] - region[1] <= 0.06
                && !boxes.contains(&region)
            {
                boxes.push(region);
            }
        }
        boxes.sort_by(|a, b| {
            a[0].total_cmp(&b[0])
                .then(a[1].total_cmp(&b[1]))
                .then(a[2].total_cmp(&b[2]))
                .then(a[3].total_cmp(&b[3]))
        });
        boxes
    }

    pub fn assess(
        self,
        references: &[Reference],
        mut progress: impl FnMut(SweepStep) -> bool,
    ) -> Result<Vec<Assessment>, String> {
        let total = self.candidates.len();
        let mut results = Vec::new();
        for (index, (candidate, bounds)) in self.candidates.into_iter().enumerate() {
            if !progress(SweepStep {
                candidate,
                index,
                total,
            }) {
                return Err("Grid comparison cancelled.".into());
            }
            let reference = bounds.and_then(|bounds| {
                references
                    .iter()
                    .find(|reference| contains(reference.coverage, bounds))
            });
            let status = match reference
                .map(|reference| assess_one(candidate, self.window, &self.corridors, reference))
            {
                Some(Ok(status)) => status,
                Some(Err(error)) => Status::Inconclusive(error),
                None => Status::Unchecked,
            };
            results.push(Assessment { candidate, status });
        }
        Ok(results)
    }
}

fn contains([w, s, e, n]: [f64; 4], [west, south, east, north]: [f64; 4]) -> bool {
    w <= west && s <= south && e >= east && n >= north
}

fn geographic_window(
    candidate: Candidate,
    [west, south, east, north]: [f64; 4],
) -> Option<[f64; 4]> {
    let projection = Projection::new(candidate.crs).ok()?;
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for [x, y] in [[west, south], [east, south], [west, north], [east, north]] {
        let point = projection.to_wgs84(Coordinate::new(x, y).ok()?).ok()?;
        bounds = [
            bounds[0].min(point.longitude()),
            bounds[1].min(point.latitude()),
            bounds[2].max(point.longitude()),
            bounds[3].max(point.latitude()),
        ];
    }
    Some(bounds)
}

fn assess_one(
    candidate: Candidate,
    window: [f64; 4],
    corridors: &[Corridor],
    reference: &Reference,
) -> Result<Status, String> {
    let roads = road_boundary_engine::detect(road_boundary_engine::Input {
        input_crs: "EPSG:4326".into(),
        output_crs: candidate.crs.epsg().into(),
        features: reference.features.clone(),
        reference: None,
        clip: Some(window),
    })?;
    if roads
        .diagnostics
        .iter()
        .any(|item| item.code == "invalid_feature" || item.code == "invalid_polygon")
    {
        return Ok(Status::Inconclusive(
            "Some reference roads have invalid geometry.".into(),
        ));
    }
    let centrelines: Vec<_> = roads
        .centrelines
        .into_iter()
        .map(|line| line.points)
        .collect();
    let edges: Vec<_> = roads.edges.into_iter().map(|line| line.points).collect();
    if !centrelines.is_empty() && edges.is_empty() {
        return Ok(Status::Inconclusive(
            "Reference supplies centrelines only.".into(),
        ));
    }
    let mut coverage = Coverage::default();
    let mut by_model: BTreeMap<String, Coverage> = BTreeMap::new();
    let mut nearby_edges = 0.0;
    let mut nearby_centres = 0.0;
    for corridor in corridors {
        let section = Section {
            name: "Survey road corridor".into(),
            axis: corridor.axis,
            stations: corridor.stations.clone(),
        };
        let fit = measure_alignment(&section, &centrelines, &edges)?;
        coverage.add(fit);
        nearby_edges += fit.eligible_edge_m;
        nearby_centres += fit.eligible_centre_m;
        // Mixed-model sections are assigned once to the group of models that supplied them.
        let models = corridor
            .models
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" + ");
        by_model.entry(models).or_default().add(fit);
    }
    if coverage.design_centre_m < 20.0
        || coverage.design_edge_m < 40.0
        || (nearby_centres > 0.0 && nearby_edges < 5.0)
    {
        return Ok(Status::Inconclusive(
            "Too little comparable road-edge and centreline coverage.".into(),
        ));
    }
    Ok(Status::Measured {
        coverage,
        by_model,
        date: reference.date.clone(),
    })
}
