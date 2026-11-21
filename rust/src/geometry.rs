//! Plan geometry independent of CAD and the user interface.

use std::collections::HashSet;
use std::f64::consts::TAU;
use std::sync::atomic::AtomicBool;

use crate::error::{ConversionError, checkpoint};
use crate::parser::{Fields, Item, SourceEntity, block_values, fields, scalar};

mod alignment;
mod drainage;

pub type Point = [f64; 2];
pub type Warning<'a> = dyn FnMut(&str, &str) + 'a;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Point,
    Circle,
    Polyline,
    Text,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Primitive {
    pub kind: Kind,
    pub points: Vec<Point>,
    pub bulges: Vec<f64>,
    pub closed: bool,
    pub radius: f64,
    pub text: String,
    pub height: f64,
    pub rotation: f64,
    pub justify: String,
    pub width: f64,
    pub slant: f64,
    pub colour: String,
    pub style: String,
    pub elevations: Vec<f64>,
}

impl Primitive {
    pub fn new(kind: Kind, points: Vec<Point>, colour: &str, style: &str) -> Self {
        Self {
            kind,
            points,
            bulges: Vec::new(),
            closed: false,
            radius: 0.0,
            text: String::new(),
            height: 1.0,
            rotation: 0.0,
            justify: "bottom-left".into(),
            width: 1.0,
            slant: 0.0,
            colour: colour.into(),
            style: style.into(),
            elevations: Vec::new(),
        }
    }

