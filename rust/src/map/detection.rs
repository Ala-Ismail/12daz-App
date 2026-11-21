//! Bounded source-coordinate evidence for offline projection detection.

use std::collections::BTreeMap;

use super::{Coordinate, CrsId, LonLat, Projection, SourceUnit};

const MODEL_LIMIT: usize = 512;
const SAMPLES_PER_MODEL: usize = 8;
const AREA_MARGIN_DEGREES: f64 = 0.005;
const LOCALITY_RADIUS_METRES: f64 = 100_000.0;

#[derive(Debug, Default, PartialEq)]
pub struct ArchiveEvidence {
    pub models: BTreeMap<String, ModelEvidence>,
    pub truncated: bool,
    pub road_sides: super::road_corridor::RoadEvidence,
}

#[derive(Debug, PartialEq)]
pub struct ModelEvidence {
    pub samples: Vec<Coordinate>,
    /// The actual points at min X, max X, min Y and max Y.
    pub extremes: [Coordinate; 4],
}

impl ArchiveEvidence {
    pub(crate) fn observe(&mut self, model: &str, point: [f64; 2]) {
        let Ok(point) = Coordinate::new(point[0], point[1]) else {
            return;
        };
        if let Some(summary) = self.models.get_mut(model) {
            summary.observe(point);
        } else if self.models.len() < MODEL_LIMIT {
            self.models.insert(
                model.to_owned(),
                ModelEvidence {
                    samples: vec![point],
                    extremes: [point; 4],
                },
            );
        } else {
            self.truncated = true;
        }
    }
}

impl ModelEvidence {
    fn observe(&mut self, point: Coordinate) {
        if self.samples.len() < SAMPLES_PER_MODEL && !self.samples.contains(&point) {
            self.samples.push(point);
        }
        for (index, better) in [
            point.x() < self.extremes[0].x(),
            point.x() > self.extremes[1].x(),
            point.y() < self.extremes[2].y(),
            point.y() > self.extremes[3].y(),
        ]
        .into_iter()
        .enumerate()
        {
            if better {
                self.extremes[index] = point;
            }
        }
    }

    fn points(&self) -> impl Iterator<Item = Coordinate> + '_ {
        self.samples.iter().chain(&self.extremes).copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub crs: CrsId,
    pub units: SourceUnit,
}

impl Candidate {
    pub fn matches_filters(self, unit: Option<SourceUnit>, zone: Option<u8>) -> bool {
        unit.is_none_or(|unit| unit == self.units)
            && zone.is_none_or(|zone| {
                self.crs
                    .mga_details()
                    .is_some_and(|(_, actual)| actual == zone)
            })
    }
}

/// Trusted declarations require a documented archive field and verified exporter fixtures.
#[derive(Clone, Copy, Debug, Default)]
pub enum Declaration {
    #[default]
    Absent,
    Verified {
        crs: CrsId,
        units: Option<SourceUnit>,
    },
    Conflicting,
}

#[derive(Debug, PartialEq)]
pub enum Detection {
    Assigned(Candidate),
    Candidates(Vec<Candidate>),
    Inconclusive {
        reason: &'static str,
        models: Vec<String>,
    },
}

fn area_of_use(crs: CrsId) -> Option<[f64; 4]> {
    // EPSG dataset in PROJ 9.5.1. GDA94 zones 46 and 47 in the legacy manual
    // selector have no corresponding registry records for offline detection.
    if CrsId::BRISBANE.contains(&crs) {
        return Some([152.37, -29.36, 153.69, -24.64]);
    }
    let (datum, zone) = crs.mga_details()?;
    Some(match (datum, zone) {
        ("GDA2020", 46) => [93.41, -15.46, 96.01, -8.57],
        ("GDA2020", 47) => [96.0, -15.56, 100.34, -8.47],
        (_, 48) => [102.14, -34.68, 108.01, -8.87],
        (_, 49) => [108.0, -37.84, 114.01, -10.72],
        (_, 50) => [114.0, -38.53, 120.01, -12.06],
        (_, 51) => [120.0, -38.07, 126.01, -10.46],
        (_, 52) => [125.99, -37.38, 132.0, -9.1],
        (_, 53) => [132.0, -40.71, 138.01, -8.88],
        (_, 54) => [138.0, -48.19, 144.01, -9.08],
        (_, 55) => [144.0, -50.89, 150.01, -9.23],
        (_, 56) => [150.0, -58.96, 156.0, -13.87],
        _ => return None,
    })
}

fn inside(point: LonLat, [west, south, east, north]: [f64; 4]) -> bool {
    let lon = point.longitude();
    let lat = point.latitude();
    let margin = AREA_MARGIN_DEGREES;
    (west - margin..=east + margin).contains(&lon)
        && (south - margin..=north + margin).contains(&lat)
}

