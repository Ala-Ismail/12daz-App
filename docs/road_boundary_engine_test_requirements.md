# Road-boundary detection engine test requirements

Use these cases to verify the [standalone engine requirements](road_boundary_engine_requirements.md). Run the tests with local fixtures. Independent qualification uses at least **80% F1** on held-out road sections. Section 8.3 records the maintainer acceptance used for the current integration.

## 1. Fixture and execution requirements

- Use small synthetic cases with expected metre coordinates for offsets, topology, provenance and error handling. Supply input CRS, source tags and expected output explicitly.
- Use independent surveyed or annotated carriageway edges for the accuracy benchmark. Freeze the reference dataset, test coverage and lateral tolerance before scoring. Keep acceptance sections geographically separate from tuning sections.
- Run the standalone engine and scorer offline. Record the engine revision, fixture revisions, settings and diagnostics. Keep private surveys and working outputs local.

## 2. Geometry and data handling

| Case | Fixture and exercise | Pass result |
| --- | --- | --- |
| RBE-T01 | Mapped polygon with an island, overlapping centreline and partial polygon coverage. | Mapped outer and island edges are preserved. The covered centreline emits no duplicate edges. Uncovered geometry retains its own classification. |
| RBE-T02 | Metre centreline from `(0, 0)` to `(100, 0)` with an 8 m carriageway width. Compare the two road sides. | Edges follow `y = -4` and `y = 4` over the expected interval. Output records the mapped width and labels both edges as estimated. |
| RBE-T03 | Bent road with a valid lane count and documented lane-width configuration, followed by a road with no credible width. | The estimated sides follow the first bend and record the inference used. The second feature reports centreline only. |
| RBE-T04 | Same-level T-junction, divided carriageways, and a bridge crossing a lower road. | Road sides follow the connected surface at the T-junction and preserve distinct carriageways and grade-separated crossings. |
| RBE-T05 | Window clipping a road, a self-intersecting polygon, malformed and negative widths, and a valid neighbouring feature. | Clip and artificial end edges are excluded. Invalid input produces feature-level diagnostics and the valid neighbour still succeeds. |
| RBE-T06 | Equivalent road geometry in two supported CRSs and a fixture with an absent CRS. | Valid fixtures agree in a common projected metre CRS within the declared tolerance. A missing CRS produces a clear validation error. |
| RBE-T07 | Same fixture, inputs and settings run twice. | Geometry, classifications and provenance are equivalent across runs. |
| RBE-T08 | Present mapped and estimated edges, mapped and synthesised centrelines, and a centreline-only result as DXF evidence. Reopen it with a DXF reader. | Separate layers identify the reference linework and output classes. An optional survey overlay has its own layers. Units, coordinates and expected entity counts survive reopening. |

## 3. Benchmark scoring

| Case | Fixture and exercise | Pass result |
| --- | --- | --- |
| RBE-T09 | Reference edge 100 m. Predictions include 80 m matched and 20 m spurious. | Precision, recall and F1 each equal 80%. The numerical threshold passes exactly. |
| RBE-T10 | Reference edge 100 m. Predict the same matching edge twice. | Recall remains 100%. Precision falls to 50% and F1 to about 66.7%. |
| RBE-T11 | Predict a parallel edge just outside the frozen tolerance and a correct edge on the other side of the road. | Neither prediction matches the tested reference side. The scorer respects tolerance and road-side identity. |
| RBE-T12 | Evaluate a surveyed section for which the engine returns centreline only. | Its known reference-edge length remains in the recall denominator. The centreline contributes no matched boundary length. |
| RBE-T13 | Run the fixed held-out benchmark across the agreed coverage. | Report pooled precision, recall and F1, plus mapped and estimated breakdowns. Independent qualification passes when the structural checks pass and the pooled F1 is at least 80%. |

## 4. Synthesised road-centreline tests

| Case | Fixture and exercise | Pass result |
| --- | --- | --- |
| RBE-T14 | 8 m wide straight carriageway polygon from `x = 0` to `x = 100`, with sides at `y = -4` and `y = 4`. Supply no mapped centreline. | The output includes a centreline inside the polygon that follows `y = 0`, with synthesised provenance and the contributing polygon identifier. |
| RBE-T15 | Bent and divided carriageways with no mapped centrelines, a median between them, and an ambiguous junction branch. | Centrelines follow the respective carriageways, remain within the road surfaces, and leave the median clear. A branch that cannot be resolved is flagged or omitted. |
| RBE-T16 | Supply a mapped centreline and a matching surface polygon, then supply an incomplete pair of road edges in a separate feature. | The mapped centreline is retained without a duplicate synthesis. Incomplete edges produce a diagnostic when they cannot support a credible centreline. |

## 5. Evidence to retain

For each benchmark run, retain the fixture identifiers, coverage, lateral tolerance, source and reference provenance, totals in metres, scores and reasons for any excluded intervals. Include a DXF showing representative matches and misses, with clearly labelled source, prediction and independent reference layers. A synthetic example demonstrates geometry and presentation. Use held-out reference data for the integration score.

## 6. Multi-model projection match acceptance

Run these cases against the automatic comparison using the accepted standalone engine. The projection match threshold is strictly greater than **70%** of evaluable survey road length across road boundaries and centrelines. The engine qualification threshold is at least **80% F1** on independent road edges.

