//! Experimental projection-fit measurements over reviewed design cross-sections.

use serde::{Deserialize, Serialize};

use crate::Point;

const STEP_M: f64 = 0.25;
const EDGE_TOLERANCE_M: f64 = 2.0;
const LATERAL_WINDOW_M: f64 = 18.0;

#[derive(Deserialize)]
pub struct Pilot {
    pub sections: Vec<Section>,
    pub cases: Vec<Case>,
}

#[derive(Deserialize)]
pub struct Section {
    pub name: String,
    /// Coordinate index along the review section, east (0) or north (1).
    pub axis: usize,
    /// Opposing kerb coordinates in order of increasing station.
    pub stations: Vec<[Point; 2]>,
}

#[derive(Debug, Clone, Copy)]
pub struct Fit {
    pub design_centre_m: f64,
    pub covered_centre_m: f64,
    pub design_edge_m: f64,
    pub covered_edge_m: f64,
    pub eligible_centre_m: f64,
    pub eligible_edge_m: f64,
}

pub fn measure_alignment(
    section: &Section,
    centrelines: &[Vec<Point>],
    edges: &[Vec<Point>],
) -> Result<Fit, String> {
    validate_section(section)?;
    let centre = measure(section, centrelines, 0.0, false, false, &mut Vec::new());
    let edge = measure(section, edges, 0.0, true, false, &mut Vec::new());
    Ok(Fit {
        design_centre_m: centre.design_m,
        covered_centre_m: centre.covered_design_m,
        design_edge_m: edge.design_m,
        covered_edge_m: edge.covered_design_m,
        eligible_centre_m: centre.eligible_detection_m,
        eligible_edge_m: edge.eligible_detection_m,
    })
}

#[derive(Deserialize)]
pub struct Case {
    name: String,
    centrelines: Vec<Vec<Point>>,
    edges: Vec<Vec<Point>>,
    /// Per-section transverse offset for the shifted control.
    #[serde(default)]
    shift_m: f64,
}

#[derive(Clone, Copy, Default, Serialize)]
struct Coverage {
    eligible_detection_m: f64,
    matched_detection_m: f64,
    design_m: f64,
    covered_design_m: f64,
}

impl Coverage {
    fn add(&mut self, other: Self) {
        self.eligible_detection_m += other.eligible_detection_m;
        self.matched_detection_m += other.matched_detection_m;
        self.design_m += other.design_m;
        self.covered_design_m += other.covered_design_m;
    }

    fn scores(self) -> Metrics {
        let following = ratio(self.matched_detection_m, self.eligible_detection_m);
        let coverage = ratio(self.covered_design_m, self.design_m);
        let fit = following.map(|following| {
            let coverage = coverage.unwrap_or(0.0);
            2.0 * following * coverage / (following + coverage).max(f64::EPSILON)
        });
        Metrics {
            lengths: self,
            detected_following: following,
            design_coverage: coverage,
            fit,
        }
    }
}

fn ratio(numerator: f64, denominator: f64) -> Option<f64> {
    (denominator > 0.0).then(|| (numerator / denominator).clamp(0.0, 1.0))
}

#[derive(Serialize)]
struct Metrics {
    #[serde(flatten)]
    lengths: Coverage,
    detected_following: Option<f64>,
    design_coverage: Option<f64>,
    fit: Option<f64>,
}

#[derive(Serialize)]
pub struct CaseReport {
    name: String,
    centrelines: Metrics,
    edges: Metrics,
    sections: Vec<SectionReport>,
}

#[derive(Serialize)]
struct SectionReport {
    name: String,
    centrelines: Metrics,
    edges: Metrics,
}