    fn edge_count(&self) -> usize {
        self.points.len().saturating_sub(usize::from(!self.closed))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GeometryOptions {
    pub keep_z: bool,
    pub plan_tolerance: f64,
    pub elevation_tolerance: f64,
    pub linear_arc_elevations: bool,
}

impl Default for GeometryOptions {
    fn default() -> Self {
        Self {
            keep_z: false,
            plan_tolerance: 0.01,
            elevation_tolerance: 0.01,
            linear_arc_elevations: false,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum GeometryError {
    Unsupported(String),
    Input(ConversionError),
}

impl From<ConversionError> for GeometryError {
    fn from(error: ConversionError) -> Self {
        Self::Input(error)
    }
}

type Result<T> = std::result::Result<T, GeometryError>;

fn unsupported(message: impl Into<String>) -> GeometryError {
    GeometryError::Unsupported(message.into())
}

fn number(value: Option<&str>, name: &str) -> Result<f64> {
    let parsed = value.and_then(|token| {
        let token = token.trim();
        if token.contains('_') {
            let bytes = token.as_bytes();
            if bytes.iter().enumerate().any(|(i, byte)| {
                *byte == b'_'
                    && (i == 0
                        || i + 1 == bytes.len()
                        || !bytes[i - 1].is_ascii_digit()
                        || !bytes[i + 1].is_ascii_digit())
            }) {
                return None;
            }
            return token.replace('_', "").parse::<f64>().ok();
        }
        token.parse::<f64>().ok()
    });
    match parsed {
        Some(number) if number.is_finite() => Ok(number),
        Some(_) => Err(unsupported(format!("Non-finite {name}."))),
        None => Err(unsupported(format!(
            "Invalid {name}: {}",
            value.map_or_else(|| "None".into(), repr)
        ))),
    }
}

pub(crate) fn repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut result = String::from(quote);
    for character in value.chars() {
        match character {
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c == quote => {
                result.push('\\');
                result.push(c);
            }
            c if c.is_control() && (c as u32) < 256 => {
                result.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => result.push(c),
        }
    }
    result.push(quote);
    result
}

fn flag(value: &str) -> Result<bool> {
    let value = value.to_lowercase();
    if value == "1" || value.starts_with(['t', 'y']) {
        return Ok(true);
    }
    if value == "0" || value.starts_with(['f', 'n']) {
        return Ok(false);
    }
    Err(unsupported(format!("Invalid boolean: {}", repr(&value))))
}

fn get<'a>(props: &Fields<'a>, key: &str, fallback: &'a str) -> Result<&'a str> {
    Ok(scalar(props, key, Some(fallback))?.unwrap_or(fallback))
}

fn numeric(props: &Fields<'_>, key: &str, fallback: Option<&str>, name: &str) -> Result<f64> {
    number(scalar(props, key, fallback)?, name)
}

fn distance(a: Point, b: Point) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

pub fn radius_bulge(a: Point, b: Point, radius: f64, major: bool) -> Result<f64> {
    if radius == 0.0 {
        return Ok(0.0);
    }
    let chord = distance(a, b);
    if chord == 0.0 {
        return Err(unsupported(
            "Coincident arc endpoints do not define a unique arc.",
        ));
    }
    let ratio = chord / (2.0 * radius.abs());
    if ratio > 1.0 + 1e-8 {
        return Err(unsupported("Arc chord exceeds its diameter."));
    }
    let minor = 2.0 * ratio.min(1.0).asin();
    let angle = if major { TAU - minor } else { minor };
    Ok(-(angle / 4.0).tan().copysign(radius))
}

#[derive(Clone, Copy, Debug)]
pub struct Arc {
    pub center: Point,
    pub radius: f64,
    pub start: f64,
    pub sweep: f64,
}

impl Arc {
    fn point(self, fraction: f64) -> Point {
        let angle = self.start + self.sweep * fraction;
        [
            self.center[0] + self.radius * angle.cos(),
            self.center[1] + self.radius * angle.sin(),
        ]
    }
}

pub fn arc_parameters(a: Point, b: Point, bulge: f64) -> Result<Arc> {
    let chord = distance(a, b);
    if bulge == 0.0 || chord == 0.0 {
        return Err(unsupported("Not an arc"));
    }
    let factor = (1.0 - bulge * bulge) / (4.0 * bulge);
    let center = [
        (a[0] + b[0]) / 2.0 - (b[1] - a[1]) * factor,
        (a[1] + b[1]) / 2.0 + (b[0] - a[0]) * factor,
    ];
    Ok(Arc {
        center,
        radius: chord * (1.0 + bulge * bulge) / (4.0 * bulge.abs()),
        start: (a[1] - center[1]).atan2(a[0] - center[0]),
        sweep: 4.0 * bulge.atan(),
    })
}

pub fn preview_points(primitive: &Primitive) -> Result<Vec<Point>> {
    match primitive.kind {
        Kind::Circle => {
            let circle = Arc {
                center: primitive.points[0],
                radius: primitive.radius,
                start: 0.0,
                sweep: TAU,
            };
            return Ok((0..=64)
                .map(|index| circle.point(index as f64 / 64.0))
                .collect());
        }
        Kind::Point | Kind::Text => return Ok(primitive.points.clone()),
        Kind::Polyline => {}
    }
    let mut sampled = Vec::new();
    for i in 0..primitive.edge_count() {
        let a = primitive.points[i];
        let b = primitive.points[(i + 1) % primitive.points.len()];
        sampled.push(a);
        let bulge = primitive.bulges.get(i).copied().unwrap_or_default();
        if bulge != 0.0 {
            let arc = arc_parameters(a, b, bulge)?;
            let steps = ((arc.sweep.abs() / TAU * 32.0).ceil() as usize).max(2);
            sampled.extend((1..steps).map(|step| arc.point(step as f64 / steps as f64)));
        }
    }
    let last = if primitive.closed {
        primitive.points.first()
    } else {
        primitive.points.last()
    };
    sampled.extend(last.copied());
    Ok(sampled)
}

fn sample_elevated_string(
    primitive: Primitive,
    colours: &[&str],
    tolerance: f64,
    warn: &mut Warning<'_>,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<Primitive>> {
    let mut runs: Vec<Primitive> = Vec::new();
    let mut added = 0;
    let mut previous_key = None;
    for i in 0..primitive.edge_count() {
        checkpoint(cancel)?;
        let j = (i + 1) % primitive.points.len();
        let (a, b) = (primitive.points[i], primitive.points[j]);
        let (za, zb) = (primitive.elevations[i], primitive.elevations[j]);
        let mut bulge = primitive.bulges[i];
        let colour = colours.get(i).copied().unwrap_or(&primitive.colour);
        let level_arc = bulge != 0.0 && za == zb;
        let key = (colour, level_arc.then_some(za));
        if previous_key != Some(key) {
            let mut run = Primitive::new(Kind::Polyline, vec![a], colour, &primitive.style);
            run.elevations.push(za);
            runs.push(run);
            previous_key = Some(key);
        }
        let run = runs.last_mut().expect("the first edge begins a run");
        if bulge != 0.0 && !level_arc {
            let arc = arc_parameters(a, b, bulge)?;
            // Maximum chord error is bounded by radius * angle_step² / 8.
            let needed = arc.sweep.abs() * (arc.radius / 8.0 / tolerance).sqrt();
            let limit_error = || {
                unsupported(
                    "Elevated arc sampling exceeds the vertex limit at the requested tolerance.",
                )
            };
            if !needed.is_finite() || needed > (100_001 - added) as f64 {
                return Err(limit_error());
            }
            let steps = (needed.ceil() as usize).max(2);
            added += steps - 1;
            if added > 100_000 {
                return Err(limit_error());
            }
            for step in 1..steps {
                checkpoint(cancel)?;
                let fraction = step as f64 / steps as f64;
                run.points.push(arc.point(fraction));
                run.elevations.push(za * (1.0 - fraction) + zb * fraction);
                run.bulges.push(0.0);
            }
            bulge = 0.0;
        }
        run.points.push(b);
        run.elevations.push(zb);
        run.bulges.push(bulge);
    }
    if let [run] = runs.as_mut_slice()
        && primitive.closed
    {
        run.closed = true;
        run.points.pop();
        run.elevations.pop();
    }
    if added != 0 {
        warn(
            "ELEVATED_ARC_SAMPLED",
            &format!(
                "Added {added} vertices at plan tolerance {tolerance} drawing units using a chord error bound. Elevations assume a linear grade between source endpoints along each arc."
            ),
        );
    }
    Ok(runs)
}

const TRANSFORMS: [&str; 5] = ["normal", "extrusion", "plane", "transform", "matrix"];
const OMITTED: [&str; 8] = [
    "drawables",
    "attributes",
    "vertex_text_data",
    "vertex_annotate_data",
    "symbol_data",
    "pipe_value",
    "diameter_data",
    "culvert_data",
];

fn report_properties(props: &Fields<'_>, warn: &mut Warning<'_>) {
    const KNOWN: &[&str] = &[
        "model",
        "name",
        "id",
        "uid",
        "chainage",
        "breakline",
        "colour",
        "style",
        "closed",
        "z",
        "x",
        "y",
        "text",
        "angle",
        "offset",
        "raise",
        "textstyle",
        "slant",
        "xfactor",
        "x_factor",
        "worldsize",
        "papersize",
        "screensize",
        "justify",
        "radius",
        "xcentre",
        "ycentre",
        "zcentre",
        "xstart",
        "ystart",
        "zstart",
        "xend",
        "yend",
        "zend",
        "data",
        "data_2d",
        "data_3d",
        "radius_data",
        "major_data",
        "geometry_data",
        "colour_data",
        "time_created",
        "time_updated",
        "interval",
        "horizontal_data",
        "horizontal_parts",
        "vertical_data",
        "vertical_parts",
        "geometry_modifiers",
    ];
    let mut unrecognized = props
        .keys()
        .filter(|key| !KNOWN.contains(&key.as_str()) && !OMITTED.contains(&key.as_str()))
        .collect::<Vec<_>>();
    unrecognized.sort_unstable();
    for key in unrecognized {
        warn(
            "OMITTED_PROPERTY",
            &format!("Source property {} is not exported.", repr(key)),
        );
    }
    for key in OMITTED {
        if let Some(value) = props.get(key)
            && (!matches!(value, Item::Block(block) if block.items.is_empty())
                || matches!(key, "drawables" | "attributes"))
        {
            warn(
                &format!("OMITTED_{}", key.to_ascii_uppercase()),
                &format!("{key} is not exported; output contains base plan geometry."),
            );
        }
    }
}

struct Geometry<'a> {
    props: Fields<'a>,
    style: &'a str,
    colour: &'a str,
    null: &'a str,
    defaults: &'a std::collections::HashMap<String, String>,
    options: GeometryOptions,
    cancel: Option<&'a AtomicBool>,
}

impl Geometry<'_> {
    fn primitive(&self, kind: Kind, points: Vec<Point>) -> Primitive {
        Primitive::new(kind, points, self.colour, self.style)
    }

    fn elevation(&self, value: Option<&str>) -> Result<f64> {
        if value.is_none_or(|value| value.eq_ignore_ascii_case("null") || value == self.null) {
            return Err(unsupported("Keep Z requires a known source elevation."));
        }
        number(value, "Z")
    }

    fn text(&self, warn: &mut Warning<'_>) -> Result<Vec<Primitive>> {
        let props = &self.props;
        if numeric(props, "offset", Some("0"), "offset")? != 0.0
            || numeric(props, "raise", Some("0"), "raise")? != 0.0
        {
            return Err(unsupported(
                "Text offset/raise placement is not implemented.",
            ));
        }
        let height = numeric(props, "worldsize", None, "worldsize")?;
        let width = numeric(
            props,
            "xfactor",
            Some(get(props, "x_factor", "1")?),
            "text width",
        )?;
        if height <= 0.0 || width <= 0.0 {
            return Err(unsupported("Text height and width must be positive."));
        }
        let justify = get(props, "justify", "bottom-left")?.to_lowercase();
        if !matches!(
            justify.split_once('-'),
            Some(("top" | "middle" | "bottom", "left" | "centre" | "right"))
        ) {
            return Err(unsupported("Unsupported text justification."));
        }
        if !get(props, "textstyle", "standard")?.eq_ignore_ascii_case("standard") {
            warn(
                "FONT_SUBSTITUTION",
                "Source textstyle replaced by the standard CAD font.",
            );
        }
        let text = get(props, "text", "")?;
        if text.is_empty() || text.contains(['\n', '\r']) {
            return Err(unsupported("Empty or multiline text is not implemented."));
        }
        let elevations = if self.options.keep_z {
            vec![self.elevation(scalar(props, "z", None)?)?]
        } else {
            vec![]
        };
        let position = [
            numeric(props, "x", None, "number")?,
            numeric(props, "y", None, "number")?,
        ];
        let mut label = self.primitive(Kind::Text, vec![position]);
        label.text = text.into();
        label.height = height;
        label.width = width;
        label.justify = justify;
        label.slant = numeric(props, "slant", Some("0"), "number")?;
        label.rotation = numeric(props, "angle", Some("0"), "number")?;
        label.elevations = elevations;
        Ok(vec![label])
    }

    fn circular(&self, kind: &str, warn: &mut Warning<'_>) -> Result<Vec<Primitive>> {
        let props = &self.props;
        let xy = |x, y| -> Result<Point> {
            Ok([
                numeric(props, x, None, "number")?,
                numeric(props, y, None, "number")?,
            ])
        };
        let center = xy("xcentre", "ycentre")?;
        let radius = numeric(props, "radius", None, "radius")?;
        if radius == 0.0 {
            return Err(unsupported("Zero circle/arc radius."));
        }
        if kind == "circle" {
            if radius < 0.0 {
                return Err(unsupported("Circle radius must be positive."));
            }
            let mut circle = self.primitive(Kind::Circle, vec![center]);
            circle.radius = radius;
            if self.options.keep_z {
                circle
                    .elevations
                    .push(self.elevation(scalar(props, "zcentre", None)?)?);
            }
            return Ok(vec![circle]);
        }
        let a = xy("xstart", "ystart")?;
        let b = xy("xend", "yend")?;
        let absolute_radius = radius.abs();
        if [a, b].iter().any(|&point| {
            (distance(point, center) - absolute_radius).abs() > 1e-7_f64.max(absolute_radius * 1e-8)
        }) {
            return Err(unsupported("Arc endpoint does not match centre/radius."));
        }
        let angle = |point: Point| (point[1] - center[1]).atan2(point[0] - center[0]);
        let (start, end) = (angle(a), angle(b));
        let sweep = if radius > 0.0 {
            -(start - end).rem_euclid(TAU)
        } else {
            (end - start).rem_euclid(TAU)
        };
        if sweep == 0.0 {
            return Err(unsupported(
                "Coincident arc endpoints; use a circle source.",
            ));
        }
        let mut path = self.primitive(Kind::Polyline, vec![a, b]);
        path.bulges = vec![(sweep / 4.0).tan(), 0.0];
        if self.options.keep_z {
            let center_z = self.elevation(scalar(props, "zcentre", None)?)?;
            let a_z = self.elevation(scalar(props, "zstart", None)?)?;
            let b_z = self.elevation(scalar(props, "zend", None)?)?;
            path.elevations = vec![a_z, b_z];
            if center_z != a_z || a_z != b_z {
                if !self.options.linear_arc_elevations || a_z == b_z {
                    return Err(unsupported(
                        "Keep Z requires a level arc or the linear arc elevation option with differing endpoint elevations.",
                    ));
                }
                return sample_elevated_string(
                    path,
                    &[],
                    self.options.plan_tolerance,
                    warn,
                    self.cancel,
                );
            }
        }
        Ok(vec![path])
    }
}

fn bits(value: f64) -> u64 {
    if value == 0.0 { 0 } else { value.to_bits() }
}

fn position_key(point: Point, z: f64) -> [u64; 3] {
    [bits(point[0]), bits(point[1]), bits(z)]
}

impl Geometry<'_> {
    fn segment_radii(
        &self,
        values: &[&str],
        legacy: bool,
        stride: usize,
        edges: usize,
        points: usize,
    ) -> Result<(Vec<f64>, Vec<bool>)> {
        let props = &self.props;
        let mut radii = vec![0.0; points];
        let mut majors = vec![false; points];
        if legacy && stride == 5 {
            for (i, data) in values.chunks_exact(5).enumerate() {
                radii[i] = number(Some(data[3]), "radius")?;
                majors[i] = flag(data[4])?;
            }
        } else if let Some(geometry) = props.get("geometry_data") {
            let Item::Block(block) = geometry else {
                return Err(unsupported("Malformed segment geometry."));
            };
            let count = block.items.len() / 2;
            if block.items.len() % 2 != 0 {
                return Err(unsupported("Malformed segment geometry."));
            }
            if count != edges && count != points {
                return Err(unsupported("Segment geometry count mismatch."));
            }
            for (i, pair) in block.items.chunks_exact(2).enumerate() {
                let [Item::Token(kind), Item::Block(body)] = pair else {
                    return Err(unsupported("Malformed segment geometry."));
                };
                let kind = kind.value.to_lowercase();
                match kind.as_str() {
                    "arc" => {
                        let options = fields(body)?;
                        radii[i] = numeric(&options, "radius", None, "radius")?;
                        if radii[i] == 0.0 {
                            return Err(unsupported("Arc segment radius cannot be zero."));
                        }
                        majors[i] = flag(get(&options, "major", "0")?)?;
                    }
                    "straight" => {}
                    _ => {
                        return Err(unsupported(format!(
                            "Unsupported {kind} curve; object skipped rather than replacing it by a false straight."
                        )));
                    }
                }
            }
        } else if let Some(radius_data) = props.get("radius_data") {
            radii = block_values(Some(radius_data))?
                .into_iter()
                .map(|token| number(Some(token), "radius"))
                .collect::<Result<_>>()?;
            majors = match props.get("major_data") {
                Some(data) => block_values(Some(data))?
                    .into_iter()
                    .map(flag)
                    .collect::<Result<_>>()?,
                None => vec![false; radii.len()],
            };
            if (radii.len() != edges && radii.len() != points) || majors.len() != radii.len() {
                return Err(unsupported("Radius/major count mismatch."));
            }
        }
        Ok((radii, majors))
    }

    fn string(&self, kind: &str, warn: &mut Warning<'_>) -> Result<Vec<Primitive>> {
        let props = &self.props;
        let mut coordinate_blocks = ["data_3d", "data_2d", "data"]
            .into_iter()
            .filter_map(|key| props.get(key));
        let raw = coordinate_blocks.next();
        if coordinate_blocks.next().is_some() {
            return Err(unsupported("Ambiguous vertex data blocks."));
        }
        if props.contains_key("geometry_data") && props.contains_key("radius_data") {
            return Err(unsupported("Conflicting segment geometry definitions."));
        }
        let raw = raw.ok_or_else(|| unsupported("No supported vertex data block."))?;
        let values = block_values(Some(raw))?;
        let legacy = kind == "polyline" || (kind == "super" && props.contains_key("data"));
        let stride = if props.contains_key("data_2d") || kind == "2d" {
            2
        } else if legacy {
            5
        } else {
            3
        };
        if values.len() % stride != 0 {
            return Err(unsupported(
                "Vertex data length does not match source dimensionality.",
            ));
        }
        let points = values
            .chunks_exact(stride)
            .map(|values| Ok([number(Some(values[0]), "X")?, number(Some(values[1]), "Y")?]))
            .collect::<Result<Vec<_>>>()?;
        if stride >= 3 {
            for data in values.chunks_exact(stride) {
                if !data[2].eq_ignore_ascii_case("null") && data[2] != self.null {
                    number(Some(data[2]), "Z")?;
                }
            }
        }
        if points.is_empty() {
            return Err(unsupported("Empty vertex data."));
        }
        let levels = match (self.options.keep_z, stride) {
            (false, _) => Vec::new(),
            (true, 2) => vec![self.elevation(scalar(props, "z", None)?)?; points.len()],
            (true, _) => values
                .chunks_exact(stride)
                .map(|data| self.elevation(Some(data[2])))
                .collect::<Result<_>>()?,
        };
        let level = |i| levels.get(i).copied().unwrap_or_default();
        let mut closed = flag(get(props, "closed", "0")?)?;
        if matches!(kind, "2d" | "3d")
            && points.len() > 2
            && points.first() == points.last()
            && levels.first() == levels.last()
        {
            closed = true;
        }
        let breakline = get(props, "breakline", &self.defaults["breakline"])?.to_lowercase();
        if !matches!(breakline.as_str(), "point" | "line") {
            return Err(unsupported("Invalid breakline type."));
        }
        let edges = points.len() - usize::from(!closed);
        let (radii, majors) = self.segment_radii(&values, legacy, stride, edges, points.len())?;
        let mut bulges = vec![0.0; points.len()];
        for i in 0..edges {
            checkpoint(self.cancel)?;
            let j = (i + 1) % points.len();
            if points[i] == points[j] && level(i) == level(j) {
                warn(
                    "DEGENERATE_SEGMENT",
                    "Repeated vertices create a collapsed segment.",
                );
            }
            bulges[i] = radius_bulge(points[i], points[j], radii[i], majors[i])?;
        }
        if breakline == "point"
            || points
                .iter()
                .enumerate()
                .all(|(i, &point)| point == points[0] && level(i) == level(0))
        {
            if breakline != "point" {
                warn(
                    "COLLAPSED_TO_POINT",
                    "All XY vertices coincide; emitted as a point.",
                );
            }
            let mut seen = HashSet::new();
            return Ok(points
                .iter()
                .enumerate()
                .filter_map(|(i, &point)| {
                    if !seen.insert(position_key(point, level(i))) {
                        return None;
                    }
                    let mut primitive = self.primitive(Kind::Point, vec![point]);
                    primitive.elevations.extend(levels.get(i).copied());
                    Some(primitive)
                })
                .collect());
        }
        let colours = props
            .get("colour_data")
            .map(|value| block_values(Some(value)))
            .transpose()?
            .unwrap_or_default();
        if !colours.is_empty() && colours.len() != edges && colours.len() != points.len() {
            return Err(unsupported("Segment colour count mismatch."));
        }
        if levels.iter().any(|&z| z != levels[0]) && bulges.iter().any(|&bulge| bulge != 0.0) {
            if !self.options.linear_arc_elevations {
                return Err(unsupported(
                    "Enable linear arc elevations to export curved strings with varying elevations in Keep Z.",
                ));
            }
            let mut path = self.primitive(Kind::Polyline, points);
            path.bulges = bulges;
            path.closed = closed;
            path.elevations = levels;
            return sample_elevated_string(
                path,
                &colours,
                self.options.plan_tolerance,
                warn,
                self.cancel,
            );
        }
        if !colours.is_empty()
            && (colours[0] != self.colour
                || colours[..edges].iter().any(|colour| *colour != colours[0]))
        {
            return Ok((0..edges)
                .map(|i| {
                    let j = (i + 1) % points.len();
                    let mut primitive = Primitive::new(
                        Kind::Polyline,
                        vec![points[i], points[j]],
                        colours[i],
                        self.style,
                    );
                    primitive.bulges = vec![bulges[i], 0.0];
                    if !levels.is_empty() {
                        primitive.elevations = vec![levels[i], levels[j]];
                    }
                    primitive
                })
                .collect());
        }
        let mut path = self.primitive(Kind::Polyline, points);
        path.bulges = bulges;
        path.closed = closed;
        path.elevations = levels;
        Ok(vec![path])
    }
}

fn primitive_geometry(
    props: Fields<'_>,
    defaults: &std::collections::HashMap<String, String>,
    kind: &str,
    options: GeometryOptions,
    warn: &mut Warning<'_>,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<Primitive>> {
    let colour = get(&props, "colour", &defaults["colour"])?;
    let style = get(&props, "style", &defaults["style"])?;
    if TRANSFORMS.iter().any(|key| props.contains_key(*key)) {
        return Err(unsupported(
            "Nonstandard source plane/transform is not implemented.",
        ));
    }
    report_properties(&props, warn);
    let geometry = Geometry {
        props,
        style,
        colour,
        null: &defaults["null"],
        defaults,
        options,
        cancel,
    };
    match kind {
        "text" => geometry.text(warn),
        "circle" | "arc" => geometry.circular(kind, warn),
        _ => geometry.string(kind, warn),
    }
}

pub(crate) fn skipped_location(source: &SourceEntity) -> Option<Point> {
    let coordinates = match source.kind.as_str() {
        "arc" => Some(("xstart", "ystart")),
        "circle" => Some(("xcentre", "ycentre")),
        "text" => Some(("x", "y")),
        "super" | "2d" | "3d" | "polyline" => None,
        _ => return None,
    };
    let props = fields(&source.block).ok()?;
    if let Some((x, y)) = coordinates {
        return Some([
            numeric(&props, x, None, "X").ok()?,
            numeric(&props, y, None, "Y").ok()?,
        ]);
    }
    let mut blocks = ["data_2d", "data_3d", "data"]
        .into_iter()
        .filter_map(|key| props.get(key));
    let Item::Block(block) = blocks.next()? else {
        return None;
    };
    if blocks.next().is_some() {
        return None;
    }
    let [Item::Token(x), Item::Token(y), ..] = block.items.as_slice() else {
        return None;
    };
    Some([
        number(Some(&x.value), "X").ok()?,
        number(Some(&y.value), "Y").ok()?,
    ])
}

#[cfg(feature = "desktop")]
pub(crate) fn visit_source_xy(
    source: &SourceEntity,
    cancel: Option<&AtomicBool>,
    mut visit: impl FnMut(Point),
) -> crate::error::Result<()> {
    let Ok(mut props) = fields(&source.block) else {
        return Ok(());
    };
    if TRANSFORMS.iter().any(|name| props.contains_key(*name)) {
        return Ok(());
    }
    let null = source.defaults.get("null").map(String::as_str);
    let finite_pair = |x: &str, y: &str| {
        if Some(x) == null || Some(y) == null {
            return None;
        }
        Some([number(Some(x), "X").ok()?, number(Some(y), "Y").ok()?])
    };
    let named: &[(&str, &str)] = match source.kind.as_str() {
        "arc" => &[("xstart", "ystart"), ("xend", "yend")],
        "circle" => &[("xcentre", "ycentre")],
        "text" => &[("x", "y")],
        "super_alignment" => {
            let Some(Item::Block(horizontal)) = props.get("horizontal_data") else {
                return Ok(());
            };
            let Ok(solved) = fields(horizontal) else {
                return Ok(());
            };
            if TRANSFORMS.iter().any(|name| solved.contains_key(*name)) {
                return Ok(());
            }
            props = solved;
            &[]
        }
        "super" | "2d" | "3d" | "polyline" => &[],
        _ => return Ok(()),
    };
    for &(x, y) in named {
        if let (Ok(Some(x)), Ok(Some(y))) = (scalar(&props, x, None), scalar(&props, y, None))
            && let Some(point) = finite_pair(x, y)
        {
            visit(point);
        }
    }
    if !named.is_empty() {
        return Ok(());
    }
    let mut blocks = ["data_2d", "data_3d", "data"]
        .into_iter()
        .filter_map(|key| props.get(key).map(|item| (key, *item)));
    let Some((key, Item::Block(block))) = blocks.next() else {
        return Ok(());
    };
    let stride = if key == "data_2d" || source.kind == "2d" {
        2
    } else if source.kind == "polyline" || (source.kind == "super" && key == "data") {
        5
    } else {
        3
    };
    if blocks.next().is_some() || !block.items.len().is_multiple_of(stride) {
        return Ok(());
    }
    for (index, vertex) in block.items.chunks_exact(stride).enumerate() {
        if index % 512 == 0 {
            checkpoint(cancel)?;
        }
        if let [Item::Token(x), Item::Token(y), ..] = vertex
            && let Some(point) = finite_pair(&x.value, &y.value)
        {
            visit(point);
        }
    }
    Ok(())
}

pub fn convert_geometry(
    source: &SourceEntity,
    options: GeometryOptions,
    warn: &mut Warning<'_>,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<Primitive>> {
    match source.kind.as_str() {
        "drainage" => drainage::drainage_plan(source, options.keep_z, warn, cancel),
        "super_alignment" => alignment::convert_alignment(source, options, warn, cancel),
        kind @ ("super" | "2d" | "3d" | "polyline" | "arc" | "circle" | "text") => {
            primitive_geometry(
                fields(&source.block)?,
                &source.defaults,
                kind,
                options,
                warn,
                cancel,
            )
        }
        _ => Err(unsupported(format!(
            "Unsupported source type: {}",
            source.kind
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::Limits;
    use crate::parser::iter_entities;

    fn convert(text: &str, options: GeometryOptions) -> (Result<Vec<Primitive>>, Vec<String>) {
        let mut reader = iter_entities([Ok(text.into())], Limits::default(), None);
        let entity = reader.next_entity(&mut |_, _, _| {}).unwrap().unwrap();
        let mut warnings = Vec::new();
        let result = convert_geometry(
            &entity,
            options,
            &mut |code, _| warnings.push(code.into()),
            None,
        );
        (result, warnings)
    }

    #[test]
    fn rust_06_exact_geometry_and_signed_arcs() {
        for radius in [-10.0_f64, 10.0] {
            for major in [false, true] {
                let bulge = radius_bulge([0.0, 0.0], [10.0, 0.0], radius, major).unwrap();
                assert!(bulge * radius < 0.0);
                assert_eq!(bulge.abs() > 1.0, major);
                let arc = arc_parameters([0.0, 0.0], [10.0, 0.0], bulge).unwrap();
                assert!((arc.radius - 10.0).abs() < 1e-10);
                let expected_y = 75.0_f64
                    .sqrt()
                    .copysign(if major { radius } else { -radius });
                assert!(distance(arc.center, [5.0, expected_y]) < 1e-10);
                assert!(distance(arc.point(1.0), [10.0, 0.0]) < 1e-10);
            }
        }
        let (circle, warnings) = convert(
            "string circle { xcentre 1 ycentre 2 zcentre 44 radius 3 }",
            GeometryOptions::default(),
        );
        let circle = &circle.unwrap()[0];
        assert_eq!(
            (circle.kind, circle.points.as_slice(), circle.radius),
            (Kind::Circle, [[1.0, 2.0]].as_slice(), 3.0)
        );
        assert!(circle.elevations.is_empty() && warnings.is_empty());
        let label = convert(r#"string text { x 1 y 2 z 99 text "Café" worldsize 0.5 angle 30 justify "top-centre" }"#, GeometryOptions::default()).0.unwrap();
        assert_eq!(label[0].text, "Café");
        assert_eq!(label[0].justify, "top-centre");
        assert_eq!(label[0].rotation, 30.0);
    }

    #[test]
    fn rust_06_keep_z_sampling_and_plan_segment_colours() {
        let line = "string super { breakline line data_3d { 0 0 12 10 0 16 20 0 16 } radius_data { -10 -10 } colour_data { red blue } }";
        let flat = convert(line, GeometryOptions::default()).0.unwrap();
        assert_eq!(
            flat.iter()
                .map(|path| path.colour.as_str())
                .collect::<Vec<_>>(),
            ["red", "blue"]
        );
        assert!(
            flat.iter()
                .all(|path| path.bulges[0] > 0.0 && path.elevations.is_empty())
        );
        assert!(
            convert(
                line,
                GeometryOptions {
                    keep_z: true,
                    ..Default::default()
                }
            )
            .0
            .is_err()
        );
        let (result, warnings) = convert(
            line,
            GeometryOptions {
                keep_z: true,
                linear_arc_elevations: true,
                ..Default::default()
            },
        );
        let result = result.unwrap();
        assert_eq!(warnings, ["ELEVATED_ARC_SAMPLED"]);
        assert!(result[0].points.len() > 2 && result[0].bulges.iter().all(|bulge| *bulge == 0.0));
        assert_eq!(result[0].elevations.first(), Some(&12.0));
        assert_eq!(result[0].elevations.last(), Some(&16.0));
        assert_eq!(result[1].elevations, [16.0, 16.0]);
        assert!(result[1].bulges[0] > 0.0);
    }

    #[test]
    fn rust_06_alignment_reconstruction_and_vertical_parabola() {
        let ip = "string super_alignment { horizontal_ips_only true valid_horizontal true closed true horizontal_parts { ip { x 0 y 0 } ip { x 10 y 0 } ip { x 10 y 10 } } }";
        let (result, warnings) = convert(ip, GeometryOptions::default());
        let result = result.unwrap();
        assert_eq!(
            warnings,
            ["ALIGNMENT_IP_RECONSTRUCTED", "ALIGNMENT_PLAN_ONLY"]
        );
        assert!(result[0].closed);
        assert_eq!(result[0].points, [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        let tangent = "string super_alignment { valid_horizontal true horizontal_parts { fixed_line { x1 0 y1 0 x2 10 y2 0 } free_arc_radius { r -2 } fixed_line { x1 10 y1 0 x2 10 y2 10 } } }";
        let (result, warnings) = convert(tangent, GeometryOptions::default());
        let result = result.unwrap();
        assert_eq!(
            warnings,
            ["ALIGNMENT_TANGENT_ARC_RECONSTRUCTED", "ALIGNMENT_PLAN_ONLY"]
        );
        for (&found, expected) in
            result[0]
                .points
                .iter()
                .zip([[0.0, 0.0], [8.0, 0.0], [10.0, 2.0], [10.0, 10.0]])
        {
            assert!(distance(found, expected) < 1e-10);
        }
        let profile = "string super_alignment { horizontal_data { breakline line data_2d { 0 0 20 0 } } vertical_data { data_2d { 0 10 20 10 } geometry_data { parabola { chainage 10 height 20 } } } }";
        let (result, warnings) = convert(
            profile,
            GeometryOptions {
                keep_z: true,
                ..Default::default()
            },
        );
        let result = result.unwrap();
        assert_eq!(warnings, ["ALIGNMENT_SAMPLED"]);
        assert!(result[0].points.len() > 3);
        for (point, z) in result[0].points.iter().zip(&result[0].elevations) {
            let x = point[0];
            assert!((z - (10.0 + x - 0.05 * x * x)).abs() < 1e-10);
        }
        assert!(result[0].points.contains(&[10.0, 0.0]));
    }

    #[test]
    fn rust_06_drainage_uses_verified_pit_spans() {
        let drainage = "string drainage { pit { ip 0 x 0 y 0 } pipe {} pit_v2 { ip 1 x 1 y 0 } pipe {} pit { ip 3 x 3 y 2 } data { 0 0 0 0 F 1 0 0 0 F 2 0 0 0 F 3 2 0 0 F } }";
        let (result, warnings) = convert(drainage, GeometryOptions::default());
        let result = result.unwrap();
        assert_eq!(warnings, ["DRAINAGE_PLAN_ONLY"]);
        assert_eq!(result[0].points, [[0.0, 0.0], [1.0, 0.0]]);
        assert_eq!(result[1].points, [[1.0, 0.0], [2.0, 0.0], [3.0, 2.0]]);
        assert!(
            convert(
                drainage,
                GeometryOptions {
                    keep_z: true,
                    ..Default::default()
                }
            )
            .0
            .is_err()
        );
    }
}