fn near(point: LonLat, known: LonLat) -> bool {
    let east = (point.longitude() - known.longitude()) * known.latitude().to_radians().cos();
    let north = point.latitude() - known.latitude();
    east.hypot(north) * 111_000.0 <= LOCALITY_RADIUS_METRES
}

pub fn evaluate(
    evidence: &ArchiveEvidence,
    declaration: Declaration,
    units: Option<SourceUnit>,
    known: Option<LonLat>,
) -> Detection {
    let unclear = |reason| Detection::Inconclusive {
        reason,
        models: Vec::new(),
    };
    let (declared_crs, declared_units) = match declaration {
        Declaration::Absent => (None, None),
        Declaration::Verified { crs, units } if area_of_use(crs).is_some() => (Some(crs), units),
        Declaration::Verified { .. } => return unclear("The declared projection is unsupported."),
        Declaration::Conflicting => return unclear("The archive has conflicting declarations."),
    };
    if declared_units.is_some() && units.is_some() && declared_units != units {
        return unclear("The declared source units differ from the selected units.");
    }
    let confirmed_units = declared_units.or(units);
    if evidence.truncated {
        return unclear("Too many models to check the full archive.");
    }
    let mut points = evidence.models.values().flat_map(ModelEvidence::points);
    let Some(first) = points.next() else {
        return unclear("No reliable source XY coordinates were found.");
    };
    if declared_crs.is_none() && points.all(|point| point == first) {
        return unclear("A single location needs independent projection evidence.");
    }

    let mut candidates = Vec::new();
    let mut best_conflict = Vec::new();
    let mut swapped_axes = false;
    let mut outside_locality = false;
    let units_to_try: &[SourceUnit] = confirmed_units
        .as_ref()
        .map_or(SourceUnit::ALL.as_slice(), std::slice::from_ref);
    for crs in CrsId::MGA.into_iter().chain(CrsId::BRISBANE) {
        if declared_crs.is_some_and(|declared| declared != crs) {
            continue;
        }
        let (Some(area), Ok(proj)) = (area_of_use(crs), Projection::new(crs)) else {
            continue;
        };
        for &unit in units_to_try {
            let position = |point| unit.project(point).and_then(|xy| proj.to_wgs84(xy)).ok();
            let mut nearby = known.is_none();
            let mut failed = Vec::new();
            for (model, extents) in &evidence.models {
                let fits = extents.points().all(|point| {
                    position(point).is_some_and(|geographic| {
                        if known.is_some_and(|known| near(geographic, known)) {
                            nearby = true;
                        }
                        inside(geographic, area)
                    })
                });
                if !fits {
                    failed.push(model);
                }
            }
            if failed.is_empty() {
                if nearby {
                    candidates.push(Candidate { crs, units: unit });
                } else {
                    outside_locality = true;
                }
            } else {
                if failed.len() < evidence.models.len()
                    && (best_conflict.is_empty() || failed.len() < best_conflict.len())
                {
                    best_conflict = failed;
                }
                if !swapped_axes {
                    swapped_axes = evidence
                        .models
                        .values()
                        .flat_map(ModelEvidence::points)
                        .all(|p| {
                            Coordinate::new(p.y(), p.x())
                                .ok()
                                .and_then(position)
                                .is_some_and(|g| inside(g, area))
                        });
                }
            }
        }
    }
    if candidates.is_empty() {
        let reason = if swapped_axes {
            "The coordinates may have easting and northing reversed."
        } else if outside_locality {
            "No compatible candidate is near the known project location."
        } else if declared_crs.is_some() {
            "The coordinates fall outside the declared projection's area of use."
        } else if !best_conflict.is_empty() {
            "Models do not share a compatible projection."
        } else {
            "No supported projection fits the source coordinates."
        };
        return Detection::Inconclusive {
            reason,
            models: best_conflict.into_iter().cloned().collect(),
        };
    }
    match candidates.as_slice() {
        [candidate] if declared_crs.is_some() && confirmed_units.is_some() => {
            Detection::Assigned(*candidate)
        }
        _ => Detection::Candidates(candidates),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn survey(points: &[[f64; 2]]) -> ArchiveEvidence {
        let mut evidence = ArchiveEvidence::default();
        for &point in points {
            evidence.observe("Survey", point);
        }
        evidence
    }

    #[test]
    fn mga_detect_01_through_04_and_26_typed_declarations_require_consistent_xy_and_units() {
        for (crs, xy) in [
            (CrsId::Gda2020MgaZone55, [350174.343, 5812977.867]),
            (CrsId::Gda94MgaZone56, [334368.0, 6250944.0]),
            (CrsId::Gda94Bcsg02, [52777.351, 158753.314]),
            (CrsId::Gda2020Bcsg2020, [152777.351, 258753.314]),
        ] {
            for (units, scale) in [(SourceUnit::Metres, 1.0), (SourceUnit::Millimetres, 1000.0)] {
                let evidence = survey(&[
                    [xy[0] * scale, xy[1] * scale],
                    [(xy[0] + 120.0) * scale, (xy[1] + 80.0) * scale],
                ]);
                let target = Candidate { crs, units };
                assert_eq!(
                    evaluate(
                        &evidence,
                        Declaration::Verified {
                            crs,
                            units: Some(units)
                        },
                        None,
                        None
                    ),
                    Detection::Assigned(target)
                );
                let without_units = Declaration::Verified { crs, units: None };
                assert!(
                    matches!(evaluate(&evidence, without_units, None, None), Detection::Candidates(items) if items.contains(&target))
                );
                assert_eq!(
                    evaluate(&evidence, without_units, Some(units), None),
                    Detection::Assigned(target)
                );
                assert!(
                    matches!(evaluate(&evidence, Declaration::Absent, Some(units), None), Detection::Candidates(items) if items.contains(&target))
                );
                if CrsId::BRISBANE.contains(&crs) {
                    let other = if crs == CrsId::Gda94Bcsg02 {
                        CrsId::Gda2020Bcsg2020
                    } else {
                        CrsId::Gda94Bcsg02
                    };
                    assert!(matches!(
                        evaluate(
                            &evidence,
                            Declaration::Verified {
                                crs: other,
                                units: Some(units)
                            },
                            None,
                            None
                        ),
                        Detection::Inconclusive { .. }
                    ));
                }
            }
        }
    }

    #[test]
    fn mga_detect_05_06_07_08_10_11_ambiguity_and_bad_evidence() {
        let mut evidence = survey(&[[500000.0, 7000000.0], [500120.0, 7000080.0]]);
        let metres = Some(SourceUnit::Metres);
        let initial = evaluate(&evidence, Declaration::Absent, metres, None);
        let known = LonLat::new(147.0, -27.0).unwrap();
        let narrowed = evaluate(&evidence, Declaration::Absent, metres, Some(known));
        assert!(matches!(initial, Detection::Candidates(ref items) if items.len() > 2));
        assert!(
            matches!(narrowed, Detection::Candidates(items) if items.len() == 2 && items.iter().all(|p| p.crs.mga_details().is_some_and(|(_, z)| z == 55)))
        );
        assert!(
            matches!(evaluate(&evidence, Declaration::Conflicting, metres, None), Detection::Inconclusive { reason, .. } if reason.contains("conflicting"))
        );
        assert!(
            matches!(evaluate(&evidence, Declaration::Verified { crs: CrsId::Wgs84, units: metres }, None, None), Detection::Inconclusive { reason, .. } if reason.contains("unsupported"))
        );
        evidence.observe("Local", [100.0, 200.0]);
        assert!(
            matches!(evaluate(&evidence, Declaration::Absent, metres, None), Detection::Inconclusive { models, .. } if models == ["Local"])
        );

        let reversed = survey(&[[7000000.0, 500000.0], [7000080.0, 500120.0]]);
        assert!(
            matches!(evaluate(&reversed, Declaration::Absent, metres, None), Detection::Inconclusive { reason, .. } if reason.contains("reversed"))
        );
        let one = survey(&[[500000.0, 7000000.0]]);
        assert!(matches!(
            evaluate(&one, Declaration::Absent, metres, None),
            Detection::Inconclusive { .. }
        ));
        assert!(matches!(
            evaluate(
                &one,
                Declaration::Verified {
                    crs: CrsId::Gda2020MgaZone55,
                    units: metres
                },
                None,
                None
            ),
            Detection::Assigned(_)
        ));
        assert!(matches!(
            evaluate(
                &ArchiveEvidence::default(),
                Declaration::Absent,
                metres,
                None
            ),
            Detection::Inconclusive { .. }
        ));
    }

    #[test]
    fn mga_detect_09_uses_documented_boundary_tolerance() {
        let declared = Declaration::Verified {
            crs: CrsId::Gda2020MgaZone55,
            units: Some(SourceUnit::Metres),
        };
        let nearby = survey(&[[766873.110, 5900922.114], [768030.241, 5900885.551]]);
        let outside = survey(&[[766873.110, 5900922.114], [768742.324, 5900862.971]]);
        assert!(matches!(
            evaluate(&nearby, declared, None, None),
            Detection::Assigned(_)
        ));
        assert!(matches!(
            evaluate(&outside, declared, None, None),
            Detection::Inconclusive { .. }
        ));
    }
}
