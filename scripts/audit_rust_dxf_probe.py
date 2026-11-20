"""Independently check the representative drawing from the Rust DXF probe."""

import sys

import ezdxf


def audit_probe(path):
    drawing = ezdxf.readfile(path)
    audit = drawing.audit()
    assert not audit.errors and not audit.fixes, [issue.message for issue in (*audit.errors, *audit.fixes)]
    assert drawing.dxfversion == "AC1032"
    assert drawing.units == 6

    label, level, graded = drawing.modelspace()
    assert label.dxftype() == "TEXT"
    assert label.dxf.text == "Café 測量"
    assert (label.dxf.halign, label.dxf.valign) == (1, 3)
    assert tuple(label.dxf.align_point) == (5, 8, 7)
    assert label.dxf.height == 0.5
    assert abs(label.dxf.rotation - 30) < 1e-10
    assert level.dxftype() == "LWPOLYLINE"
    assert level.dxf.elevation == 12
    assert level.get_points("xyb") == [(500000, 6200000, -0.5), (500001.25, 6200002.5, 0)]
    assert graded.dxftype() == "POLYLINE" and graded.is_3d_polyline
    assert [tuple(point) for point in graded.points()] == [(1, 2, 12), (3, 4, 16)]


if __name__ == "__main__":
    audit_probe(sys.argv[1])
    print("DXF 2018 reopened and passed the independent probe audit.")
