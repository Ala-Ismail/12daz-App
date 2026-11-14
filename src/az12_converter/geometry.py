"""Plan geometry, independent of CAD and GUI libraries."""
from dataclasses import dataclass, field
import math

from .errors import ConversionError
from .parser import Block, fields, scalar, block_values


class UnsupportedGeometry(ValueError):
    pass


@dataclass
class Primitive:
    kind: str
    points: list[tuple[float, float]] = field(default_factory=list)
    bulges: list[float] = field(default_factory=list)
    closed: bool = False
    radius: float = 0
    text: str = ""
    height: float = 1
    rotation: float = 0
    justify: str = "bottom-left"
    width: float = 1
    slant: float = 0
    colour: str = "red"
    style: str = "1"


def number(value, name="number"):
    try:
        result = float(value)
    except (ValueError, TypeError) as exc:
        raise UnsupportedGeometry(f"Invalid {name}: {value!r}") from exc
    if not math.isfinite(result):
        raise UnsupportedGeometry(f"Non-finite {name}.")
    return result


def flag(value):
    value = str(value).lower()
    if value == "1" or value.startswith(("t", "y")):
        return True
    if value == "0" or value.startswith(("f", "n")):
        return False
    raise UnsupportedGeometry(f"Invalid boolean: {value!r}")


def radius_bulge(a, b, radius, major=False):
    """12d positive radius bulges left of the directed chord (DXF clockwise)."""
    if radius == 0:
        return 0.0
    chord = math.dist(a, b)
    if chord == 0:
        raise UnsupportedGeometry("Coincident arc endpoints do not define a unique arc.")
    ratio = chord / (2 * abs(radius))
    if ratio > 1 + 1e-8:
        raise UnsupportedGeometry("Arc chord exceeds its diameter.")
    angle = 2 * math.asin(min(1.0, ratio))
    if major:
        angle = 2 * math.pi - angle
    return -math.copysign(math.tan(angle / 4), radius)


def arc_parameters(a, b, bulge):
    chord = math.dist(a, b)
    if not bulge or not chord:
        raise ValueError("Not an arc")
    mx, my = (a[0] + b[0]) / 2, (a[1] + b[1]) / 2
    factor = (1 - bulge * bulge) / (4 * bulge)
    centre = (mx - (b[1] - a[1]) * factor, my + (b[0] - a[0]) * factor)
    return centre, chord * (1 + bulge * bulge) / (4 * abs(bulge)), math.atan2(a[1] - centre[1], a[0] - centre[0]), 4 * math.atan(bulge)


def preview_points(primitive, segments=32):
    """Display-only sampling. Export retains exact bulges/circles."""
    if primitive.kind == "circle":
        x, y = primitive.points[0]
        return [(x + primitive.radius * math.cos(i * math.tau / 64), y + primitive.radius * math.sin(i * math.tau / 64)) for i in range(65)]
    if primitive.kind in {"point", "text"}:
        return primitive.points
    result = []
    pts = primitive.points
    edges = len(pts) if primitive.closed else len(pts) - 1
    for i in range(max(0, edges)):
        a, b = pts[i], pts[(i + 1) % len(pts)]
        bulge = primitive.bulges[i] if i < len(primitive.bulges) else 0
        result.append(a)
        if bulge:
            centre, radius, start, sweep = arc_parameters(a, b, bulge)
            steps = max(2, math.ceil(abs(sweep) / math.tau * segments))
            result.extend((centre[0] + radius * math.cos(start + sweep * j / steps), centre[1] + radius * math.sin(start + sweep * j / steps)) for j in range(1, steps))
    if pts:
        result.append(pts[0] if primitive.closed else pts[-1])
    return result


