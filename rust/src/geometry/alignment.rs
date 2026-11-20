use super::*;
use crate::parser::Block;

fn block<'a>(item: Option<&&'a Item>) -> Option<&'a Block> {
    match item.copied() {
        Some(Item::Block(block)) => Some(block),
        _ => None,
    }
}

fn intersection_vertices(
    props: &Fields<'_>,
    null: &str,
    cancel: Option<&AtomicBool>,
) -> Result<Option<Vec<Point>>> {
    if !flag(get(props, "horizontal_ips_only", "false")?)?
        || !flag(get(props, "valid_horizontal", "false")?)?
    {
        return Ok(None);
    }
    let Some(parts) = block(props.get("horizontal_parts")) else {
        return Ok(None);
    };
    if parts.items.len() < 4 || parts.items.len() % 2 != 0 {
        return Ok(None);
    }
    let mut points = Vec::new();
    for pair in parts.items.chunks_exact(2) {
        checkpoint(cancel)?;
        let [Item::Token(kind), Item::Block(body)] = pair else {
            return Ok(None);
        };
        if !kind.value.eq_ignore_ascii_case("ip") {
            return Ok(None);
        }
        let props = fields(body)?;
        if props.keys().any(|key| {
            !matches!(
                key.as_str(),
                "x" | "y" | "id" | "time_created" | "time_updated"
            )
        }) {
            return Err(unsupported(
                "IP-only alignment contains unverified IP fields.",
            ));
        }
        let (x, y) = (scalar(&props, "x", None)?, scalar(&props, "y", None)?);
        if x == Some(null) || y == Some(null) {
            return Err(unsupported("Intersection point has a missing coordinate."));
        }
        points.push([number(x, "intersection X")?, number(y, "intersection Y")?]);
    }
    if points.iter().all(|point| *point == points[0]) {
        return Err(unsupported(
            "IP-only alignment has fewer than two distinct points.",
        ));
    }
    Ok(Some(points))
}

