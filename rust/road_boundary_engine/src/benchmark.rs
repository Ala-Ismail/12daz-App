//! Length-weighted comparison against independent, explicitly identified road sides.

use geo::algorithm::buffer::{BufferStyle, LineCap};
use geo::{BooleanOps, Buffer, Euclidean, Length, LineString, MultiLineString, Rect};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use crate::{Point, crs, load_json, validate_bounds};

#[derive(Deserialize)]
pub struct Benchmark {
    crs: String,
    tolerance_m: f64,
    reference_source: String,
    reference_date: String,
    sections: Vec<Section>,
}

#[derive(Deserialize)]
struct Section {
    id: String,
    road_type: String,
    window: [f64; 4],
    prediction_file: PathBuf,
    #[serde(default)]
    prediction_sides: BTreeMap<usize, String>,
    reference: Vec<ReferenceSide>,
}

#[derive(Deserialize)]
struct ReferenceSide {
    side: String,
    points: Vec<Point>,
}

#[derive(Deserialize)]
struct Predictions {
    output_crs: String,
    edges: Vec<Prediction>,
}

#[derive(Deserialize)]
struct Prediction {
    points: Vec<Point>,
    provenance: String,
}

#[derive(Clone, Copy, Default, Serialize)]
struct Lengths {
    predicted_m: f64,
    reference_m: f64,
    matched_prediction_m: f64,
    matched_reference_m: f64,
}

impl Lengths {
    fn add(&mut self, other: Self) {
        self.predicted_m += other.predicted_m;
        self.reference_m += other.reference_m;
        self.matched_prediction_m += other.matched_prediction_m;
        self.matched_reference_m += other.matched_reference_m;
    }
}

#[derive(Serialize)]
struct Scores {
    #[serde(flatten)]
    lengths: Lengths,
    precision: f64,
    recall: f64,
    f1: f64,
}