def convert_geometry(source, warn):
    if source.kind not in {"super_alignment", "super", "2d", "3d", "polyline", "arc", "circle", "text"}:
        raise UnsupportedGeometry(f"Unsupported source type: {source.kind}")
    props = fields(source.block)
    if source.kind == "super_alignment":
        horizontal = props.get("horizontal_data")
        if not isinstance(horizontal, Block):
            raise UnsupportedGeometry("Alignment has no solved horizontal_data block.")
        props = {**props, **fields(horizontal)}
        warn("ALIGNMENT_PLAN_ONLY", "Solved horizontal geometry only; labels, vertical design and civil semantics are omitted.")

    colour = scalar(props, "colour", source.defaults["colour"])
    style = scalar(props, "style", source.defaults["style"])
    common = {"colour": colour, "style": style}
    if any(key in props for key in ("normal", "extrusion", "plane", "transform", "matrix")):
        raise UnsupportedGeometry("Nonstandard source plane/transform is not implemented.")
    recognized = {"model", "name", "id", "uid", "chainage", "breakline", "colour", "style", "closed", "z", "x", "y", "text", "angle", "offset", "raise", "textstyle", "slant", "xfactor", "x_factor", "worldsize", "papersize", "screensize", "justify", "radius", "xcentre", "ycentre", "zcentre", "xstart", "ystart", "zstart", "xend", "yend", "zend", "data", "data_2d", "data_3d", "radius_data", "major_data", "geometry_data", "colour_data", "time_created", "time_updated", "interval", "attributes", "drawables", "vertex_text_data", "vertex_annotate_data", "symbol_data", "pipe_value", "diameter_data", "culvert_data", "horizontal_data", "horizontal_parts", "vertical_data", "vertical_parts", "geometry_modifiers"}
    for key in props.keys() - recognized:
        warn("OMITTED_PROPERTY", f"Source property {key!r} is not exported.")
    for key in ("drawables", "attributes", "vertex_text_data", "vertex_annotate_data", "symbol_data", "pipe_value", "diameter_data", "culvert_data"):
        value = props.get(key)
        if value is not None and (not isinstance(value, Block) or value.items or key in {"drawables", "attributes"}):
            warn("OMITTED_" + key.upper(), f"{key} is not exported; output contains base plan geometry.")
    if source.kind == "text":
        if any(number(scalar(props, key, 0), key) != 0 for key in ("offset", "raise")):
            raise UnsupportedGeometry("Text offset/raise placement is not implemented.")
        height = number(scalar(props, "worldsize"), "worldsize")
        width = number(scalar(props, "xfactor", scalar(props, "x_factor", 1)), "text width")
        if height <= 0 or width <= 0:
            raise UnsupportedGeometry("Text height and width must be positive.")
        justify = scalar(props, "justify", "bottom-left").lower()
        if justify not in {f"{v}-{h}" for v in ("top", "middle", "bottom") for h in ("left", "centre", "right")}:
            raise UnsupportedGeometry("Unsupported text justification.")
        if scalar(props, "textstyle", "standard").lower() != "standard":
            warn("FONT_SUBSTITUTION", "Source textstyle replaced by the standard CAD font.")
        text = scalar(props, "text", "")
        if not text or any(c in text for c in "\n\r"):
            raise UnsupportedGeometry("Empty or multiline text is not implemented.")
        return [Primitive("text", [(number(scalar(props, "x")), number(scalar(props, "y")))], text=text, height=height,
                          width=width, justify=justify, slant=number(scalar(props, "slant", 0)), rotation=number(scalar(props, "angle", 0)), **common)]
    if source.kind in {"arc", "circle"}:
        centre = (number(scalar(props, "xcentre")), number(scalar(props, "ycentre")))
        r = number(scalar(props, "radius"), "radius")
        if r == 0:
            raise UnsupportedGeometry("Zero circle/arc radius.")
        if source.kind == "circle":
            if r < 0:
                raise UnsupportedGeometry("Circle radius must be positive.")
            return [Primitive("circle", [centre], radius=r, **common)]
        a = (number(scalar(props, "xstart")), number(scalar(props, "ystart")))
        b = (number(scalar(props, "xend")), number(scalar(props, "yend")))
        for p in (a, b):
            if abs(math.dist(p, centre) - abs(r)) > max(1e-7, abs(r) * 1e-8):
                raise UnsupportedGeometry("Arc endpoint does not match centre/radius.")
        start, end = (math.atan2(p[1] - centre[1], p[0] - centre[0]) for p in (a, b))
        sweep = -((start - end) % math.tau) if r > 0 else (end - start) % math.tau
        if not sweep:
            raise UnsupportedGeometry("Coincident arc endpoints; use a circle source.")
        return [Primitive("polyline", [a, b], [math.tan(sweep / 4), 0], **common)]

    coordinate_blocks = [key for key in ("data_3d", "data_2d", "data") if key in props]
    if len(coordinate_blocks) > 1:
        raise UnsupportedGeometry("Ambiguous vertex data blocks.")
    if "geometry_data" in props and "radius_data" in props:
        raise UnsupportedGeometry("Conflicting segment geometry definitions.")
    raw = props.get("data_3d") or props.get("data_2d") or props.get("data")
    if raw is None:
        raise UnsupportedGeometry("No supported vertex data block.")
    values = block_values(raw)
    legacy_curve_data = source.kind == "polyline" or (source.kind == "super" and "data" in props)
    stride = 2 if "data_2d" in props or source.kind == "2d" else (5 if legacy_curve_data else 3)
    if len(values) % stride:
        raise UnsupportedGeometry("Vertex data length does not match source dimensionality.")
    points = [(number(values[i], "X"), number(values[i + 1], "Y")) for i in range(0, len(values), stride)]
    # Null Z is allowed; malformed/non-finite non-null Z is not.
    if stride >= 3:
        for i in range(2, len(values), stride):
            if values[i].lower() != "null" and values[i] != source.defaults["null"]:
                number(values[i], "Z")
    if not points:
        raise UnsupportedGeometry("Empty vertex data.")
    closed = flag(scalar(props, "closed", "0"))
    if source.kind in {"2d", "3d"} and len(points) > 2 and points[0] == points[-1]:
        closed = True
    breakline = scalar(props, "breakline", source.defaults["breakline"]).lower()
    if breakline not in {"point", "line"}:
        raise UnsupportedGeometry("Invalid breakline type.")
    nsegments = len(points) if closed else len(points) - 1
    radii, majors = [0.] * len(points), [False] * len(points)
    if legacy_curve_data:
        radii = [number(values[i + 3], "radius") for i in range(0, len(values), 5)]
        majors = [flag(values[i + 4]) for i in range(0, len(values), 5)]
    elif "geometry_data" in props:
        data = props["geometry_data"]
        if not isinstance(data, Block) or len(data.items) % 2:
            raise UnsupportedGeometry("Malformed segment geometry.")
        segments = list(zip(data.items[::2], data.items[1::2]))
        if len(segments) not in {nsegments, len(points)}:
            raise UnsupportedGeometry("Segment geometry count mismatch.")
        for i, (kind, body) in enumerate(segments):
            if not hasattr(kind, "value") or not isinstance(body, Block):
                raise UnsupportedGeometry("Malformed segment geometry.")
            kind = kind.value.lower()
            if kind == "arc":
                params = fields(body)
                radii[i] = number(scalar(params, "radius"), "radius")
                if not radii[i]:
                    raise UnsupportedGeometry("Arc segment radius cannot be zero.")
                majors[i] = flag(scalar(params, "major", "0"))
            elif kind != "straight":
                raise UnsupportedGeometry(f"Unsupported {kind} curve; object skipped rather than replacing it by a false straight.")
    elif "radius_data" in props:
        radii = [number(v, "radius") for v in block_values(props["radius_data"])]
        majors = [flag(v) for v in block_values(props["major_data"])] if "major_data" in props else [False] * len(radii)
        if len(radii) not in {nsegments, len(points)} or len(majors) != len(radii):
            raise UnsupportedGeometry("Radius/major count mismatch.")
    bulges = [0.] * len(points)
    for i in range(max(0, nsegments)):
        if points[i] == points[(i + 1) % len(points)]:
            warn("DEGENERATE_SEGMENT", "Repeated XY vertices create a collapsed segment.")
        bulges[i] = radius_bulge(points[i], points[(i + 1) % len(points)], radii[i], majors[i])
    if breakline == "point" or len(set(points)) == 1:
        if breakline != "point":
            warn("COLLAPSED_TO_POINT", "All XY vertices coincide; emitted as a point.")
        return [Primitive("point", [p], **common) for p in dict.fromkeys(points)]
    colours = block_values(props["colour_data"]) if "colour_data" in props else None
    if colours:
        if len(colours) not in {nsegments, len(points)}:
            raise UnsupportedGeometry("Segment colour count mismatch.")
        if len(set(colours[:nsegments])) > 1 or colours[0] != colour:
            return [Primitive("polyline", [points[i], points[(i + 1) % len(points)]], [bulges[i], 0], colour=colours[i], style=style) for i in range(nsegments)]
    return [Primitive("polyline", points, bulges, closed, **common)]