fn default_alignment_modifiers(item: Option<&&Item>) -> Result<bool> {
    let Some(item) = item else { return Ok(true) };
    let Item::Block(body) = item else {
        return Ok(false);
    };
    let props = fields(body)?;
    if props.keys().any(|key| {
        !matches!(
            key.as_str(),
            "use_reference_chainage"
                | "major_chainage_interval"
                | "start"
                | "end"
                | "chainage"
                | "default_hip"
                | "default_vip"
        )
    }) || flag(get(&props, "use_reference_chainage", "false")?)?
    {
        return Ok(false);
    }
    for key in ["start", "end", "chainage"] {
        let Some(item) = props.get(key) else { continue };
        let Item::Block(body) = item else {
            return Ok(false);
        };
        let settings = fields(body)?;
        let modes = if key == "chainage" {
            [("mode", "start_point")].as_slice()
        } else {
            [("horz_mode", "default"), ("vert_mode", "relative")].as_slice()
        };
        if settings.keys().any(|name| {
            !matches!(name.as_str(), "x" | "y") && !modes.iter().any(|(mode, _)| *mode == name)
        }) {
            return Ok(false);
        }
        for (mode, expected) in modes {
            if get(&settings, mode, expected)? != *expected {
                return Ok(false);
            }
        }
        for axis in ["x", "y"] {
            if numeric(&settings, axis, Some("0"), "number")? != 0.0 {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

struct FixedLine {
    start: Point,
    end: Point,
    direction: Point,
    visible: bool,
}

fn fixed_line(body: &Block, null: &str) -> Result<FixedLine> {
    let props = fields(body)?;
    if props.keys().any(|key| {
        !matches!(
            key.as_str(),
            "id" | "time_created"
                | "time_updated"
                | "x1"
                | "y1"
                | "x2"
                | "y2"
                | "offset"
                | "visible"
        )
    }) {
        return Err(unsupported("Unverified fixed-line parameters."));
    }
    let mut coordinates = [0.0; 4];
    for (coordinate, key) in coordinates.iter_mut().zip(["x1", "y1", "x2", "y2"]) {
        let value = scalar(&props, key, None)?;
        if value == Some(null) {
            return Err(unsupported("Fixed line has a missing coordinate."));
        }
        *coordinate = number(value, key)?;
    }
    let [x1, y1, x2, y2] = coordinates;
    let length = (x2 - x1).hypot(y2 - y1);
    if length == 0.0 {
        return Err(unsupported("Zero-length fixed line."));
    }
    let (ux, uy) = ((x2 - x1) / length, (y2 - y1) / length);
    let offset = numeric(&props, "offset", Some("0"), "fixed-line offset")?;
    Ok(FixedLine {
        start: [x1 + uy * offset, y1 - ux * offset],
        end: [x2 + uy * offset, y2 - ux * offset],
        direction: [ux, uy],
        visible: flag(get(&props, "visible", "true")?)?,
    })
}

fn tangent_arc_plan(parts: Option<&&Item>, null: &str) -> Result<Option<(Vec<Point>, Vec<f64>)>> {
    let Some(parts) = block(parts) else {
        return Ok(None);
    };
    let [
        Item::Token(first),
        before,
        Item::Token(middle),
        curve,
        Item::Token(last),
        after,
    ] = parts.items.as_slice()
    else {
        return Ok(None);
    };
    if first.value != "fixed_line"
        || middle.value != "free_arc_radius"
        || last.value != "fixed_line"
    {
        return Ok(None);
    }
    let (Item::Block(before), Item::Block(curve), Item::Block(after)) = (before, curve, after)
    else {
        return Err(unsupported("Malformed tangent arc parts."));
    };
    let before = fixed_line(before, null)?;
    let after = fixed_line(after, null)?;
    let props = fields(curve)?;
    if props
        .keys()
        .any(|key| !matches!(key.as_str(), "id" | "time_created" | "time_updated" | "r"))
    {
        return Err(unsupported("Unverified free-radius arc parameters."));
    }
    let raw = scalar(&props, "r", None)?;
    if raw == Some(null) {
        return Err(unsupported("Free arc has a missing radius."));
    }
    let radius = number(raw, "free arc radius")?;
    let (u, v) = (before.direction, after.direction);
    let cross = u[0] * v[1] - u[1] * v[0];
    let sweep = cross.atan2(u[0] * v[0] + u[1] * v[1]);
    if cross.abs() < 1e-10 || radius == 0.0 || sweep * radius >= 0.0 {
        return Err(unsupported(
            "Fixed lines and radius do not define a supported minor tangent arc.",
        ));
    }
    let delta = [
        after.start[0] - before.start[0],
        after.start[1] - before.start[1],
    ];
    let crossing = (delta[0] * v[1] - delta[1] * v[0]) / cross;
    let setback = radius.abs() * (sweep.abs() / 2.0).tan();
    let entry_distance = crossing - setback;
    let entry = std::array::from_fn(|i| before.start[i] + entry_distance * u[i]);
    let exit = std::array::from_fn(|i| before.start[i] + crossing * u[i] + setback * v[i]);
    let exit_distance: f64 = (0..2).map(|i| (after.end[i] - exit[i]) * v[i]).sum();
    if (before.visible && entry_distance < -1e-6) || (after.visible && exit_distance < -1e-6) {
        return Err(unsupported(
            "Tangent arc cuts beyond a visible fixed-line endpoint.",
        ));
    }
    let mut points = vec![entry, exit];
    let mut bulges = vec![(sweep / 4.0).tan(), 0.0];
    if before.visible && entry_distance > 1e-6 {
        points.insert(0, before.start);
        bulges.insert(0, 0.0);
    }
    if after.visible && exit_distance > 1e-6 {
        points.push(after.end);
        bulges.push(0.0);
    }
    Ok(Some((points, bulges)))
}

struct ProfileSegment {
    start: f64,
    end: f64,
    height: f64,
    grade: f64,
    quadratic: f64,
}

fn profile_segments(profile: &Block, null: &str) -> Result<Vec<ProfileSegment>> {
    let props = fields(profile)?;
    let raw = props.get("data_2d");
    let data = if block(raw).is_some() {
        block_values(raw.copied())?
    } else {
        vec![]
    };
    if data.len() < 4 || data.len() % 2 != 0 {
        return Err(unsupported(
            "Alignment has no usable solved vertical vertices.",
        ));
    }
    if data
        .iter()
        .any(|item| item.eq_ignore_ascii_case("null") || *item == null)
    {
        return Err(unsupported(
            "Solved alignment profile has missing coordinates.",
        ));
    }
    let stations = data
        .iter()
        .step_by(2)
        .map(|value| number(Some(value), "profile chainage"))
        .collect::<Result<Vec<_>>>()?;
    let levels = data
        .iter()
        .skip(1)
        .step_by(2)
        .map(|value| number(Some(value), "profile elevation"))
        .collect::<Result<Vec<_>>>()?;
    if stations.windows(2).any(|pair| pair[1] <= pair[0]) {
        return Err(unsupported("Alignment profile chainages must increase."));
    }
    let count = stations.len() - 1;
    let geometry = match props.get("geometry_data") {
        None => None,
        Some(Item::Block(body)) if [2 * count, 2 * (count + 1)].contains(&body.items.len()) => {
            Some(body)
        }
        _ => return Err(unsupported("Alignment profile segment count mismatch.")),
    };
    let mut segments = Vec::with_capacity(count);
    for i in 0..count {
        let (a, b, za, zb) = (stations[i], stations[i + 1], levels[i], levels[i + 1]);
        let mut grade = (zb - za) / (b - a);
        let mut quadratic = 0.0;
        if let Some(body) = geometry {
            let [Item::Token(kind), Item::Block(body)] = &body.items[i * 2..i * 2 + 2] else {
                return Err(unsupported("Malformed alignment profile segment."));
            };
            if kind.value.eq_ignore_ascii_case("parabola") {
                let vip = fields(body)?;
                let mid = numeric(&vip, "chainage", None, "vertical intersection chainage")?;
                let height = numeric(&vip, "height", None, "vertical intersection height")?;
                if mid <= a || mid >= b || (mid - (a + b) / 2.0).abs() > 1e-6 {
                    return Err(unsupported(
                        "Only symmetric solved alignment parabolas are supported.",
                    ));
                }
                grade = (height - za) / (mid - a);
                quadratic = ((zb - height) / (b - mid) - grade) / (2.0 * (b - a));
            } else if !kind.value.eq_ignore_ascii_case("straight") {
                return Err(unsupported(format!(
                    "Unsupported alignment profile segment: {}.",
                    kind.value
                )));
            }
        }
        segments.push(ProfileSegment {
            start: a,
            end: b,
            height: za,
            grade,
            quadratic,
        });
    }
    Ok(segments)
}

fn alignment_elevations(
    primitives: Vec<Primitive>,
    profile: &Block,
    start: f64,
    null: &str,
    options: GeometryOptions,
    warn: &mut Warning<'_>,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<Primitive>> {
    let segments = profile_segments(profile, null)?;
    let mut output = Vec::new();
    let mut station = start;
    let mut remaining = 100_000_usize;
    for primitive in primitives {
        if primitive.kind != Kind::Polyline {
            return Err(unsupported(
                "Alignment plan must consist of solved line and arc segments.",
            ));
        }
        let mut sampled =
            Primitive::new(Kind::Polyline, vec![], &primitive.colour, &primitive.style);
        for i in 0..primitive.edge_count() {
            checkpoint(cancel)?;
            let a = primitive.points[i];
            let b = primitive.points[(i + 1) % primitive.points.len()];
            let bulge = primitive.bulges[i];
            let arc = (bulge != 0.0)
                .then(|| arc_parameters(a, b, bulge))
                .transpose()?;
            let length = arc.map_or_else(|| distance(a, b), |arc| arc.sweep.abs() * arc.radius);
            if length == 0.0 {
                continue;
            }
            let end = station + length;
            if station < segments[0].start - 1e-6 || end > segments[segments.len() - 1].end + 1e-6 {
                return Err(unsupported(
                    "Solved vertical profile does not cover the horizontal alignment.",
                ));
            }
            let first = segments
                .partition_point(|segment| segment.start <= station)
                .saturating_sub(1);
            for profile in &segments[first..] {
                let lo = station.max(profile.start);
                let hi = end.min(profile.end);
                if hi <= lo {
                    break;
                }
                let extremum = if profile.quadratic != 0.0 {
                    profile.start - profile.grade / (2.0 * profile.quadratic)
                } else {
                    lo
                };
                let boundaries = if lo < extremum && extremum < hi {
                    vec![lo, extremum, hi]
                } else {
                    vec![lo, hi]
                };
                let plan_rate = arc.map_or(0.0, |arc| {
                    ((1.0 / arc.radius) / 8.0 / options.plan_tolerance).sqrt()
                });
                let elevation_rate =
                    (profile.quadratic.abs() / 4.0 / options.elevation_tolerance).sqrt();
                for pair in boundaries.windows(2) {
                    let (left, right) = (pair[0], pair[1]);
                    let needed = (right - left) * plan_rate.max(elevation_rate);
                    let exhausted = || unsupported("Alignment sampling vertex limit exceeded.");
                    if !needed.is_finite() || needed >= remaining as f64 {
                        return Err(exhausted());
                    }
                    let steps = (needed.ceil() as usize).max(1);
                    remaining = remaining.checked_sub(steps + 1).ok_or_else(exhausted)?;
                    for k in usize::from(!sampled.points.is_empty())..=steps {
                        checkpoint(cancel)?;
                        let chainage = if k == steps {
                            right
                        } else {
                            left + (right - left) * k as f64 / steps as f64
                        };
                        let t = (chainage - station) / length;
                        let xy = arc.map_or_else(
                            || std::array::from_fn(|axis| a[axis] + t * (b[axis] - a[axis])),
                            |arc| arc.point(t),
                        );
                        let offset = chainage - profile.start;
                        sampled.points.push(xy);
                        sampled.elevations.push(
                            profile.height + offset * (profile.grade + offset * profile.quadratic),
                        );
                    }
                }
                if hi >= end {
                    break;
                }
            }
            station = end;
        }
        if sampled.points.len() < 2 {
            return Err(unsupported("Empty sampled alignment geometry."));
        }
        output.push(sampled);
    }
    let vertices: usize = output.iter().map(|path| path.points.len()).sum();
    warn(
        "ALIGNMENT_SAMPLED",
        &format!(
            "Sampled {vertices} alignment vertices using a second-derivative chord error bound with plan tolerance {} and elevation tolerance {} drawing units. Alignment labels and civil attributes omitted.",
            options.plan_tolerance, options.elevation_tolerance
        ),
    );
    Ok(output)
}

pub(super) fn convert_alignment(
    source: &SourceEntity,
    options: GeometryOptions,
    warn: &mut Warning<'_>,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<Primitive>> {
    let mut props = fields(&source.block)?;
    let horizontal = block(props.get("horizontal_data"));
    let solved = horizontal.map(fields).transpose()?.unwrap_or_default();
    let null = &source.defaults["null"];
    let mut reconstructed = None;
    let alignment_path = |points, bulges, closed| -> Result<Primitive> {
        let colour = get(
            &solved,
            "colour",
            get(&props, "colour", &source.defaults["colour"])?,
        )?;
        let style = get(
            &solved,
            "style",
            get(&props, "style", &source.defaults["style"])?,
        )?;
        let mut path = Primitive::new(Kind::Polyline, points, colour, style);
        path.bulges = bulges;
        path.closed = closed;
        Ok(path)
    };
    if let Some(points) = intersection_vertices(&props, null, cancel)? {
        let closed = flag(get(&props, "closed", "0")?)?;
        if [
            "geometry_data",
            "radius_data",
            "major_data",
            "data_3d",
            "data",
        ]
        .iter()
        .chain(&TRANSFORMS)
        .any(|key| solved.contains_key(*key))
        {
            return Err(unsupported(
                "IP-only alignment has an unverified solved plan representation.",
            ));
        }
        let mut has_solved = false;
        if let Some(&raw) = solved.get("data_2d") {
            if !matches!(raw, Item::Block(_)) {
                return Err(unsupported("Malformed solved IP alignment vertices."));
            }
            let values = block_values(Some(raw))?;
            if !values.is_empty() {
                has_solved = true;
                let inconsistent =
                    || unsupported("Intersection points disagree with solved alignment geometry.");
                if values.len() != points.len() * 2 {
                    return Err(inconsistent());
                }
                for (value, coordinate) in values.into_iter().zip(points.iter().flatten()) {
                    if (number(Some(value), "number")? - coordinate).abs() > 1e-6 {
                        return Err(inconsistent());
                    }
                }
                if flag(get(&solved, "closed", "0")?)? != closed {
                    return Err(inconsistent());
                }
            }
        }
        if !has_solved {
            if TRANSFORMS.iter().any(|key| props.contains_key(*key)) {
                return Err(unsupported(
                    "IP-only alignment has an unsupported source transform.",
                ));
            }
            let bulges = vec![0.0; points.len()];
            reconstructed = Some(alignment_path(points, bulges, closed)?);
            warn(
                "ALIGNMENT_IP_RECONSTRUCTED",
                "Plan reconstructed from verified straight intersection points.",
            );
        }
    } else {
        let solved_vertices = ["data_2d", "data_3d", "data"]
            .iter()
            .filter_map(|key| solved.get(*key))
            .any(|value| !matches!(value, Item::Block(body) if body.items.is_empty()));
        if !solved_vertices
            && flag(get(&props, "valid_horizontal", "false")?)?
            && !flag(get(&props, "closed", "0")?)?
            && let Some((points, bulges)) = tangent_arc_plan(props.get("horizontal_parts"), null)?
        {
            if TRANSFORMS
                .iter()
                .any(|key| props.contains_key(*key) || solved.contains_key(*key))
                || !default_alignment_modifiers(props.get("geometry_modifiers"))?
            {
                return Err(unsupported(
                    "Tangent arc reconstruction requires unmodified source geometry.",
                ));
            }
            reconstructed = Some(alignment_path(points, bulges, false)?);
            warn(
                "ALIGNMENT_TANGENT_ARC_RECONSTRUCTED",
                "Plan reconstructed from two fixed lines and a free-radius tangent arc.",
            );
        }
    }
    if horizontal.is_none() && reconstructed.is_none() {
        return Err(unsupported(
            "Alignment has no solved horizontal_data block.",
        ));
    }
    if reconstructed.is_none()
        && let Some(parts) = block(props.get("horizontal_parts"))
    {
        for pair in parts.items.chunks_exact(2) {
            if let [Item::Token(kind), Item::Block(body)] = pair
                && matches!(
                    kind.value.to_lowercase().as_str(),
                    "transition_compound_curve" | "transition_compound_taper"
                )
                && flag(get(&fields(body)?, "active", "true")?)?
            {
                warn(
                    "ALIGNMENT_TRANSITION_SOLVED_ONLY",
                    "Used the exported solved plan for the active transition. The original transition definition was not evaluated.",
                );
                break;
            }
        }
    }
    if options.keep_z {
        let profile = block(props.get("vertical_data"))
            .ok_or_else(|| unsupported("Keep Z requires a solved alignment profile."))?;
        let start = numeric(
            &solved,
            "chainage",
            Some(get(&props, "chainage", "0")?),
            "alignment chainage",
        )?;
        let plan = match reconstructed {
            Some(path) => vec![path],
            None => primitive_geometry(
                solved,
                &source.defaults,
                "super",
                GeometryOptions::default(),
                warn,
                cancel,
            )?,
        };
        return alignment_elevations(plan, profile, start, null, options, warn, cancel);
    }
    warn(
        "ALIGNMENT_PLAN_ONLY",
        "Horizontal geometry only; labels, vertical design and civil semantics are omitted.",
    );
    if let Some(path) = reconstructed {
        return Ok(vec![path]);
    }
    props.extend(solved);
    primitive_geometry(
        props,
        &source.defaults,
        "super_alignment",
        options,
        warn,
        cancel,
    )
}
