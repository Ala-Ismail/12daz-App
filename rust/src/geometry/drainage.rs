use super::*;

pub(super) fn drainage_plan(
    source: &SourceEntity,
    keep_z: bool,
    warn: &mut Warning<'_>,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<Primitive>> {
    if keep_z {
        return Err(unsupported(
            "Drainage pipe centreline elevations are not verified for Keep Z.",
        ));
    }
    let mut props = Fields::new();
    let mut pits = Vec::new();
    let mut pipes = 0;
    if source.block.items.len() % 2 != 0 {
        return Err(unsupported("Malformed drainage object."));
    }
    for pair in source.block.items.chunks_exact(2) {
        checkpoint(cancel)?;
        let Item::Token(name) = &pair[0] else {
            continue;
        };
        let kind = name.value.to_lowercase();
        let value = &pair[1];
        if matches!(kind.as_str(), "pit" | "pit_v2" | "pipe") {
            let Item::Block(body) = value else {
                return Err(unsupported("Malformed drainage pit or pipe block."));
            };
            if kind == "pipe" {
                pipes += 1;
            } else {
                pits.push(fields(body)?);
            }
        } else if (matches!(kind.as_str(), "data" | "colour" | "style")
            || TRANSFORMS.contains(&kind.as_str()))
            && props.insert(kind, value).is_some()
        {
            return Err(unsupported("Repeated drainage geometry field."));
        }
    }
    if TRANSFORMS.iter().any(|key| props.contains_key(*key)) {
        return Err(unsupported("Drainage source transform is not supported."));
    }
    let values = if matches!(props.get("data"), Some(Item::Block(_))) {
        block_values(props.get("data").copied())?
    } else {
        vec![]
    };
    if pipes == 0 {
        return Err(unsupported("Drainage contains no pipe spans."));
    }
    if pits.len() != pipes + 1 || values.len() < 10 || values.len() % 5 != 0 {
        return Err(unsupported(
            "Drainage has no verified sequence of pits, pipes and solved vertices.",
        ));
    }
    let null = source.defaults["null"].as_str();
    let mut points = Vec::new();
    for data in values.chunks_exact(5) {
        checkpoint(cancel)?;
        if data[..2].contains(&null) {
            return Err(unsupported(
                "Drainage has a missing solved plan coordinate.",
            ));
        }
        if number(Some(data[3]), "drainage segment radius")? != 0.0 {
            return Err(unsupported("Curved drainage spans are not supported."));
        }
        points.push([
            number(Some(data[0]), "drainage X")?,
            number(Some(data[1]), "drainage Y")?,
        ]);
    }
    let mut indices = Vec::new();
    for pit in pits {
        let raw = get(&pit, "ip", "")?;
        let index_error = || unsupported("Drainage pit has an invalid solved vertex index.");
        if raw.len() > points.len().to_string().len()
            || !raw.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(index_error());
        }
        let index = raw.parse::<usize>().map_err(|_| index_error())?;
        if index >= points.len() || indices.last().is_some_and(|&previous| index <= previous) {
            return Err(unsupported(
                "Drainage pit indices do not follow the solved vertex order.",
            ));
        }
        let (x, y) = (scalar(&pit, "x", None)?, scalar(&pit, "y", None)?);
        if x == Some(null)
            || y == Some(null)
            || distance(points[index], [number(x, "pit X")?, number(y, "pit Y")?]) > 1e-6
        {
            return Err(unsupported(
                "Drainage pit coordinates disagree with the solved vertices.",
            ));
        }
        indices.push(index);
    }
    if indices[0] != 0 || indices[indices.len() - 1] != points.len() - 1 {
        return Err(unsupported(
            "Drainage pit indices do not cover the solved vertices.",
        ));
    }
    let colour = get(&props, "colour", &source.defaults["colour"])?;
    let style = get(&props, "style", &source.defaults["style"])?;
    let mut spans = Vec::new();
    for pair in indices.windows(2) {
        checkpoint(cancel)?;
        let span = &points[pair[0]..=pair[1]];
        if span.iter().all(|point| *point == span[0]) {
            return Err(unsupported("Drainage pipe span collapses in plan."));
        }
        spans.push(Primitive::new(Kind::Polyline, span.to_vec(), colour, style));
    }
    warn(
        "DRAINAGE_PLAN_ONLY",
        &format!(
            "Exported {pipes} solved pipe spans in plan. Pit structures, pipe dimensions, elevations and hydraulic attributes were omitted."
        ),
    );
    Ok(spans)
}
