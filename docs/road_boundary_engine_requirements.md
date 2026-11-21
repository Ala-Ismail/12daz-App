# Road-boundary detection engine requirements

The [standalone Rust engine](../rust/road_boundary_engine/Cargo.toml) turns local OpenStreetMap road reference geometry into road edges and centrelines with a record of how each was obtained. It exports DXF evidence and can score output against independently labelled road edges. The [Background Map design](../design.md#19-openstreetmap-road-boundaries-beneath-the-survey) describes its intended use in 12dAZ after qualification.

## 1. Scope and separation

- **RBE-01 Standalone operation.** Keep the detection library and its test runner independent of the 12dAZ desktop, archive reader, projection suggestion UI and DXF exporter. Run detection and benchmarking from local files without an Internet connection.
- **RBE-02 Responsibility.** Detect or estimate road-surface edges from OpenStreetMap vectors and synthesise road centrelines where needed. Report the observation or assumption used for each result. Leave survey comparison, grid elimination and plan rendering to the later 12dAZ integration.
- Preserve feature identifiers and provenance for the [future multi-model projection comparison](../detection_enhancment.md#21-score-the-road-models-in-the-archive), which uses both road boundaries and centrelines and requires greater than 70% agreement.
- **RBE-03 Integration gate.** The maintainer accepted the MGA54 and MGA56 reviews and corridor pilots for integration. The 80% held-out F1 benchmark is the quantitative target for any future independent qualification.

## 2. Inputs and outputs

- **RBE-04 Reference input.** Accept road-surface polygons and road centrelines from a local vector extract. Carry source feature identifiers, available width and lane tags, road class and grade information. Record the extract's source, date, coverage and attribution.
- **RBE-05 Coordinates.** Require an explicit input CRS and perform offsets and distance measurements in a projected coordinate system with metre units. Identify the output coordinate system. Report unusable coordinates or missing CRS information before attempting an offset.
- **RBE-06 Result categories.** Report each usable reference as **Mapped outline**, **Estimated edge** or **Centreline only**. Include the geometry and source identifier. For an estimated edge, record the width in metres and whether it came from a mapped width, lane information or a road-class estimate. For a centreline-only result, record why the edge could not be established.

## 3. Detection rules

- **RBE-07 Prefer mapped surfaces.** Use the outer road-surface boundary where a valid mapped polygon represents the carriageway. Preserve holes and identified traffic islands. Where a mapped polygon and its corresponding centreline overlap, use the polygon in that covered section and avoid duplicate predictions.
- **RBE-08 Estimate from centrelines.** Use a valid mapped carriageway width before considering lane information or a documented road-class default. With a single total width and no side-specific evidence, offset the centreline by half the width on each side. Mark the result as estimated. Return the centreline alone when the available evidence cannot support a width.
- **RBE-09 Respect road topology.** Follow bends and preserve separate carriageways. Join plausible same-level junctions and keep grade-separated crossings distinct. Exclude edges introduced by clipping or buffering the end of an incomplete reference feature from predicted road sides.
- **RBE-10 Handle uncertain inputs.** Report invalid polygons, malformed widths and incomplete coverage with a reason. Retain usable features when an individual feature fails. Identical inputs and settings should produce repeatable geometry and classifications.
- **RBE-11 Synthesise road centrelines.** Retain a compatible mapped centreline when available. Where one is missing, derive a line inside the road surface from the corridor geometry or credible paired road sides. Follow the carriageway through bends, preserve separate carriageways around medians, and break or flag ambiguous junction branches. Output centrelines with separate **Mapped centreline** or **Synthesised centreline** provenance and contributing feature identifiers. Avoid treating a centreline and boundaries derived from it as independent observations.

## 4. Standalone test cases

Use the [detailed test requirements](road_boundary_engine_test_requirements.md) for reproducible fixtures, DXF evidence and scoring checks.

| Case | Input | Expected result |
| --- | --- | --- |
| RBE-T01 | Mapped carriageway with a traffic island and a centreline through it | The mapped outline preserves the island. Its centreline produces no duplicate estimated edge. |
| RBE-T02 | Straight centreline with a valid 8 m width | The two estimated sides are 4 m from the centreline and identify the width source. |
| RBE-T03 | Curved centreline with lane-based width, followed by a section with no credible width | The first section has estimated sides through the bend. The second returns a centreline with a reason. |
| RBE-T04 | Connected junction and a nearby grade-separated crossing | The connected road surfaces join. The grade-separated road sides stay independent. |
| RBE-T05 | Clipped road, malformed width and invalid polygon beside valid features | Clip and artificial end edges are excluded from predictions. Problems are reported, and valid features still produce results. |
| RBE-T06 | Different input CRSs representing the same road and a file with missing CRS | Equivalent inputs produce matching edges in metres. The engine reports the missing CRS. |
| RBE-T14 | Road-surface polygon with no mapped centreline | A synthesised road centreline follows the interior of the carriageway and is identified as derived. |

## 5. Independent benchmark and 80% F1 target

- Use independently surveyed or annotated left and right road edges as the reference. Keep the evaluation sections geographically separate from those used to tune widths or geometry rules. Include curves, junctions, divided roads, and roads with incomplete width tags. Record fixture provenance and the date of the road data.
- Set the lateral match tolerance in metres before scoring. Match corresponding road sides over their common evaluated extent. Prevent a predicted edge from matching multiple reference edges and count duplicate predictions as excess length. Exclude artificial clipping edges and evaluate across the same coverage for predictions and reference.
- **Precision** is the fraction of predicted road-edge length that matches the reference. **Recall** is the fraction of reference road-edge length found by the engine. Pool the lengths across the held-out sections and calculate **F1 = 2 × precision × recall ÷ (precision + recall)**. Report precision, recall and F1 overall, with results by road type and by mapped or estimated output where applicable. Centreline-only output contributes no detected edge length where reference edges exist.
- Independent quantitative qualification requires the structural cases in Section 4 and **F1 ≥ 80%** on a held-out benchmark. Save the fixture list, tolerance, engine revision, metrics and failure examples as acceptance evidence.

## 6. Implementation steps

Run each stage's listed tests and rerun the earlier stages' tests before moving on. Record results and fix failures in the standalone component.

| Stage | Deliverable | Test gate |
| --- | --- | --- |
| 6.1 | Create a standalone Rust crate and offline runner. Read local road features with explicit CRS, coverage and stable identifiers. Establish projected metre output and feature-level diagnostics. | RBE-T06 and the input validation portions of RBE-T05. |
| 6.2 | Preserve mapped road-surface outlines. Add offsets from mapped width or lane/class estimates, width provenance and centreline-only fallback. Remove duplicate estimated edges where a mapped surface covers the same road. | RBE-T01 through RBE-T03. |
| 6.3 | Resolve same-level junctions and distinct carriageways. Preserve grade separation, clip to the evaluation extent and omit artificial edges. | RBE-T04, remaining RBE-T05 checks and RBE-T07. |
| 6.4 | Retain mapped road centrelines. Synthesise missing ones from suitable surfaces or paired edges. Carry source identifiers and flag ambiguous geometry. | RBE-T14 through RBE-T16. |
| 6.5 | Export layered DXF evidence and verify it independently. Freeze benchmark coverage, reference geometry and tolerances. Run the scorer and retain matched and missed examples. | RBE-T08 through RBE-T13, including at least 80% held-out road-edge F1. |
| 6.6 | Use the accepted engine in the automatic 12dAZ comparison across models. | RPM-T01 through RPM-T05, including the strict greater-than-70% projection match threshold. |

## 7. Run the standalone engine

Build it from the repository root. For the first build, Cargo needs access to the locked dependencies. Subsequent builds and all engine runs can use the local cache and files.

```powershell
cargo build --manifest-path rust/road_boundary_engine/Cargo.toml --release --locked
$engine = '.\rust\road_boundary_engine\target\release\road-boundary-engine.exe'
& $engine --input roads.json --output detected.json --dxf detected.dxf
```

The input is a JSON document. Store line and polygon coordinates in the `geometry` field using the GeoJSON X/Y order. For geography, supply longitude then latitude. Preserve the road tags and stable OpenStreetMap identifiers in the export. Tag carriageway polygons with `area:highway`. For individually mapped sides, use `road_side` set to `left` or `right` with a shared `corridor_id`. The optional `reference` contains `source`, `date`, `attribution` and `coverage`. Coverage is the extract's bounding rectangle in the input CRS. Its projected envelope in the output is descriptive. Set `clip` explicitly in the output CRS to limit the evaluation window.

```json
{
  "input_crs": "EPSG:7855",
  "output_crs": "EPSG:7855",
  "features": [
    {
      "id": "demo-road",
      "tags": {"highway": "residential", "width": "8"},
      "geometry": {"type": "LineString", "coordinates": [[350000, 5813000], [350100, 5813000]]}
    }
  ]
}
```

Supported input grids are WGS 84 longitude/latitude, GDA94 and GDA2020 MGA zones 46–56, and the GDA94 and GDA2020 Brisbane City Survey Grids. Output must be a projected metre grid from that list. WGS 84 to GDA2020 uses the backdrop's map-scale datum approximation. Width estimates use mapped width first, then 3.3 m per lane, then a road-class default where defined. The engine omits ambiguous inferred centrelines and records a diagnostic. `--dxf` is optional. Choose new output filenames for each run.

### 7.1 Score independently surveyed or annotated road edges

Run `road-boundary-engine --benchmark benchmark.json --output scores.json --dxf score-evidence.dxf`. The manifest declares `crs`, `tolerance_m`, `reference_source`, `reference_date` and `sections`. For each section record its `id`, `road_type`, `window` in the output CRS, `prediction_file` relative to the manifest, `reference` entries with `side` and `points`, and `prediction_sides`. This mapping assigns zero-based `edges` indices from the detection JSON to pre-labelled road-side IDs. Set sides from road identity and traversal before scoring. Evaluate nonoverlapping section coverage and keep the matching settings fixed.

The scorer clips predictions and reference to the same window, uses flat-ended lateral tolerance buffers for matching, and claims reference length once. Unassigned predictions in scope count toward predicted length. The JSON reports pooled totals, precision, recall and F1, plus section, road-type and provenance breakdowns. The DXF separates reference, matched, unmatched and missed edges. `meets_numerical_threshold` reports the numeric cutoff. Record the engine revision and verify the independence and coverage of the reference dataset before declaring qualification.


### 7.2 Run a design corridor pilot

Use `road-boundary-engine --corridor-pilot input.json --output fit.json --dxf fit.dxf` for reviewed design corridors. Input contains `sections` with `name`, `axis` (0 for easting and 1 for northing), and ordered `stations` of opposing kerb pairs. Use consistent metre coordinates and station spacing of 0.5 m for the pilot. Each `cases` entry has `name`, `centrelines` and `edges` arrays of line coordinates, with an optional `shift_m` for the lateral control. The DXF shows fit evidence for the first case. The [MGA56 pilot results](road_boundary_engine_test_requirements.md#81-mga56-design-corridor-pilot) document the evaluation settings and scope.
