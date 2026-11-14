import math
import re

import ezdxf
from ezdxf import enums, units

from ..errors import ConversionError, checkpoint


UNIT_CODES = {"metres": units.M, "millimetres": units.MM, "feet": units.FT, "inches": units.IN}
COLOURS = {"red": 1, "yellow": 2, "green": 3, "cyan": 4, "blue": 5, "magenta": 6, "white": 7,
           "black": 7, "grey": 8, "gray": 8, "no_colour": 256}


class DxfWriter:
    def __init__(self, unit_name):
        if unit_name not in UNIT_CODES:
            raise ConversionError("Choose units: metres, millimetres, feet or inches. Unit declaration does not rescale coordinates.")
        self.doc = ezdxf.new("R2018")
        self.doc.units = UNIT_CODES[unit_name]
        self.doc.header["$PDMODE"] = 3
        self.doc.header["$PDSIZE"] = -2
        self.space = self.doc.modelspace()
        self.layers = {}
        self.used = {"0", "defpoints"}
        self.count = 0

    def layer(self, model):
        if model not in self.layers:
            base = re.sub(r'[<>/\\":;?*|=\x00-\x1f]', "_", model).strip()[:240] or "Model"
            name, i = base, 2
            while name.casefold() in self.used:
                name = f"{base}_{i}"
                i += 1
            self.used.add(name.casefold())
            self.layers[model] = name
            self.doc.layers.new(name)
        return self.layers[model]

    def write(self, primitive, model, warn):
        attrs = {"layer": self.layer(model)}
        colour = primitive.colour.lower()
        if colour in COLOURS:
            attrs["color"] = COLOURS[colour]
        elif colour.isdecimal() and 1 <= int(colour) <= 255:
            attrs["color"] = int(colour)
        else:
            attrs["color"] = 7
            warn("COLOUR_SUBSTITUTION", f"Unresolved colour {primitive.colour!r}; using white/black CAD index 7.")
        if primitive.style.lower() not in {"1", "continuous"}:
            warn("LINESTYLE_SUBSTITUTION", f"Unresolved style {primitive.style!r}; using CONTINUOUS.")
        if primitive.kind == "point":
            self.space.add_point((*primitive.points[0], 0), dxfattribs=attrs)
        elif primitive.kind == "circle":
            self.space.add_circle((*primitive.points[0], 0), primitive.radius, dxfattribs=attrs)
        elif primitive.kind == "polyline":
            points = [(x, y, primitive.bulges[i] if i < len(primitive.bulges) else 0) for i, (x, y) in enumerate(primitive.points)]
            self.space.add_lwpolyline(points, format="xyb", close=primitive.closed, dxfattribs=attrs)
        elif primitive.kind == "text":
            alignment = primitive.justify.upper().replace("CENTRE", "CENTER").replace("-", "_")
            text = self.space.add_text(primitive.text, dxfattribs={**attrs, "height": primitive.height,
                       "width": primitive.width, "oblique": primitive.slant, "rotation": primitive.rotation})
            text.set_placement((*primitive.points[0], 0), align=getattr(enums.TextEntityAlignment, alignment))
        else:
            raise ConversionError(f"Unexpected output primitive {primitive.kind}.")
        self.count += 1

    def save(self, path):
        self.doc.set_modelspace_vport(100)
        self.doc.saveas(path)


def validate_dxf(path, expected_count, cancel=None):
    checkpoint(cancel)
    doc = ezdxf.readfile(path)
    audit = doc.audit()
    if audit.errors or audit.fixes:
        raise ConversionError("Output DXF failed audit or required repair.")
    if doc.dxfversion != "AC1032":
        raise ConversionError("Unexpected DXF version.")
    space = doc.modelspace()
    if len(space) != expected_count:
        raise ConversionError("DXF entity count changed during validation.")
    for entity in space:
        checkpoint(cancel)
        for name in ("elevation", "thickness"):
            if entity.dxf.hasattr(name) and entity.dxf.get(name) != 0:
                raise ConversionError(f"Nonzero output {name}.")
        for name in ("location", "center", "insert", "align_point"):
            if entity.dxf.hasattr(name):
                point = entity.dxf.get(name)
                if point.z != 0 or not all(math.isfinite(v) for v in point):
                    raise ConversionError("Output geometry has invalid coordinates or nonzero Z.")
        if tuple(entity.dxf.get("extrusion", (0, 0, 1))) != (0, 0, 1):
            raise ConversionError("Nonstandard output extrusion.")
        if entity.dxftype() == "LWPOLYLINE":
            if any(not math.isfinite(v) for point in entity.get_points("xyb") for v in point):
                raise ConversionError("Invalid polyline coordinates.")
    return doc