impl From<Lengths> for Scores {
    fn from(lengths: Lengths) -> Self {
        let divided = |numerator: f64, denominator: f64| {
            if denominator > 0.0 {
                (numerator / denominator).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let precision = divided(lengths.matched_prediction_m, lengths.predicted_m);
        let recall = divided(lengths.matched_reference_m, lengths.reference_m);
        Self {
            lengths,
            precision,
            recall,
            f1: divided(2.0 * precision * recall, precision + recall),
        }
    }
}

#[derive(Serialize)]
pub struct Report {
    engine_version: &'static str,
    crs: String,
    tolerance_m: f64,
    reference_source: String,
    reference_date: String,
    overall: Scores,
    meets_numerical_threshold: bool,
    sections: Vec<SectionReport>,
    road_types: BTreeMap<String, Scores>,
    provenance: BTreeMap<String, ProvenanceTotals>,
    #[serde(skip)]
    pub(crate) evidence: Vec<(&'static str, MultiLineString)>,
}

#[derive(Default, Serialize)]
struct ProvenanceTotals {
    predicted_m: f64,
    matched_prediction_m: f64,
    matched_reference_m: f64,
}

#[derive(Serialize)]
struct SectionReport {
    id: String,
    road_type: String,
    window: [f64; 4],
    prediction_file: PathBuf,
    scores: Scores,
    unassigned_predictions: usize,
}

pub fn score(input: Benchmark, directory: &Path) -> Result<Report, Box<dyn std::error::Error>> {
    crs::Projection::new(&input.crs, &input.crs)?;
    let tolerance = input.tolerance_m;
    if !tolerance.is_finite()
        || tolerance <= 0.0
        || input.sections.is_empty()
        || input.reference_source.trim().is_empty()
        || input.reference_date.trim().is_empty()
    {
        return Err(
            "benchmark requires a positive metre tolerance, reference source/date and sections"
                .into(),
        );
    }
    let mut seen = BTreeSet::new();
    let mut overall = Lengths::default();
    let mut road_types = BTreeMap::<String, Lengths>::new();
    let mut provenance = BTreeMap::<String, ProvenanceTotals>::new();
    let mut evidence = Vec::new();
    let mut reports = Vec::new();
    for section in input.sections {
        if section.id.trim().is_empty()
            || !seen.insert(section.id.clone())
            || section.road_type.trim().is_empty()
        {
            return Err(
                "benchmark section IDs must be nonempty and unique, with a road type".into(),
            );
        }
        validate_bounds(section.window, "benchmark window")?;
        let [west, south, east, north] = section.window;
        let window = Rect::new((west, south), (east, north)).to_polygon();
        let predictions: Predictions = load_json(&directory.join(&section.prediction_file))?;
        if predictions.output_crs != input.crs
            || section
                .prediction_sides
                .keys()
                .any(|index| *index >= predictions.edges.len())
        {
            return Err(format!(
                "{}: prediction CRS or edge index does not match manifest",
                section.id
            )
            .into());
        }
        let mut truth = BTreeMap::<String, MultiLineString>::new();
        for side in section.reference {
            if side.side.trim().is_empty() || truth.contains_key(&side.side) {
                return Err(format!(
                    "{}: supply one reference line per named road side",
                    section.id
                )
                .into());
            }
            let clipped = window.clip(&checked_path(side.points)?, false);
            if length(&clipped) <= 0.0 {
                return Err(format!(
                    "{}: reference side {} has no length inside the window",
                    section.id, side.side
                )
                .into());
            }
            evidence.push(("REFERENCE-ROAD-EDGE", clipped.clone()));
            truth.insert(side.side, clipped);
        }
        if section
            .prediction_sides
            .values()
            .any(|side| !truth.contains_key(side))
        {
            return Err(
                format!("{}: prediction names an unknown reference side", section.id).into(),
            );
        }

        let mut totals = Lengths {
            reference_m: truth.values().map(length).sum(),
            ..Lengths::default()
        };
        let mut unassigned = 0;
        // Qualifying mapped outlines take precedence over estimated duplicates, independent of file order.
        let mut edges: Vec<_> = predictions.edges.into_iter().enumerate().collect();
        edges.sort_by_key(|(_, edge)| edge.provenance != "mapped_outline");
        for (index, edge) in edges {
            if edge.provenance != "mapped_outline" && edge.provenance != "estimated_edge" {
                return Err(format!(
                    "{}: invalid edge provenance {}",
                    section.id, edge.provenance
                )
                .into());
            }
            let line = window.clip(&checked_path(edge.points)?, false);
            let predicted_m = length(&line);
            if predicted_m <= 0.0 {
                continue;
            }
            let summary = if let Some(remaining) = section
                .prediction_sides
                .get(&index)
                .and_then(|side| truth.get_mut(side))
            {
                score_side(line, remaining, tolerance, &mut evidence)
            } else {
                unassigned += 1;
                evidence.push(("UNMATCHED-PREDICTED-ROAD-EDGE", line));
                Lengths {
                    predicted_m,
                    ..Lengths::default()
                }
            };
            totals.add(summary);
            let class = provenance.entry(edge.provenance).or_default();
            class.predicted_m += summary.predicted_m;
            class.matched_prediction_m += summary.matched_prediction_m;
            class.matched_reference_m += summary.matched_reference_m;
        }
        evidence.extend(
            truth
                .into_values()
                .map(|line| ("MISSED-REFERENCE-ROAD-EDGE", line)),
        );
        overall.add(totals);
        road_types
            .entry(section.road_type.clone())
            .or_default()
            .add(totals);
        reports.push(SectionReport {
            id: section.id,
            road_type: section.road_type,
            window: section.window,
            prediction_file: section.prediction_file,
            scores: totals.into(),
            unassigned_predictions: unassigned,
        });
    }
    if overall.reference_m <= 0.0 {
        return Err("benchmark needs independent reference geometry in its windows".into());
    }
    let overall = Scores::from(overall);
    Ok(Report {
        engine_version: env!("CARGO_PKG_VERSION"),
        crs: input.crs,
        tolerance_m: tolerance,
        reference_source: input.reference_source,
        reference_date: input.reference_date,
        meets_numerical_threshold: overall.f1 >= 0.8,
        overall,
        sections: reports,
        road_types: road_types
            .into_iter()
            .map(|(kind, stats)| (kind, stats.into()))
            .collect(),
        provenance,
        evidence,
    })
}

fn checked_path(mut points: Vec<Point>) -> Result<MultiLineString, String> {
    points.dedup();
    if points.len() < 2
        || points
            .iter()
            .flatten()
            .any(|coordinate| !coordinate.is_finite())
    {
        return Err("benchmark line needs two distinct finite points".into());
    }
    Ok(MultiLineString(vec![LineString::from(
        points.into_iter().map(|[x, y]| (x, y)).collect::<Vec<_>>(),
    )]))
}

fn length(lines: &MultiLineString) -> f64 {
    Euclidean.length(lines)
}

fn score_side(
    line: MultiLineString,
    remaining: &mut MultiLineString,
    tolerance: f64,
    evidence: &mut Vec<(&'static str, MultiLineString)>,
) -> Lengths {
    let buffer = |lines: &MultiLineString| {
        lines.buffer_with_style(BufferStyle::new(tolerance).line_cap(LineCap::Butt))
    };
    let predicted_m = length(&line);
    if remaining.0.is_empty() {
        evidence.push(("UNMATCHED-PREDICTED-ROAD-EDGE", line));
        return Lengths {
            predicted_m,
            ..Lengths::default()
        };
    }
    let prediction_buffer = buffer(&line);
    let matched_reference = prediction_buffer.clip(remaining, false);
    let matched_reference_m = length(&matched_reference);
    if matched_reference_m <= f64::EPSILON {
        evidence.push(("UNMATCHED-PREDICTED-ROAD-EDGE", line));
        return Lengths {
            predicted_m,
            ..Lengths::default()
        };
    }
    let reference_hit_buffer = buffer(&matched_reference);
    let matched_prediction = reference_hit_buffer.clip(&line, false);
    let matched_prediction_m = length(&matched_prediction);
    if matched_prediction_m <= f64::EPSILON {
        evidence.push(("UNMATCHED-PREDICTED-ROAD-EDGE", line));
        return Lengths {
            predicted_m,
            ..Lengths::default()
        };
    }
    *remaining = prediction_buffer.clip(remaining, true);
    evidence.push(("MATCHED-PREDICTED-ROAD-EDGE", matched_prediction));
    evidence.push((
        "UNMATCHED-PREDICTED-ROAD-EDGE",
        reference_hit_buffer.clip(&line, true),
    ));
    Lengths {
        predicted_m,
        matched_prediction_m,
        matched_reference_m,
        ..Lengths::default()
    }
}
