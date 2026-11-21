pub mod benchmark;
mod centreline;
pub mod corridor;
mod crs;
pub mod evidence;
mod roads;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

pub type Point = [f64; 2];

pub fn load_json<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<T, Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(File::open(path)?);
    const UTF8_BOM: &[u8] = &[0xef, 0xbb, 0xbf];
    if reader.fill_buf()?.starts_with(UTF8_BOM) {
        reader.consume(UTF8_BOM.len());
    }
    Ok(serde_json::from_reader(reader)?)
}

#[derive(Deserialize)]
pub struct Input {
    pub input_crs: String,
    pub output_crs: String,
    pub features: Vec<Value>,
    pub reference: Option<Reference>,
    /// Optional clipping window in output CRS metres (west, south, east, north).
    pub clip: Option<[f64; 4]>,
}

#[derive(Deserialize, Serialize)]
pub struct Reference {
    pub source: String,
    pub date: String,
    pub attribution: String,
    /// West, south, east and north in the associated document's coordinate system.
    pub coverage: [f64; 4],
}

#[derive(Deserialize)]
struct Feature {
    id: String,
    #[serde(default)]
    tags: BTreeMap<String, String>,
    geometry: Geometry,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "coordinates")]
enum Geometry {
    LineString(Vec<Point>),
    Polygon(Vec<Vec<Point>>),
}

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub feature_id: Option<String>,
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Serialize)]
pub struct RoadLine {
    pub source_ids: Vec<String>,
    pub points: Vec<Point>,
    pub provenance: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_source: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_status: Option<&'static str>,
}

#[derive(Serialize)]
pub struct Output {
    pub output_crs: String,
    pub reference: Option<Reference>,
    pub edges: Vec<RoadLine>,
    pub centrelines: Vec<RoadLine>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn detect(input: Input) -> Result<Output, String> {
    let projection = crs::Projection::new(&input.input_crs, &input.output_crs)?;
    if let Some(bounds) = input.clip {
        validate_bounds(bounds, "clip")?;
    }
    let reference = input
        .reference
        .map(|mut reference| -> Result<_, String> {
            let [west, south, east, north] = reference.coverage;
            validate_bounds(reference.coverage, "reference coverage")?;
            let mut bounds = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for corner in [[west, south], [west, north], [east, south], [east, north]] {
                let [x, y] = projection.point(corner)?;
                bounds = [
                    bounds[0].min(x),
                    bounds[1].min(y),
                    bounds[2].max(x),
                    bounds[3].max(y),
                ];
            }
            reference.coverage = bounds;
            Ok(reference)
        })
        .transpose()?;
    let mut output = Output {
        output_crs: input.output_crs,
        reference,
        edges: Vec::new(),
        centrelines: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut ids = BTreeSet::new();
    let mut features = Vec::new();
    for value in input.features {
        let id = value.get("id").and_then(Value::as_str).map(str::to_owned);
        let result = (|| {
            let mut feature: Feature = serde_json::from_value(value).map_err(|e| e.to_string())?;
            if feature.id.trim().is_empty() || ids.contains(&feature.id) {
                return Err("feature ID must be nonempty and unique".into());
            }
            project_geometry(&mut feature.geometry, &projection)?;
            ids.insert(feature.id.clone());
            features.push(feature);
            Ok(())
        })();
        if let Err(message) = result {
            output.diagnostics.push(Diagnostic {
                feature_id: id,
                code: "invalid_feature",
                message,
            });
        }
    }
    roads::build(features, input.clip, &mut output);
    Ok(output)
}

fn validate_bounds([west, south, east, north]: [f64; 4], label: &str) -> Result<(), String> {
    if [west, south, east, north]
        .iter()
        .any(|value| !value.is_finite())
        || west >= east
        || south >= north
    {
        return Err(format!("{label} must have finite ordered bounds"));
    }
    Ok(())
}

fn project_geometry(geometry: &mut Geometry, projection: &crs::Projection) -> Result<(), String> {
    let (paths, ring) = match geometry {
        Geometry::LineString(points) => (std::slice::from_mut(points), false),
        Geometry::Polygon(rings) => (rings.as_mut_slice(), true),
    };
    if paths.is_empty() {
        return Err("polygon has no exterior ring".into());
    }
    for points in paths {
        for point in points.iter_mut() {
            *point = projection.point(*point)?;
        }
        points.dedup();
        if ring && (points.len() < 4 || points.first() != points.last()) {
            return Err("polygon ring must be closed with at least three vertices".into());
        }
        if !ring && points.len() < 2 {
            return Err("line must contain two distinct points".into());
        }
    }
    Ok(())
}
