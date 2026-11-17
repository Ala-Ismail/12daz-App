# Expanded geometry sample

`expanded_geometry.12da` is a hand-authored F-EXPANDED fixture in UTF-8 without a BOM. Coordinates are in metres. The 4D string, contour, pipe, spiral parameters and intersection-point alignment blocks use proposed layouts pending comparison with a genuine 12d Model export.

| Object | Reference expectation |
| --- | --- |
| Open 2D | Two legs of lengths 10 and 5 with an open end |
| Vertical then sloped | Keep Z preserves both vertices at XY (0, 10) and their elevations of −2 and 3 |
| Legacy polyline | Circular segment with radius 10 and elevations 5 and 9 at its endpoints |
| XYZ plus sample measure | Two XYZ vertices with mock fourth values 100 and 112 |
| Closed contour | Closed 10 × 10 square at Z = 12.5 |
| Sloping centreline | Three centreline vertices at Z = −1, −2 and −3 |
| Straight arc and transition | Straight east, then a quarter circle centred at (10, 110) with a northward exit tangent. The 10 m spiral turns by 0.1 rad, with heading measured from north given by s²/1000 at distance s along the spiral. Integrate (−sin(s²/1000), cos(s²/1000)) from 0 to 10 for the endpoint offset. This example includes a curvature jump at the arc-to-spiral join. |
| Solved horizontal | Straight from (0, 200) to (20, 200) |
| Intersection point with transitions and sag | Symmetric left turn through 90° with two 10 m clothoids and a 20 m radius arc. Each spiral turns by 0.25 rad, leaving π/2 − 0.5 rad for the circular sweep. The vertical profile reaches Z = 10 at chainages 40 and 80 and a minimum of 9.75 at chainage 60. |

Run the current CLI with `inspect tests/fixtures/expanded_geometry.12da --encoding utf-8`. Expect nine source objects, four convertible and five skipped with diagnostics. This fixture exercises the current skip path and supplies inputs for development. Format qualification requires examples exported by 12d Model.
