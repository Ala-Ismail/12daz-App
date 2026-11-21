//! Derive reviewed-style cross sections from road-side geometry across the archive.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
};

use crate::{
    error::Result,
    geometry::{self, GeometryError, Kind, Point},
    parser::{SourceEntity, fields, scalar},
};

use super::SourceUnit;

const STEP_M: f64 = 0.5;
const MIN_STATIONS: usize = 21;
const POINT_LIMIT: usize = 150_000;
const HIT_LIMIT: usize = 250_000;
const WIDTH_RANGE: std::ops::RangeInclusive<f64> = 3.0..=16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EdgeKind {
    Kerb,
    Bitumen(bool),
}

#[derive(Debug, PartialEq)]
struct RoadSide {
    model: String,
    kind: EdgeKind,
    points: Vec<Point>,
}

#[derive(Debug, Default, PartialEq)]
pub struct RoadEvidence {
    sides: Vec<RoadSide>,
    points: usize,
    truncated: bool,
}

#[derive(Debug)]
pub struct Corridor {
    pub axis: usize,
    pub stations: Vec<[Point; 2]>,
    pub models: BTreeSet<String>,
}

#[derive(Clone, Copy)]
struct Hit {
    cross: f64,
    gradient: f64,
    side: usize,
}

impl RoadEvidence {
    pub(crate) fn observe(
        &mut self,
        source: &SourceEntity,
        model: &str,
        cancel: Option<&AtomicBool>,
    ) -> Result<()> {
        if self.truncated {
            return Ok(());
        }
        let Some(kind) = classify(source, model) else {
            return Ok(());
        };
        match geometry::convert_geometry(
            source,
            geometry::GeometryOptions::default(),
            &mut |_, _| {},
            cancel,
        ) {
            Ok(lines) => {
                for line in lines
                    .into_iter()
                    .filter(|line| line.kind == Kind::Polyline && !line.closed)
                {
                    let points = match geometry::preview_points(&line) {
                        Ok(points) => points,
                        Err(GeometryError::Unsupported(_)) => continue,
                        Err(GeometryError::Input(error)) => return Err(error),
                    };
                    if points.len() < 2 {
                        continue;
                    }
                    self.points += points.len();
                    if self.points > POINT_LIMIT {
                        self.truncated = true;
                        return Ok(());
                    }
                    self.sides.push(RoadSide {
                        model: model.into(),
                        kind,
                        points,
                    });
                }
            }
            Err(GeometryError::Unsupported(_)) => {}
            Err(GeometryError::Input(error)) => return Err(error),
        }
        Ok(())
    }

