use std::collections::BTreeMap;

use geo::algorithm::bool_ops::unary_union;
use geo::algorithm::buffer::{BufferStyle, LineCap, LineJoin};
use geo::orient::{Direction, Orient};
use geo::{
    BooleanOps, Buffer, Coord, LineString, MultiLineString, MultiPolygon, Polygon, Validation,
};

use crate::centreline::{paired_surface, surface_centreline, uncovered_by_mapped};
use crate::{Diagnostic, Feature, Geometry, Output, Point, RoadLine};

const SNAP_M: f64 = 0.002;
const LANE_WIDTH_M: f64 = 3.3;

#[derive(Clone, Copy)]
struct Width {
    metres: f64,
    source: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Grade(i16, bool, bool);

struct Corridor {
    id: String,
    grade: Grade,
    area: MultiPolygon,
    sides: MultiLineString,
    width: Option<Width>,
}

pub(crate) fn build(features: Vec<Feature>, clip: Option<[f64; 4]>, output: &mut Output) {
    let mut corridors = Vec::new();
    let mut mapped = BTreeMap::<Grade, Vec<LineString>>::new();
    let mut pairs = BTreeMap::<(Grade, String), Vec<MarkedSide>>::new();
    for feature in features {
        let grade = grade(&feature, output);
        match feature.geometry {
            Geometry::Polygon(rings) if feature.tags.contains_key("area:highway") => {
                let mut rings = rings.into_iter().map(path);
                let polygon = Polygon::new(rings.next().expect("validated ring"), rings.collect())
                    .orient(Direction::Default);
                if let Err(error) = polygon.check_validation() {
                    diagnostic(output, &feature.id, "invalid_polygon", error.to_string());
                    continue;
                }
                let sides = MultiLineString(polygon.rings().cloned().collect());
                corridors.push(Corridor {
                    id: feature.id,
                    grade,
                    area: MultiPolygon(vec![polygon]),
                    sides,
                    width: None,
                });
            }
            Geometry::LineString(points) if feature.tags.contains_key("road_side") => {
                let side = &feature.tags["road_side"];
                if let Some(corridor) = feature
                    .tags
                    .get("corridor_id")
                    .filter(|id| !id.trim().is_empty())
                    && (side == "left" || side == "right")
                {
                    let side = MarkedSide {
                        id: feature.id,
                        left: side == "left",
                        line: path(points),
                    };
                    output
                        .edges
                        .push(road_line(&side.id, &side.line, "mapped_outline", None));
                    pairs
                        .entry((grade, corridor.clone()))
                        .or_default()
                        .push(side);
                    continue;
                }
                diagnostic(
                    output,
                    &feature.id,
                    "incomplete_road_sides",
                    "road side requires left/right and a corridor ID",
                );
            }
            Geometry::LineString(points) if feature.tags.contains_key("highway") => {
                let width = road_width(&feature.id, &feature.tags, output);
                let line = path(points);
                mapped.entry(grade).or_default().push(line.clone());
                let mut centreline = road_line(&feature.id, &line, "mapped_centreline", None);
                if width.is_none() {
                    centreline.edge_status = Some("centreline_only");
                }
                output.centrelines.push(centreline);
                let Some(width) = width else {
                    diagnostic(
                        output,
                        &feature.id,
                        "centreline_only",
                        "no usable carriageway width",
                    );
                    continue;
                };
                let area = line.buffer_with_style(
                    BufferStyle::new(width.metres / 2.0)
                        .line_cap(LineCap::Butt)
                        .line_join(LineJoin::Miter(0.4)),
                );
                let sides = without_end_caps(&area, &line, width.metres / 2.0);
                corridors.push(Corridor {
                    id: feature.id,
                    grade,
                    area,
                    sides,
                    width: Some(width),
                });
            }
            _ => {}
        }
    }
    for corridor in corridors.iter().filter(|corridor| corridor.width.is_none()) {
        synthesise(
            &corridor.area.0[0],
            &[&corridor.id],
            mapped.get(&corridor.grade).map_or(&[], Vec::as_slice),
            output,
        );
    }
    for ((grade, _), sides) in pairs {
        match sides.as_slice() {
            [first, second] if first.left != second.left => {
                if let Some(polygon) = paired_surface(&first.line, &second.line) {
                    synthesise(
                        &polygon,
                        &[&first.id, &second.id],
                        mapped.get(&grade).map_or(&[], Vec::as_slice),
                        output,
                    );
                    continue;
                }
            }
            _ => {}
        }
        for side in sides {
            diagnostic(
                output,
                &side.id,
                "incomplete_road_sides",
                "a single credible pair of opposite sides is required",
            );
        }
    }
    let mut grades: BTreeMap<Grade, Vec<Corridor>> = BTreeMap::new();
    for corridor in corridors {
        grades.entry(corridor.grade).or_default().push(corridor);
    }
    for corridors in grades.into_values() {
        let mapped: Vec<_> = corridors
            .iter()
            .filter(|item| item.width.is_none())
            .flat_map(|item| item.area.iter())
            .collect();
        // A narrower island or median inside a mapped surface is still covered by the mapping.
        let footprints: Vec<_> = mapped
            .iter()
            .map(|polygon| Polygon::new(polygon.exterior().clone(), Vec::new()))
            .collect();
        let surfaces = unary_union(footprints.iter());
        let mapped_mask = if surfaces.0.is_empty() {
            MultiPolygon(Vec::new())
        } else {
            surfaces.buffer(SNAP_M)
        };
        let boundary_band = if corridors.len() > 1 {
            let estimated: Vec<_> = corridors
                .iter()
                .filter(|item| item.width.is_some())
                .map(|item| {
                    let area = if surfaces.0.is_empty() {
                        item.area.clone()
                    } else {
                        item.area.difference(&surfaces)
                    };
                    area.orient(Direction::Default)
                })
                .collect();
            let union = unary_union(
                mapped
                    .into_iter()
                    .chain(estimated.iter().flat_map(|area| area.iter())),
            );
            let boundary =
                MultiLineString(union.iter().flat_map(Polygon::rings).cloned().collect());
            Some(boundary.buffer(SNAP_M))
        } else {
            None
        };
        for corridor in corridors {
            let sides = if corridor.width.is_some() && !mapped_mask.0.is_empty() {
                mapped_mask.clip(&corridor.sides, true)
            } else {
                corridor.sides
            };
            let sides = match &boundary_band {
                Some(band) => band.clip(&sides, false),
                None => sides,
            };
            let provenance = if corridor.width.is_some() {
                "estimated_edge"
            } else {
                "mapped_outline"
            };
            output.edges.extend(
                sides
                    .iter()
                    .filter(|line| line_length(line) > SNAP_M * 2.0)
                    .map(|line| road_line(&corridor.id, line, provenance, corridor.width)),
            );
        }
    }
    if let Some([west, south, east, north]) = clip {
        let window = geo::Rect::new((west, south), (east, north)).to_polygon();
        clip_lines(&mut output.edges, &window);
        clip_lines(&mut output.centrelines, &window);
    }
}

struct MarkedSide {
    id: String,
    left: bool,
    line: LineString,
}

fn synthesise(polygon: &Polygon, ids: &[&str], mapped: &[LineString], output: &mut Output) {
    let Some(line) = surface_centreline(polygon) else {
        diagnostic(
            output,
            ids[0],
            "ambiguous_centreline",
            "road surface has no reliable unbranched midpoint path",
        );
        return;
    };
    for part in uncovered_by_mapped(polygon, mapped, line)
        .iter()
        .filter(|part| line_length(part) > 0.5)
    {
        let mut line = road_line(ids[0], part, "synthesised_centreline", None);
        line.source_ids = ids.iter().map(|id| (*id).to_owned()).collect();
        line.source_ids.sort_unstable();
        output.centrelines.push(line);
    }
}

fn line_length(line: &LineString) -> f64 {
    line.lines()
        .map(|segment| {
            let delta = segment.end - segment.start;
            delta.x.hypot(delta.y)
        })
        .sum()
}

fn clip_lines(lines: &mut Vec<RoadLine>, window: &Polygon) {
    let clipped = std::mem::take(lines).into_iter().flat_map(|mut road| {
        let original = MultiLineString(vec![path(std::mem::take(&mut road.points))]);
        window
            .clip(&original, false)
            .0
            .into_iter()
            .filter(|line| line_length(line) > SNAP_M * 2.0)
            .map(move |line| RoadLine {
                points: line.0.into_iter().map(|point| [point.x, point.y]).collect(),
                ..road.clone()
            })
    });
    lines.extend(clipped);
}

fn path(points: Vec<Point>) -> LineString {
    LineString::from(points.into_iter().map(|[x, y]| (x, y)).collect::<Vec<_>>())
}

fn road_line(
    id: &str,
    line: &LineString,
    provenance: &'static str,
    width: Option<Width>,
) -> RoadLine {
    RoadLine {
        source_ids: vec![id.to_owned()],
        points: line.0.iter().map(|p| [p.x, p.y]).collect(),
        provenance,
        width_m: width.map(|w| w.metres),
        width_source: width.map(|w| w.source),
        edge_status: None,
    }
}

fn grade(feature: &Feature, output: &mut Output) -> Grade {
    let tagged = |key| feature.tags.get(key).is_some_and(|tag| tag != "no");
    let bridge = tagged("bridge");
    let tunnel = tagged("tunnel");
    let default = i16::from(bridge) - i16::from(tunnel);
    let layer = feature.tags.get("layer").map_or(default, |value| {
        value.parse().unwrap_or_else(|_| {
            diagnostic(
                output,
                &feature.id,
                "invalid_layer",
                "layer must be an integer",
            );
            default
        })
    });
    Grade(layer, bridge, tunnel)
}

fn diagnostic(output: &mut Output, id: &str, code: &'static str, message: impl Into<String>) {
    output.diagnostics.push(Diagnostic {
        feature_id: Some(id.to_owned()),
        code,
        message: message.into(),
    });
}

fn road_width(id: &str, tags: &BTreeMap<String, String>, output: &mut Output) -> Option<Width> {
    if let Some(raw) = tags.get("width") {
        let raw = raw.trim();
        let metres = raw.strip_suffix(" m").unwrap_or(raw).parse::<f64>().ok();
        if let Some(metres) =
            metres.filter(|width| width.is_finite() && (0.25..=100.0).contains(width))
        {
            return Some(Width {
                metres,
                source: "mapped_width",
            });
        }
        diagnostic(output, id, "invalid_width", "width must be 0.25–100 metres");
    }
    if let Some(raw) = tags.get("lanes") {
        if let Some(lanes) = raw
            .parse::<u8>()
            .ok()
            .filter(|lanes| (1..=20).contains(lanes))
        {
            return Some(Width {
                metres: f64::from(lanes) * LANE_WIDTH_M,
                source: "lane_count",
            });
        }
        diagnostic(
            output,
            id,
            "invalid_lanes",
            "lanes must be an integer between 1 and 20",
        );
    }
    let metres = match tags.get("highway")?.as_str() {
        "motorway" | "trunk" => 10.5,
        "primary" | "secondary" => 8.0,
        "tertiary" | "residential" | "unclassified" => 6.5,
        "living_street" | "service" => 4.0,
        _ => return None,
    };
    Some(Width {
        metres,
        source: "road_class",
    })
}

fn without_end_caps(area: &MultiPolygon, centre: &LineString, radius: f64) -> MultiLineString {
    let points = &centre.0;
    let n = points.len();
    if points[0] == points[n - 1] {
        return MultiLineString(
            area.iter()
                .flat_map(|polygon| polygon.rings().cloned())
                .collect(),
        );
    }
    let ends = [(points[0], points[1]), (points[n - 1], points[n - 2])];
    let on_cap = |p: Coord, end: Coord, neighbour: Coord| {
        let delta = p - end;
        let axis = neighbour - end;
        delta.x.hypot(delta.y) < radius + SNAP_M
            && (delta.x * axis.x + delta.y * axis.y).abs() < SNAP_M * axis.x.hypot(axis.y)
    };
    let mut sides = Vec::new();
    for ring in area.iter().flat_map(Polygon::rings) {
        let segments: Vec<_> = ring.lines().collect();
        let cap = |segment: &geo::Line| {
            ends.iter().any(|&(end, next)| {
                on_cap(segment.start, end, next) && on_cap(segment.end, end, next)
            })
        };
        let Some(first_cap) = segments.iter().position(cap) else {
            sides.push(ring.clone());
            continue;
        };
        let mut path = Vec::new();
        for segment in segments
            .iter()
            .cycle()
            .skip(first_cap + 1)
            .take(segments.len())
        {
            if cap(segment) {
                if !path.is_empty() {
                    sides.push(LineString(std::mem::take(&mut path)));
                }
            } else {
                if path.is_empty() {
                    path.push(segment.start);
                }
                path.push(segment.end);
            }
        }
    }
    MultiLineString(sides)
}