#[derive(Serialize)]
pub struct Report {
    step_m: f64,
    heading_tolerance_degrees: f64,
    lateral_window_m: f64,
    edge_tolerance_m: f64,
    cases: Vec<CaseReport>,
    #[serde(skip)]
    pub(crate) evidence: Vec<(&'static str, [Point; 2])>,
}

pub fn score(input: Pilot) -> Result<Report, String> {
    if input.sections.is_empty() || input.cases.is_empty() {
        return Err("corridor pilot needs design sections and detection cases".into());
    }
    for section in &input.sections {
        validate_section(section)?;
    }
    let mut evidence = Vec::new();
    let mut cases = Vec::new();
    for (case_index, case) in input.cases.into_iter().enumerate() {
        if !(-100.0..=100.0).contains(&case.shift_m)
            || case.centrelines.iter().chain(&case.edges).any(|line| {
                line.iter()
                    .flatten()
                    .any(|value| !(-40_000_000.0..=40_000_000.0).contains(value))
                    || line
                        .windows(2)
                        .any(|pair| length(pair[0], pair[1]) > 10_000.0)
            })
        {
            return Err(format!(
                "{}: invalid coordinates, segment length or control shift",
                case.name
            ));
        }
        let mut centres = Coverage::default();
        let mut edges = Coverage::default();
        let mut sections = Vec::new();
        for section in &input.sections {
            let collect = case_index == 0;
            let centre = measure(
                section,
                &case.centrelines,
                case.shift_m,
                false,
                collect,
                &mut evidence,
            );
            let edge = measure(
                section,
                &case.edges,
                case.shift_m,
                true,
                collect,
                &mut evidence,
            );
            centres.add(centre);
            edges.add(edge);
            sections.push(SectionReport {
                name: section.name.clone(),
                centrelines: centre.scores(),
                edges: edge.scores(),
            });
        }
        cases.push(CaseReport {
            name: case.name,
            centrelines: centres.scores(),
            edges: edges.scores(),
            sections,
        });
    }
    Ok(Report {
        step_m: STEP_M,
        heading_tolerance_degrees: 35.0,
        lateral_window_m: LATERAL_WINDOW_M,
        edge_tolerance_m: EDGE_TOLERANCE_M,
        cases,
        evidence,
    })
}

fn validate_section(section: &Section) -> Result<(), String> {
    if section.axis > 1 || section.stations.len() < 2 {
        return Err(format!(
            "{}: missing design cross-sections or invalid axis",
            section.name
        ));
    }
    let axis = section.axis;
    let valid = section.stations.iter().all(|[a, b]| {
        a.iter().chain(b).all(|coordinate| coordinate.is_finite())
            && a[axis] == b[axis]
            && (2.0..=50.0).contains(&(b[1 - axis] - a[1 - axis]))
    }) && section
        .stations
        .windows(2)
        .all(|pair| pair[1][0][axis] > pair[0][0][axis]);
    if !valid {
        return Err(format!(
            "{}: kerbs must form ordered stations 2–50 m wide",
            section.name
        ));
    }
    Ok(())
}

fn interpolate(a: Point, b: Point, t: f64) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn length(a: Point, b: Point) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn measure(
    section: &Section,
    detected: &[Vec<Point>],
    shift: f64,
    edge: bool,
    collect: bool,
    evidence: &mut Vec<(&'static str, [Point; 2])>,
) -> Coverage {
    let axis = section.axis;
    let cross = 1 - axis;
    let count = if edge { 2 } else { 1 };
    let mut totals = Coverage::default();
    let mut covered = vec![[false; 2]; section.stations.len() - 1];
    for line in detected {
        for pair in line.windows(2) {
            let mut a = pair[0];
            let mut b = pair[1];
            a[cross] += shift;
            b[cross] += shift;
            let distance = length(a, b);
            if !distance.is_finite() || distance <= f64::EPSILON {
                continue;
            }
            let samples = (distance / STEP_M).ceil() as usize;
            let weight = distance / samples as f64;
            for part in 0..samples {
                let fraction = (part as f64 + 0.5) / samples as f64;
                let point = interpolate(a, b, fraction);
                let idx = section
                    .stations
                    .partition_point(|station| station[0][axis] <= point[axis]);
                if idx == 0 || idx == section.stations.len() {
                    continue;
                }
                let [lo, hi] = section.stations[idx - 1];
                let [next_lo, next_hi] = section.stations[idx];
                let t = (point[axis] - lo[axis]) / (next_lo[axis] - lo[axis]);
                let left = interpolate(lo, next_lo, t)[cross];
                let right = interpolate(hi, next_hi, t)[cross];
                let lateral = point[cross];
                if lateral < left - LATERAL_WINDOW_M || lateral > right + LATERAL_WINDOW_M {
                    continue;
                }
                totals.eligible_detection_m += weight;
                let side = usize::from((lateral - left).abs() > (lateral - right).abs());
                let (start, end) = if edge {
                    if side == 0 {
                        (lo, next_lo)
                    } else {
                        (hi, next_hi)
                    }
                } else {
                    (interpolate(lo, hi, 0.5), interpolate(next_lo, next_hi, 0.5))
                };
                let dot = ((b[0] - a[0]) * (end[0] - start[0])
                    + (b[1] - a[1]) * (end[1] - start[1]))
                    .abs();
                let aligned = dot >= distance * length(start, end) * 35.0_f64.to_radians().cos();
                let close = if edge {
                    (lateral - left).abs().min((lateral - right).abs()) <= EDGE_TOLERANCE_M
                } else {
                    (left..=right).contains(&lateral)
                };
                if aligned && close {
                    totals.matched_detection_m += weight;
                    covered[idx - 1][if edge { side } else { 0 }] = true;
                }
                if collect {
                    let layer = match (edge, aligned && close) {
                        (false, true) => "FIT-MATCHED-CENTRELINE",
                        (false, false) => "FIT-MISSED-CENTRELINE",
                        (true, true) => "FIT-MATCHED-DETECTED-EDGE",
                        (true, false) => "FIT-MISSED-DETECTED-EDGE",
                    };
                    evidence.push((
                        layer,
                        [
                            interpolate(a, b, part as f64 / samples as f64),
                            interpolate(a, b, (part + 1) as f64 / samples as f64),
                        ],
                    ));
                }
            }
        }
    }
    for (i, pair) in section.stations.windows(2).enumerate() {
        for side in 0..count {
            let points = if edge {
                [pair[0][side], pair[1][side]]
            } else {
                [
                    interpolate(pair[0][0], pair[0][1], 0.5),
                    interpolate(pair[1][0], pair[1][1], 0.5),
                ]
            };
            let length = length(points[0], points[1]);
            totals.design_m += length;
            if covered[i][side] {
                totals.covered_design_m += length;
            }
            if collect {
                let layer = match (edge, covered[i][side]) {
                    (false, true) => "FIT-COVERED-DESIGN-AXIS",
                    (false, false) => "FIT-UNCOVERED-DESIGN-AXIS",
                    (true, true) => "FIT-COVERED-DESIGN-KERB",
                    (true, false) => "FIT-UNCOVERED-DESIGN-KERB",
                };
                evidence.push((layer, points));
            }
        }
    }
    totals
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section() -> Section {
        Section {
            name: "straight".into(),
            axis: 0,
            stations: (0..=200)
                .map(|step| {
                    let x = f64::from(step) / 2.0;
                    [[x, -4.0], [x, 4.0]]
                })
                .collect(),
        }
    }

    #[test]
    fn aligned_centreline_beats_shifted_and_crossing_controls() {
        let points = vec![[100.0, 0.0], [0.0, 0.0]];
        let centreline = |name: &str, shift_m, points| Case {
            name: name.into(),
            centrelines: vec![points],
            edges: Vec::new(),
            shift_m,
        };
        let report = score(Pilot {
            sections: vec![section()],
            cases: vec![
                centreline("aligned", 0.0, points.clone()),
                centreline("shifted", 12.0, points),
                centreline("crossing", 0.0, vec![[50.0, -18.0], [50.0, 18.0]]),
            ],
        })
        .unwrap();
        let fits: Vec<_> = report
            .cases
            .iter()
            .map(|case| case.centrelines.fit)
            .collect();
        assert_eq!(fits, vec![Some(1.0), Some(0.0), Some(0.0)]);
    }

    #[test]
    fn short_match_earns_partial_design_coverage_and_edges_score_separately() {
        let case = Case {
            name: "partial".into(),
            centrelines: vec![vec![[0.0, 0.0], [25.0, 0.0]]],
            edges: vec![vec![[0.0, -3.0], [100.0, -3.0]]],
            shift_m: 0.0,
        };
        let report = score(Pilot {
            sections: vec![section()],
            cases: vec![case],
        })
        .unwrap();
        let measured = &report.cases[0];
        assert_eq!(measured.centrelines.detected_following, Some(1.0));
        assert_eq!(measured.centrelines.design_coverage, Some(0.25));
        assert_eq!(measured.centrelines.fit, Some(0.4));
        assert_eq!(measured.edges.design_coverage, Some(0.5));
    }
}