    pub fn pair(&self, units: SourceUnit) -> std::result::Result<Vec<Corridor>, &'static str> {
        if self.truncated {
            return Err("The archive exceeds the road-side evidence limit.");
        }
        let factor = units.metres();
        let ignored_islands = self.small_kerb_loops(factor);
        let mut stations = BTreeMap::<(usize, i64), Vec<Hit>>::new();
        let mut hits = 0;
        for (side, line) in self.sides.iter().enumerate() {
            if ignored_islands.contains(&side) {
                continue;
            }
            for &[a, b] in line.points.array_windows() {
                if a.iter()
                    .chain(&b)
                    .any(|value| (value * factor).abs() > 40_000_000.0)
                {
                    continue;
                }
                let delta = [(b[0] - a[0]) * factor, (b[1] - a[1]) * factor];
                let axis = usize::from(delta[0].abs() < delta[1].abs());
                if delta[axis].abs() <= f64::EPSILON {
                    continue;
                }
                let across = 1 - axis;
                let gradient = delta[across] / delta[axis];
                let first = (a[axis].min(b[axis]) * factor / STEP_M).ceil() as i64;
                let end = (a[axis].max(b[axis]) * factor / STEP_M).ceil() as i64;
                let count = end.saturating_sub(first) as usize;
                if count > HIT_LIMIT - hits {
                    return Err("Road sections exceed the pairing limit.");
                }
                hits += count;
                for station in first..end {
                    let cross = a[across] * factor
                        + gradient * (station as f64 * STEP_M - a[axis] * factor);
                    stations.entry((axis, station)).or_default().push(Hit {
                        cross,
                        gradient,
                        side,
                    });
                }
            }
        }
        let mut active = Vec::<Corridor>::new();
        let mut corridors = Vec::new();
        let mut previous = None;
        for ((axis, station), mut hits) in stations {
            if previous != Some((axis, station - 1)) {
                finish(&mut active, &mut corridors);
            }
            previous = Some((axis, station));
            hits.sort_by(|a, b| a.cross.total_cmp(&b.cross));
            let pairs = self.station_pairs(&hits);
            let mut next = Vec::new();
            let mut links = vec![None; pairs.len()];
            let mut use_counts = vec![0; active.len()];
            for (index, pair) in pairs.iter().enumerate() {
                let mut matches = active.iter().enumerate().filter(|(_, track)| {
                    track.stations.last().is_some_and(|last| {
                        (last[0][1 - axis] - pair[0].cross).abs() <= 0.85
                            && (last[1][1 - axis] - pair[1].cross).abs() <= 0.85
                    })
                });
                if let Some((prior, _)) = matches.next()
                    && matches.next().is_none()
                {
                    links[index] = Some(prior);
                    use_counts[prior] += 1;
                }
            }
            let mut prior: Vec<_> = active.drain(..).map(Some).collect();
            for ([lower, upper], link) in pairs.into_iter().zip(links) {
                let mut corridor = link
                    .filter(|&index| use_counts[index] == 1)
                    .and_then(|index| prior[index].take())
                    .unwrap_or_else(|| Corridor {
                        axis,
                        stations: Vec::new(),
                        models: BTreeSet::new(),
                    });
                let point = |cross| {
                    let mut point = [0.0; 2];
                    point[axis] = station as f64 * STEP_M;
                    point[1 - axis] = cross;
                    point
                };
                corridor
                    .stations
                    .push([point(lower.cross), point(upper.cross)]);
                for hit in [lower, upper] {
                    corridor.models.insert(self.sides[hit.side].model.clone());
                }
                next.push(corridor);
            }
            corridors.extend(prior.into_iter().flatten().filter(long_enough));
            active = next;
        }
        finish(&mut active, &mut corridors);
        Ok(corridors)
    }

    fn small_kerb_loops(&self, scale: f64) -> BTreeSet<usize> {
        type Node = (i64, i64);
        let node = |[x, y]: Point| {
            (
                (x * scale * 100.0).round() as i64,
                (y * scale * 100.0).round() as i64,
            )
        };
        let mut endpoints = BTreeMap::<usize, [(&str, Node); 2]>::new();
        let mut neighbours = BTreeMap::<(&str, Node), Vec<usize>>::new();
        for (i, line) in self.sides.iter().enumerate() {
            if line.kind != EdgeKind::Kerb {
                continue;
            }
            let (Some(a), Some(b)) = (line.points.first(), line.points.last()) else {
                continue;
            };
            let keys = [
                (line.model.as_str(), node(*a)),
                (line.model.as_str(), node(*b)),
            ];
            endpoints.insert(i, keys);
            for key in keys {
                neighbours.entry(key).or_default().push(i);
            }
        }
        let mut visited = BTreeSet::new();
        let mut loops = BTreeSet::new();
        for &start in endpoints.keys() {
            let mut stack = vec![start];
            let mut chain = Vec::new();
            let mut perimeter = 0.0;
            let mut closed = true;
            while let Some(index) = stack.pop() {
                if !visited.insert(index) {
                    continue;
                }
                chain.push(index);
                perimeter += self.sides[index]
                    .points
                    .array_windows()
                    .map(|&[a, b]| (a[0] - b[0]).hypot(a[1] - b[1]) * scale)
                    .sum::<f64>();
                for key in endpoints[&index] {
                    let connected = &neighbours[&key];
                    closed &= connected.len() == 2;
                    stack.extend(connected.iter().copied());
                }
            }
            if closed && perimeter <= 60.0 {
                loops.extend(chain);
            }
        }
        loops
    }

    fn station_pairs(&self, hits: &[Hit]) -> Vec<[Hit; 2]> {
        // The same road side can be exported twice or split between models.
        let mut unique: Vec<Hit> = Vec::new();
        for &hit in hits {
            if unique.last().is_none_or(|last| {
                (last.cross - hit.cross).abs() > 0.12
                    || self.sides[last.side].kind != self.sides[hit.side].kind
            }) {
                unique.push(hit);
            }
        }
        let mut candidates = vec![Vec::new(); unique.len()];
        for i in 0..unique.len() {
            let a = unique[i];
            for j in i + 1..unique.len() {
                let b = unique[j];
                let width =
                    (b.cross - a.cross) / (1.0 + ((a.gradient + b.gradient) / 2.0).powi(2)).sqrt();
                if b.cross - a.cross > 16.0 * 2.0_f64.sqrt() {
                    break;
                }
                let compatible = match (self.sides[a.side].kind, self.sides[b.side].kind) {
                    (EdgeKind::Kerb, EdgeKind::Kerb) => j == i + 1,
                    (EdgeKind::Bitumen(a), EdgeKind::Bitumen(b)) => a != b,
                    _ => false,
                };
                if compatible
                    && a.side != b.side
                    && WIDTH_RANGE.contains(&width)
                    && (a.gradient.atan() - b.gradient.atan()).abs() <= 20.0_f64.to_radians()
                {
                    candidates[i].push(j);
                    candidates[j].push(i);
                }
            }
        }
        (0..unique.len())
            .filter_map(|i| {
                let &[j] = candidates[i].as_slice() else {
                    return None;
                };
                (i < j && candidates[j].len() == 1).then(|| [unique[i], unique[j]])
            })
            .collect()
    }
}

fn long_enough(corridor: &Corridor) -> bool {
    corridor.stations.len() >= MIN_STATIONS
}

fn finish(active: &mut Vec<Corridor>, corridors: &mut Vec<Corridor>) {
    corridors.extend(active.drain(..).filter(long_enough));
}

fn classify(source: &SourceEntity, model: &str) -> Option<EdgeKind> {
    let mut words = model.split(|ch: char| !ch.is_alphanumeric());
    if words.any(|word| word.eq_ignore_ascii_case("kerb") || word.eq_ignore_ascii_case("curb")) {
        return Some(EdgeKind::Kerb);
    }
    let props = fields(&source.block).ok()?;
    let code = scalar(&props, "name", None).ok()??;
    let colour = scalar(&props, "colour", None).ok()??;
    if !colour.eq_ignore_ascii_case("vis rd bitumen") {
        return None;
    }
    if code.eq_ignore_ascii_case("CE00") {
        Some(EdgeKind::Bitumen(false))
    } else if code.eq_ignore_ascii_case("CE0I") {
        Some(EdgeKind::Bitumen(true))
    } else {
        None
    }
}
