"""Plan geometry, independent of CAD and GUI libraries."""
from dataclasses import dataclass, field, replace
import math
from bisect import bisect_right

from .errors import ConversionError, checkpoint
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
    elevations: list[float] = field(default_factory=list)


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


def alignment_elevations(primitives, profile, start, null_value, warn, cancel, plan_tolerance, elevation_tolerance):
    """Combine solved plan geometry and a chainage/elevation profile."""
    props = fields(profile)
    raw = props.get("data_2d")
    data = block_values(raw) if isinstance(raw, Block) else []
    if len(data) < 4 or len(data) % 2:
        raise UnsupportedGeometry("Alignment has no usable solved vertical vertices.")
    if any(value.lower() == "null" or value == null_value for value in data):
        raise UnsupportedGeometry("Solved alignment profile has missing coordinates.")
    stations = [number(v, "profile chainage") for v in data[::2]]
    levels = [number(v, "profile elevation") for v in data[1::2]]
    if any(b <= a for a, b in zip(stations, stations[1:])):
        raise UnsupportedGeometry("Alignment profile chainages must increase.")
    geometry = props.get("geometry_data")
    items = geometry.items if isinstance(geometry, Block) else []
    count = len(stations) - 1
    if geometry is not None and (not isinstance(geometry, Block) or len(items) not in {2 * count, 2 * (count + 1)}):
        raise UnsupportedGeometry("Alignment profile segment count mismatch.")
    coefficients = []
    for i in range(count):
        a, b = stations[i:i + 2]
        za, zb = levels[i:i + 2]
        grade, quadratic = (zb - za) / (b - a), 0.0
        if items:
            kind, body = items[2 * i:2 * i + 2]
            if not hasattr(kind, "value") or not isinstance(body, Block):
                raise UnsupportedGeometry("Malformed alignment profile segment.")
            if kind.value.lower() == "parabola":
                vip = fields(body)
                mid = number(scalar(vip, "chainage"), "vertical intersection chainage")
                height = number(scalar(vip, "height"), "vertical intersection height")
                if not a < mid < b or abs(mid - (a + b) / 2) > 1e-6:
                    raise UnsupportedGeometry("Only symmetric solved alignment parabolas are supported.")
                grade = (height - za) / (mid - a)
                quadratic = ((zb - height) / (b - mid) - grade) / (2 * (b - a))
            elif kind.value.lower() != "straight":
                raise UnsupportedGeometry(f"Unsupported alignment profile segment: {kind.value}.")
        coefficients.append((grade, quadratic))

    result = []
    station = start
    remaining = 100_000
    for primitive in primitives:
        if primitive.kind != "polyline":
            raise UnsupportedGeometry("Alignment plan must consist of solved line and arc segments.")
        points, elevations = [], []
        edges = len(primitive.points) if primitive.closed else len(primitive.points) - 1
        for i in range(edges):
            checkpoint(cancel)
            a, b = primitive.points[i], primitive.points[(i + 1) % len(primitive.points)]
            bulge = primitive.bulges[i]
            arc = arc_parameters(a, b, bulge) if bulge else None
            length = abs(arc[3]) * arc[1] if arc else math.dist(a, b)
            if not length:
                continue
            end = station + length
            if station < stations[0] - 1e-6 or end > stations[-1] + 1e-6:
                raise UnsupportedGeometry("Solved vertical profile does not cover the horizontal alignment.")
            first = max(0, min(count - 1, bisect_right(stations, station) - 1))
            for j in range(first, count):
                lo, hi = max(station, stations[j]), min(end, stations[j + 1])
                if hi <= lo:
                    break
                grade, quadratic = coefficients[j]
                origin = stations[j]
                extremum = origin - grade / (2 * quadratic) if quadratic else lo
                boundaries = [lo, extremum, hi] if lo < extremum < hi else [lo, hi]
                # Linear interpolation error is bounded by max |f''| * step² / 8.
                plan_rate = math.sqrt((1 / arc[1]) / 8 / plan_tolerance) if arc else 0
                elevation_rate = math.sqrt(abs(quadratic) / 4 / elevation_tolerance)
                rate = max(plan_rate, elevation_rate)
                for left, right in zip(boundaries, boundaries[1:]):
                    needed = (right - left) * rate
                    if not math.isfinite(needed) or needed >= remaining:
                        raise UnsupportedGeometry("Alignment sampling vertex limit exceeded.")
                    steps = max(1, math.ceil(needed))
                    remaining -= steps + 1
                    if remaining < 0:
                        raise UnsupportedGeometry("Alignment sampling vertex limit exceeded.")
                    for k in range(steps + 1):
                        checkpoint(cancel)
                        if k == 0 and points:
                            continue
                        chainage = right if k == steps else left + (right - left) * k / steps
                        t = (chainage - station) / length
                        if arc:
                            (cx, cy), radius, angle, sweep = arc
                            xy = (cx + radius * math.cos(angle + t * sweep), cy + radius * math.sin(angle + t * sweep))
                        else:
                            xy = (a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]))
                        offset = chainage - origin
                        points.append(xy)
                        elevations.append(levels[j] + offset * (grade + offset * quadratic))
                if hi >= end:
                    break
            station = end
        if len(points) < 2:
            raise UnsupportedGeometry("Empty sampled alignment geometry.")
        result.append(Primitive("polyline", points, colour=primitive.colour, style=primitive.style,
                                elevations=elevations))
    vertices = sum(len(primitive.points) for primitive in result)
    warn("ALIGNMENT_SAMPLED", f"Sampled {vertices} alignment vertices using a second-derivative chord error bound "
         f"with plan tolerance {plan_tolerance:g} and elevation tolerance {elevation_tolerance:g} drawing units. "
         "Alignment labels and civil attributes omitted.")
    return result