| Case | Fixture and exercise | Pass result |
| --- | --- | --- |
| RPM-T01 | Match 75 m of boundaries and 65 m of centreline, out of 100 m per class. Increase matched centreline by 1 m and repeat. | 140/200 m is exactly 70% and fails the match threshold. 141/200 m is 70.5% and passes with adequate evidence. Apply the threshold before display rounding. |
| RPM-T02 | Store edges and centrelines in separate road models alongside long utilities and property models. Duplicate part of the road linework in a third road model. | The road models contribute to one pooled score. Exclude non-road geometry, count each duplicate road section once and report its contributing models. |
| RPM-T03 | Supply 40/50 m of matched edges and 30/150 m of matched centreline from two comparable road models. | The pooled score is 70/200 m, or 35%. The report shows both model results. |
| RPM-T04 | Compare a perfect centreline match where the map has insufficient edge coverage, then use a complete reference with a surveyed road section that has no corresponding map road. | The first run is inconclusive. The second counts the surveyed section as unmatched length and reports why. |
| RPM-T05 | Two candidate grids pass 70% with adequate evidence. Another falls below the threshold but lacks reliable evidence for elimination. | Both matches remain visible. The third candidate remains available with its status and reason. The active projection stays as selected. |

## 7. Run the standalone acceptance fixtures

From the repository root, run `cargo test --manifest-path rust/road_boundary_engine_tests/Cargo.toml --offline --locked`. The scoring fixtures exercise arithmetic on supplied lengths. Several spatial and benchmark cases still need fixtures and implementation.

Build the engine with `cargo build --manifest-path rust/road_boundary_engine/Cargo.toml --offline --locked`. Set `RBE_ENGINE_EXE` to the absolute path of the resulting executable in the engine crate's `target/debug` directory. Run `cargo test --manifest-path rust/road_boundary_engine_tests/Cargo.toml --offline --locked -- --include-ignored` to exercise detection, DXF and benchmark scoring along with the arithmetic tests.

The test adapter calls the runner with `--input <input.json> --output <output.json>`. Input contains `input_crs`, `output_crs` and a `features` array with `id`, `tags` and GeoJSON-style `geometry`. Output contains `output_crs`, `edges` and `centrelines`. Each tested line carries `source_ids`, `points` and `provenance`. Estimated edges include `width_source`. The missing-CRS fixture expects a nonzero exit and a CRS diagnostic on standard error. Straight fixture coordinates have a 5 cm numerical tolerance. The held-out benchmark sets its own lateral tolerance.

## 8. Validation status

The Windows release build passed all 25 automated checks, including synthetic coverage of RBE-T01 through RBE-T12 and RBE-T14 through RBE-T16. The independent `ezdxf` reader audited the MGA55 OpenStreetMap example and the synthetic centreline example with no errors or fixes. The multi-model fixtures exercise scoring arithmetic. The end-to-end projection tests depend on app integration. See Section 8.3 for the maintainer's acceptance decision and the status of RBE-T13.

The MGA56 road-design overlay passed the maintainer's visual review. The detected lines and crossings appeared to align with the design.

### 8.1 MGA56 design corridor pilot

The pilot used two manually paired opposing design kerb chains on the north and east arms, covering 37.9 m of design centreline. It excluded the junction interior and other sections without verified kerb pairs. The scorer measured detected length following the corridor and design length covered, then used their harmonic mean as the fit. It required heading agreement within 35 degrees and checked edges separately against the nearer design side within 2 m of transverse distance.

| Case | Centreline fit | Road-edge fit |
| --- | ---: | ---: |
| MGA56 detection | 100% | 70.7% |
| 12 m lateral shift | 0% | 0% |
| Crossing-only control | 0% | Unscored |

These results cover the two reviewed design arms. See Section 8.4 for the archive-wide comparison.

### 8.2 MGA54 design overlay review

The maintainer approved the visual alignment of the MGA54 GHD road-design overlay. The archive's saved registration is GDA2020 / MGA Zone 54 (EPSG:7854). The engine produced 50 estimated road-edge polylines and 22 mapped centreline polylines, and both DXFs passed an independent audit.

The corridor pilot used the `CE00` and `CE0I` bitumen-edge strings on opposite sides of the `MC00` design alignment. It counted duplicate strings once and excluded junction transitions and sections without a continuous pair. The three Sheedy Road sections covered 475.8 m of design centreline. The pilot reused the MGA56 heading and edge tolerances.

| Case | Centreline fit | Road-edge fit |
| --- | ---: | ---: |
| MGA54 detection | 100% | 100% |
| 12 m lateral shift | 0% | 0% |
| Crossing-only control | 0% | Unscored |

The scored DXF passed an independent audit. The two design pilots distinguish the actual alignment from the controls. See Section 8.4 for the archive-wide comparison.

### 8.3 Maintainer acceptance

The maintainer accepted the current standalone road detection results based on the MGA54 and MGA56 visual reviews and corridor pilots. The independent held-out benchmark (RBE-T13) was waived for this acceptance, and no independent road-edge F1 score was measured. The maintainer then approved implementation of the 12dAZ integration using this acceptance. The standalone pilot scores measure design alignment.

### 8.4 Integrated corridor comparison

The two sample-based acceptance tests ran without applying saved grids or selecting road models. Automatic pairing recovered all 921 reviewed MGA54 stations, 44 of 44 MGA56 east-arm stations and 28 of 33 north-arm stations. The archive-wide score includes all automatically paired sections within the supplied reference coverage.

| Archive | Centreline coverage | Road-edge coverage | Pooled score | Result |
| --- | ---: | ---: | ---: | --- |
| MGA54 GHD | 100.0% | 99.8% | 99.9% | Match |
| MGA56 SHRP | 86.6% | 56.6% | 66.6% | Best measured zone for review |

The comparison ran offline with cached road extracts. Candidates beyond the extract coverage remained unchecked. The crossing-only control and strict 70% threshold tests passed. Applying a suggested grid remains a separate user action.
