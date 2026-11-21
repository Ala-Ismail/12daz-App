use geo::{
    BooleanOps, Coord, Euclidean, Length, LineString, MultiLineString, Polygon, Relate, Validation,
};

pub(crate) fn surface_centreline(polygon: &Polygon) -> Option<LineString> {
    let origin = *polygon.exterior().0.first()?;
    let segments: Vec<_> = polygon.rings().flat_map(LineString::lines).collect();
    let longest = polygon
        .exterior()
        .lines()
        .max_by(|left, right| Euclidean.length(left).total_cmp(&Euclidean.length(right)))?;
    let delta = longest.end - longest.start;
    let bearing = delta.y.atan2(delta.x);
    // Add the longest side's exact bearing to avoid tilting regular rotated corridors.
    std::iter::once(bearing)
        .chain((0..16).map(|step| bearing + f64::from(step) * std::f64::consts::PI / 16.0))
        .filter_map(|angle| {
            let (sin, cos) = angle.sin_cos();
            let local = |point: Coord| {
                let offset = point - origin;
                [
                    offset.x * cos + offset.y * sin,
                    -offset.x * sin + offset.y * cos,
                ]
            };
            let sides: Vec<_> = segments
                .iter()
                .map(|edge| [local(edge.start), local(edge.end)])
                .collect();
            let (score, points) = cross_sections(&sides)?;
            let line = LineString(
                points
                    .into_iter()
                    .map(|[x, y]| Coord {
                        x: origin.x + x * cos - y * sin,
                        y: origin.y + x * sin + y * cos,
                    })
                    .collect(),
            );
            polygon.relate(&line).is_contains().then_some((score, line))
        })
        .max_by(|(left, _), (right, _)| left.total_cmp(right))
        .map(|(_, line)| line)
}

fn cross_sections(sides: &[[[f64; 2]; 2]]) -> Option<(f64, Vec<[f64; 2]>)> {
    let min = sides
        .iter()
        .map(|edge| edge[0][0])
        .fold(f64::INFINITY, f64::min);
    let max = sides
        .iter()
        .map(|edge| edge[0][0])
        .fold(f64::NEG_INFINITY, f64::max);
    let span = max - min;
    if span < 4.0 {
        return None;
    }
    let samples = ((span / 1.0).ceil() as usize).clamp(4, 512);
    let inset = 0.1;
    let mut points = Vec::with_capacity(samples + 1);
    let mut widths = Vec::with_capacity(samples + 1);
    let mut crossed = Vec::new();
    for step in 0..=samples {
        let x = min + inset + (span - 2.0 * inset) * step as f64 / samples as f64;
        crossed.clear();
        for &[[ax, ay], [bx, by]] in sides {
            if (ax <= x && x < bx) || (bx <= x && x < ax) {
                crossed.push(ay + (by - ay) * (x - ax) / (bx - ax));
            }
        }
        let &[ym, yn] = crossed.as_slice() else {
            return None;
        };
        widths.push((ym - yn).abs());
        points.push([x, (ym + yn) / 2.0]);
    }
    widths.sort_by(f64::total_cmp);
    let median = widths[widths.len() / 2];
    let widest = *widths.last()?;
    if median < 0.5 || span < median * 2.0 || widest > median * 2.5 {
        return None;
    }
    // Prefer narrow, consistent transects over cuts that run obliquely along a road side.
    Some((span / widest, points))
}

pub(crate) fn paired_surface(left: &LineString, right: &LineString) -> Option<Polygon> {
    let [left_start, left_end] = [*left.0.first()?, *left.0.last()?];
    let [right_start, right_end] = [*right.0.first()?, *right.0.last()?];
    let gap = |a: Coord, b: Coord| (a.x - b.x).hypot(a.y - b.y);
    let direct = gap(left_start, right_start) + gap(left_end, right_end);
    let crossed = gap(left_start, right_end) + gap(left_end, right_start);
    let mut opposite = right.0.clone();
    if direct <= crossed {
        opposite.reverse();
    }
    let ends = [
        gap(left_end, *opposite.first()?),
        gap(left_start, *opposite.last()?),
    ];
    let lengths = [Euclidean.length(left), Euclidean.length(right)];
    if ends.iter().any(|width| !(0.5..=30.0).contains(width))
        || lengths[0].min(lengths[1]) < 0.5 * lengths[0].max(lengths[1])
    {
        return None;
    }
    let mut ring = left.0.clone();
    ring.extend(opposite);
    let polygon = Polygon::new(LineString(ring), Vec::new());
    polygon.is_valid().then_some(polygon)
}

pub(crate) fn uncovered_by_mapped(
    polygon: &Polygon,
    mapped: &[LineString],
    candidate: LineString,
) -> MultiLineString {
    use geo::{
        Buffer,
        algorithm::buffer::{BufferStyle, LineCap},
    };

    if mapped.is_empty() {
        return MultiLineString(vec![candidate]);
    }
    let near = candidate.buffer(2.0);
    let covered = polygon.clip(&MultiLineString(mapped.to_vec()), false);
    let minimum_agreement = (Euclidean.length(&candidate) * 0.2).clamp(5.0, 8.0);
    let compatible = MultiLineString(
        near.clip(&covered, false)
            .0
            .into_iter()
            .filter(|part| Euclidean.length(part) >= minimum_agreement)
            .collect(),
    );
    if compatible.0.is_empty() {
        return MultiLineString(vec![candidate]);
    }
    compatible
        .buffer_with_style(BufferStyle::new(2.0).line_cap(LineCap::Butt))
        .clip(&MultiLineString(vec![candidate]), true)
}