def default_alignment_modifiers(block):
    if block is None:
        return True
    if not isinstance(block, Block):
        return False
    props = fields(block)
    if props.keys() - {"use_reference_chainage", "major_chainage_interval", "start", "end",
                       "chainage", "default_hip", "default_vip"}:
        return False
    if flag(scalar(props, "use_reference_chainage", "false")):
        return False
    for key in ("start", "end", "chainage"):
        if key not in props:
            continue
        entry = props[key]
        if not isinstance(entry, Block):
            return False
        settings = fields(entry)
        modes = {"mode": "start_point"} if key == "chainage" else {"horz_mode": "default", "vert_mode": "relative"}
        if settings.keys() - modes.keys() - {"x", "y"}:
            return False
        if any(scalar(settings, mode, default) != default for mode, default in modes.items()):
            return False
        if any(number(scalar(settings, axis, "0")) != 0 for axis in ("x", "y")):
            return False
    return True


def tangent_arc_plan(parts, null_value):
    """Solve the open fixed-line / free-radius arc / fixed-line layout."""
    if not isinstance(parts, Block) or len(parts.items) != 6:
        return None
    names = [getattr(item, "value", "") for item in parts.items[::2]]
    if names != ["fixed_line", "free_arc_radius", "fixed_line"]:
        return None
    before, curve, after = parts.items[1::2]
    if not all(isinstance(part, Block) for part in (before, curve, after)):
        raise UnsupportedGeometry("Malformed tangent arc parts.")

    metadata = {"id", "time_created", "time_updated"}

    def line(block):
        props = fields(block)
        if props.keys() - metadata - {"x1", "y1", "x2", "y2", "offset", "visible"}:
            raise UnsupportedGeometry("Unverified fixed-line parameters.")
        values = [scalar(props, key) for key in ("x1", "y1", "x2", "y2")]
        if null_value in values:
            raise UnsupportedGeometry("Fixed line has a missing coordinate.")
        x1, y1, x2, y2 = (number(value, key) for value, key in zip(values, ("x1", "y1", "x2", "y2")))
        length = math.hypot(x2 - x1, y2 - y1)
        if not length:
            raise UnsupportedGeometry("Zero-length fixed line.")
        ux, uy = (x2 - x1) / length, (y2 - y1) / length
        offset = number(scalar(props, "offset", "0"), "fixed-line offset")
        return ((x1 + uy * offset, y1 - ux * offset), (x2 + uy * offset, y2 - ux * offset),
                (ux, uy), flag(scalar(props, "visible", "true")))

    start, _, u, entry_visible = line(before)
    other, end, v, exit_visible = line(after)
    props = fields(curve)
    if props.keys() - metadata - {"r"}:
        raise UnsupportedGeometry("Unverified free-radius arc parameters.")
    raw_radius = scalar(props, "r")
    if raw_radius == null_value:
        raise UnsupportedGeometry("Free arc has a missing radius.")
    radius = number(raw_radius, "free arc radius")
    cross = u[0] * v[1] - u[1] * v[0]
    dot = u[0] * v[0] + u[1] * v[1]
    sweep = math.atan2(cross, dot)
    if abs(cross) < 1e-10 or not radius or sweep * radius >= 0:
        raise UnsupportedGeometry("Fixed lines and radius do not define a supported minor tangent arc.")
    delta = other[0] - start[0], other[1] - start[1]
    intersection_distance = (delta[0] * v[1] - delta[1] * v[0]) / cross
    setback = abs(radius) * math.tan(abs(sweep) / 2)
    entry_distance = intersection_distance - setback
    entry = tuple(start[i] + entry_distance * u[i] for i in range(2))
    exit_point = tuple(start[i] + intersection_distance * u[i] + setback * v[i] for i in range(2))
    exit_distance = sum((end[i] - exit_point[i]) * v[i] for i in range(2))
    if (entry_visible and entry_distance < -1e-6) or (exit_visible and exit_distance < -1e-6):
        raise UnsupportedGeometry("Tangent arc cuts beyond a visible fixed-line endpoint.")
    points, bulges = [entry, exit_point], [math.tan(sweep / 4), 0.0]
    if entry_visible and entry_distance > 1e-6:
        points.insert(0, start)
        bulges.insert(0, 0.0)
    if exit_visible and exit_distance > 1e-6:
        points.append(end)
        bulges.append(0.0)
    return points, bulges


def intersection_vertices(props, null_value, cancel):
    """Read the explicitly IP-only, straight-segment 12d layout."""
    if not flag(scalar(props, "horizontal_ips_only", "false")):
        return None
    if not flag(scalar(props, "valid_horizontal", "false")):
        return None
    parts = props.get("horizontal_parts")
    if not isinstance(parts, Block) or len(parts.items) < 4 or len(parts.items) % 2:
        return None
    points = []
    for kind, body in zip(parts.items[::2], parts.items[1::2]):
        checkpoint(cancel)
        if getattr(kind, "value", "").lower() != "ip" or not isinstance(body, Block):
            return None
        fields_ip = fields(body)
        if fields_ip.keys() - {"x", "y", "id", "time_created", "time_updated"}:
            raise UnsupportedGeometry("IP-only alignment contains unverified IP fields.")
        x, y = (scalar(fields_ip, key) for key in ("x", "y"))
        if null_value in (x, y):
            raise UnsupportedGeometry("Intersection point has a missing coordinate.")
        points.append((number(x, "intersection X"), number(y, "intersection Y")))
    if len(set(points)) < 2:
        raise UnsupportedGeometry("IP-only alignment has fewer than two distinct points.")
    return points


def drainage_plan(source, warn, keep_z, cancel):
    if keep_z:
        raise UnsupportedGeometry("Drainage pipe centreline elevations are not verified for Keep Z.")
    props = {}
    pits = []
    pipes = 0
    items = source.block.items
    if len(items) % 2:
        raise UnsupportedGeometry("Malformed drainage object.")
    for name, value in zip(items[::2], items[1::2]):
        checkpoint(cancel)
        kind = getattr(name, "value", "").lower()
        if kind in {"pit", "pit_v2", "pipe"}:
            if not isinstance(value, Block):
                raise UnsupportedGeometry("Malformed drainage pit or pipe block.")
            if kind == "pipe":
                pipes += 1
            else:
                pits.append(fields(value))
        elif kind in {"data", "colour", "style", "normal", "extrusion", "plane", "transform", "matrix"}:
            if kind in props:
                raise UnsupportedGeometry("Repeated drainage geometry field.")
            props[kind] = value
    if {"normal", "extrusion", "plane", "transform", "matrix"} & props.keys():
        raise UnsupportedGeometry("Drainage source transform is not supported.")
    data = props.get("data")
    values = block_values(data) if isinstance(data, Block) else []
    if not pipes:
        raise UnsupportedGeometry("Drainage contains no pipe spans.")
    if len(pits) != pipes + 1 or len(values) < 10 or len(values) % 5:
        raise UnsupportedGeometry("Drainage has no verified sequence of pits, pipes and solved vertices.")
    points = []
    null_value = source.defaults["null"]
    for i in range(0, len(values), 5):
        checkpoint(cancel)
        if null_value in values[i:i + 2]:
            raise UnsupportedGeometry("Drainage has a missing solved plan coordinate.")
        if number(values[i + 3], "drainage segment radius") != 0:
            raise UnsupportedGeometry("Curved drainage spans are not supported.")
        points.append((number(values[i], "drainage X"), number(values[i + 1], "drainage Y")))
    indices = []
    for pit in pits:
        token = scalar(pit, "ip", "")
        if not token.isascii() or not token.isdecimal() or len(token) > len(str(len(points))):
            raise UnsupportedGeometry("Drainage pit has an invalid solved vertex index.")
        index = int(token)
        if index >= len(points) or (indices and index <= indices[-1]):
            raise UnsupportedGeometry("Drainage pit indices do not follow the solved vertex order.")
        x, y = (scalar(pit, axis) for axis in ("x", "y"))
        if null_value in (x, y) or math.dist(points[index], (number(x, "pit X"), number(y, "pit Y"))) > 1e-6:
            raise UnsupportedGeometry("Drainage pit coordinates disagree with the solved vertices.")
        indices.append(index)
    if indices[0] != 0 or indices[-1] != len(points) - 1:
        raise UnsupportedGeometry("Drainage pit indices do not cover the solved vertices.")
    colour = scalar(props, "colour", source.defaults["colour"])
    style = scalar(props, "style", source.defaults["style"])
    result = []
    for first, last in zip(indices, indices[1:]):
        checkpoint(cancel)
        span = points[first:last + 1]
        if len(set(span)) < 2:
            raise UnsupportedGeometry("Drainage pipe span collapses in plan.")
        result.append(Primitive("polyline", span, colour=colour, style=style))
    warn("DRAINAGE_PLAN_ONLY", f"Exported {pipes} solved pipe spans in plan. Pit structures, pipe dimensions, "
         "elevations and hydraulic attributes were omitted.")
    return result


def convert_geometry(source, warn, keep_z=False, cancel=None, *, plan_tolerance=0.01, elevation_tolerance=0.01):
    if source.kind == "drainage":
        return drainage_plan(source, warn, keep_z, cancel)
    if source.kind not in {"super_alignment", "super", "2d", "3d", "polyline", "arc", "circle", "text"}:
        raise UnsupportedGeometry(f"Unsupported source type: {source.kind}")
    props = fields(source.block)
    if source.kind == "super_alignment":
        horizontal = props.get("horizontal_data")
        ip_points = intersection_vertices(props, source.defaults["null"], cancel)
        solved = fields(horizontal) if isinstance(horizontal, Block) else {}
        reconstructed = None
        if ip_points is not None:
            closed = flag(scalar(props, "closed", "0"))
            raw = solved.get("data_2d")
            if any(key in solved for key in ("geometry_data", "radius_data", "major_data", "data_3d", "data",
                                             "normal", "extrusion", "plane", "transform", "matrix")):
                raise UnsupportedGeometry("IP-only alignment has an unverified solved plan representation.")
            if raw is not None:
                if not isinstance(raw, Block):
                    raise UnsupportedGeometry("Malformed solved IP alignment vertices.")
                values = block_values(raw)
                expected = [coordinate for point in ip_points for coordinate in point]
                if values and (len(values) != len(expected) or any(
                    abs(number(value) - coordinate) > 1e-6 for value, coordinate in zip(values, expected)
                ) or flag(scalar(solved, "closed", "0")) != closed):
                    raise UnsupportedGeometry("Intersection points disagree with solved alignment geometry.")
            if not isinstance(raw, Block) or not raw.items:
                if any(key in props for key in ("normal", "extrusion", "plane", "transform", "matrix")):
                    raise UnsupportedGeometry("IP-only alignment has an unsupported source transform.")
                reconstructed = [Primitive("polyline", ip_points, [0.0] * len(ip_points), closed,
                                     colour=scalar(solved, "colour", scalar(props, "colour", source.defaults["colour"])),
                                     style=scalar(solved, "style", scalar(props, "style", source.defaults["style"])))]
                warn("ALIGNMENT_IP_RECONSTRUCTED", "Plan reconstructed from verified straight intersection points.")
        else:
            vertex_keys = {"data_2d", "data_3d", "data"} & solved.keys()
            has_solved_vertices = any(not isinstance(solved[key], Block) or solved[key].items for key in vertex_keys)
            if not has_solved_vertices and flag(scalar(props, "valid_horizontal", "false")) and not flag(scalar(props, "closed", "0")):
                arc = tangent_arc_plan(props.get("horizontal_parts"), source.defaults["null"])
                if arc is not None:
                    modifiers = props.get("geometry_modifiers")
                    transforms = {"normal", "extrusion", "plane", "transform", "matrix"}
                    if transforms & (props.keys() | solved.keys()) or not default_alignment_modifiers(modifiers):
                        raise UnsupportedGeometry("Tangent arc reconstruction requires unmodified source geometry.")
                    points, bulges = arc
                    reconstructed = [Primitive("polyline", points, bulges,
                                               colour=scalar(solved, "colour", scalar(props, "colour", source.defaults["colour"])),
                                               style=scalar(solved, "style", scalar(props, "style", source.defaults["style"])))]
                    warn("ALIGNMENT_TANGENT_ARC_RECONSTRUCTED", "Plan reconstructed from two fixed lines and a free-radius tangent arc.")
        if not isinstance(horizontal, Block) and reconstructed is None:
            raise UnsupportedGeometry("Alignment has no solved horizontal_data block.")
        parts = props.get("horizontal_parts")
        if reconstructed is None and isinstance(parts, Block) and any(
            getattr(kind, "value", "").lower() in {"transition_compound_curve", "transition_compound_taper"}
            and isinstance(body, Block) and flag(scalar(fields(body), "active", "true"))
            for kind, body in zip(parts.items[::2], parts.items[1::2])
        ):
            warn("ALIGNMENT_TRANSITION_SOLVED_ONLY",
                 "Used the exported solved plan for the active transition. The original transition definition was not evaluated.")
        if keep_z:
            profile = props.get("vertical_data")
            if not isinstance(profile, Block):
                raise UnsupportedGeometry("Keep Z requires a solved alignment profile.")
            plan = reconstructed or convert_geometry(replace(source, kind="super", block=horizontal), warn, cancel=cancel)
            return alignment_elevations(plan, profile, number(scalar(solved, "chainage",
                                        scalar(props, "chainage", "0")), "alignment chainage"),
                                        source.defaults["null"], warn, cancel, plan_tolerance, elevation_tolerance)
        warn("ALIGNMENT_PLAN_ONLY", "Horizontal geometry only; labels, vertical design and civil semantics are omitted.")
        if reconstructed is not None:
            return reconstructed
        props = {**props, **solved}

    colour = scalar(props, "colour", source.defaults["colour"])
    style = scalar(props, "style", source.defaults["style"])
    common = {"colour": colour, "style": style}
    def elevation(value):
        if value is None or value.lower() == "null" or value == source.defaults["null"]:
            raise UnsupportedGeometry("Keep Z requires a known source elevation.")
        return number(value, "Z")

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
        if keep_z:
            common["elevations"] = [elevation(scalar(props, "z", None))]
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
            if keep_z:
                common["elevations"] = [elevation(scalar(props, "zcentre", None))]
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
        if keep_z:
            levels = [elevation(scalar(props, key, None)) for key in ("zcentre", "zstart", "zend")]
            if len(set(levels)) != 1:
                raise UnsupportedGeometry("Keep Z does not support arcs with varying elevations.")
            common["elevations"] = levels[1:]
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
    levels = []
    if keep_z:
        levels = ([elevation(values[i]) for i in range(2, len(values), stride)] if stride >= 3
                  else [elevation(scalar(props, "z", None))] * len(points))
    positions = list(zip(points, levels)) if keep_z else points
    closed = flag(scalar(props, "closed", "0"))
    if source.kind in {"2d", "3d"} and len(points) > 2 and positions[0] == positions[-1]:
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
    if levels and len(set(levels)) > 1 and any(radii[:nsegments]):
        raise UnsupportedGeometry("Keep Z does not support curved strings with varying elevations.")
    for i in range(max(0, nsegments)):
        if positions[i] == positions[(i + 1) % len(points)]:
            warn("DEGENERATE_SEGMENT", "Repeated vertices create a collapsed segment.")
        bulges[i] = radius_bulge(points[i], points[(i + 1) % len(points)], radii[i], majors[i])
    if breakline == "point" or len(set(positions)) == 1:
        if breakline != "point":
            warn("COLLAPSED_TO_POINT", "All XY vertices coincide; emitted as a point.")
        if levels:
            return [Primitive("point", [p], elevations=[z], **common) for p, z in dict.fromkeys(positions)]
        return [Primitive("point", [p], **common) for p in dict.fromkeys(points)]
    colours = block_values(props["colour_data"]) if "colour_data" in props else None
    if colours:
        if len(colours) not in {nsegments, len(points)}:
            raise UnsupportedGeometry("Segment colour count mismatch.")
        if len(set(colours[:nsegments])) > 1 or colours[0] != colour:
            return [Primitive("polyline", [points[i], points[j]], [bulges[i], 0], colour=colours[i], style=style,
                              elevations=[levels[i], levels[j]] if levels else [])
                    for i in range(nsegments) for j in [(i + 1) % len(points)]]
    return [Primitive("polyline", points, bulges, closed, elevations=levels, **common)]
