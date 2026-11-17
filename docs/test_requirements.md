# Test requirements and test cases

Status: acceptance baseline, 14 November 2026. An executable unittest suite now covers parts of these requirements. Case rows define acceptance expectations, not blanket pass claims; see [validation status](validation_status.md) for results and pending qualification.

Scope: the first release converts `.12daz` and `.12da` to 2D DXF 2018. DWG is outside this test baseline. Read alongside [the design](../12daz_design.md).

## Test requirements and traceability

| Requirement | Acceptance requirement | Cases |
| --- | --- | --- |
| TR-01 Input | Recognize supported containers, select an unambiguous payload and reject unreadable or invalid input without publishing output | IN-01 to IN-07 |
| TR-02 Encoding and grammar | Decode supported encodings without character replacement; parse documented nested syntax and resolve model/style context | PA-01 to PA-06 |
| TR-03 Geometry | Preserve source XY, ordering, closure and supported exact curves; output finite, genuinely 2D geometry | GE-01 to GE-08 |
| TR-04 Units and precision | Require resolved units; do not rescale by declaring units; preserve large-coordinate precision | GE-09 to GE-11 |
| TR-05 Approximation | Supported approximate curves meet the chosen XY deviation tolerance or produce an explicit unmet-tolerance diagnostic | GE-12 to GE-14 |
| TR-06 Layers and appearance | Map selected models deterministically, preserve supported styling and disclose substitutions | ST-01 to ST-04 |
| TR-07 DXF and publication | Produce auditable DXF 2018; validate before publishing; protect existing files through failures and cancellation | DX-01 to DX-07 |
| TR-08 Accountability | Reconcile every selected source object, report losses and retain source references | RP-01 to RP-04 |
| TR-09 GUI | Enforce valid states and settings, keep selection consistent with export, remain responsive, and provide offline help with the required copyright | UI-01 to UI-10 |
| TR-10 CLI | Match GUI conversion semantics and return documented exit codes | CL-01 to CL-03 |
| TR-11 Performance | Meet the agreed sample benchmark and resource limits without reducing output fidelity | PF-01 to PF-03 |
| TR-12 Distribution | Run offline on a clean supported Windows x64 machine without Python, AutoCAD or 12d Model | PK-01 to PK-03 |
| TR-13 Expanded geometry (planned) | Qualify expanded string families and reconstruct intersection-point alignments with tolerance-controlled horizontal and vertical curve sampling in the selected output mode | EG-01 to EG-12 |

All cases are required for the enabled first-release feature set, except GE-12 to GE-14 and GE-08's optional symbol/TIN variants, which become required when those features are enabled. An unavailable optional feature must be handled by the unsupported-object path and reported accurately. Marking a required case Not applicable requires a documented scope change; it is not a passing result.

TR-13 and EG-01 to EG-12 are conditional acceptance requirements for the expanded geometry plan in the design, not claims of implemented or tested support. They become mandatory for each enabled source variant and output mode. Existing zero-Z requirements apply to Flatten 2D; planned Keep Z output must instead satisfy the XYZ/profile assertions below. Keep Z does not permit nonzero thickness or unverified source elevations.

## Fixtures and expected results

Use small, shareable synthetic `.12da` fixtures derived from the documented grammar, plus compressed equivalents. Commit only fixtures with redistribution permission. Keep private surveys in ignored `sample/`; record their hashes in local test evidence. Never use generated mockup values as test expectations.

Each fixture manifest must specify its ID, provenance, archive version, encoding, model names, selected source counts, geometry definitions, units, expected output types/counts/layers, expected warnings and expected outcome. Record optional feature flags explicitly. Manually or analytically establish expectations before testing; do not generate expected geometry through the converter under test.

Fixture groups:

- F-INPUT: plain and ZIP files, UTF-16 LE/BE and UTF-8 BOMs, legacy no-BOM encoding, Unicode names, invalid/corrupt archives and small resource-limit fixtures.
- F-GEOMETRY: isolated primitives, mixed Z, arc directions, closure, text, null values, degenerate and invalid geometry, large coordinates and units.
- F-STYLES: model/layer collisions, forward style references, mixed colours and missing styles/fonts.
- F-MIXED: supported, approximated and unsupported source objects with independently enumerated counts.
- F-EXPANDED: verified 2D/3D/4D strings, super strings, polylines, existing contours, pipe centrelines and solved/intersection-point super alignments. Include documented arc, spiral and parabolic vertical-curve definitions, independent chainage-position/profile references and per-mode expected results. Record the fourth component's verified meaning, curve conventions and source-definition precedence.
- F-SURVEY: the local sample identified in the design, supplemented with an authorized reference DXF when available. Its object inventory and expected counts must be established before it can be an acceptance fixture.

## Numerical and output assertions

- Exact XY geometry: compare against independent analytic expectations using absolute tolerance `max(1e-8 drawing units, 8 * ulp(expected coordinate))`. Do not use a loose relative tolerance that hides errors at survey coordinate magnitudes.
- Output Z, elevation and thickness: require exactly zero for all supported entity properties, including text insertion/alignment and block content if blocks are introduced. Require standard +Z extrusion where applicable.
- Arc tests: compare centre, radius, endpoints and signed sweep/direction; do not rely on visually similar bounds. Compare text rotation modulo a full turn.
- Approximate curves: verify maximum XY deviation using an independent analytic bound where available. Numerical sampling alone is insufficient to claim a guaranteed maximum; document the bound or unresolved evidence limitation.
- Reconcile counts at source-object level: `selected = exact + approximated + skipped`, where categories are mutually exclusive. Record partial conversion as approximated with a loss diagnostic, not exact. Record emitted DXF entity counts separately.
- Repeated runs: compare canonical entities, layers, settings and diagnostics. Ignore volatile handles, timestamps, job identifiers and serialization ordering; byte-identical DXF is not required.
- Reopen every acceptance DXF and audit it; unapproved audit findings or automatic repair requirements fail acceptance. Use an independent CAD viewer/application for release interoperability evidence as well.

## Input and parser cases

| ID | Setup / action | Expected result |
| --- | --- | --- |
| IN-01 | Convert equivalent plain and single-payload ZIP fixtures | Same canonical geometry, layers, units and diagnostic classifications |
| IN-02 | Supply empty file, missing path, unreadable file, unsupported container and mismatched extension | No crash; content is identified by bytes; unsupported content fails clearly; no output published |
| IN-03 | Read truncated ZIP, damaged member data or CRC error | Integrity failure, no success state or published partial drawing |
| IN-04 | Supply no payload, multiple candidate payloads and an encrypted archive | Each rejected with a specific actionable diagnostic |
| IN-05 | Include one payload plus unrelated entries and path-traversal member names | Valid payload handled according to policy; unrelated entries reported; no archive-controlled filesystem writes |
| IN-06 | Cross decompressed-byte, member-count, nesting, token-length and entity-count limits; test exactly at each limit | At-limit valid input accepted; limit+1 rejected; actual bytes counted; controlled failure and cleanup |
| IN-07 | Convert from Unicode/space-containing input and output paths; attempt output equal to input | Valid paths work; input overwrite rejected; source hash unchanged |
| PA-01 | Decode UTF-16 LE/BE and UTF-8 BOM fixtures containing non-ASCII model names/text | Names and text preserved exactly, without replacement characters |
| PA-02 | Supply no-BOM legacy fixture with explicit encoding and invalid encoded bytes | Valid explicit encoding succeeds; undecodable input fails with location/context |
| PA-03 | Parse comments, quoted braces, escaped quotes, nested blocks and documented numeric forms | Correct token boundaries and independently expected values |
| PA-04 | Define styles after their use and switch model context repeatedly | Correct resolved styles and source model assignment |
| PA-05 | Place supported entities before/after an unknown safely delimited object | Unknown content reported; supported neighbours retained and counted |
| PA-06 | Supply unterminated quote/block, invalid structure and unrecognized archive version | Malformed input fails; unverified version receives an explicit compatibility diagnostic and follows a documented support policy |

## Geometry and styling cases

| ID | Setup / action | Expected result |
| --- | --- | --- |
| GE-01 | Export known points and straight segments with positive/negative Z | Expected XY within numerical tolerance; output Z zero |
| GE-02 | Export open and closed strings, including duplicate end vertices | Ordering/closure preserved; no unintended joining or filling |
| GE-03 | Export clockwise/counterclockwise arcs, major arcs and full circles | Correct centre, radius, endpoints and direction without unintended short-arc substitution |
| GE-04 | Export mixed line/arc super strings for enabled segment variants | Continuous expected geometry and correct bulge signs; unsupported variants disclosed |
| GE-05 | Export text with nonzero Z, rotation, alignment and Unicode content | Supported placement/content retained in XY; all relevant Z fields zero; substitutions reported |
| GE-06 | Provide null Z, null XY, NaN/infinity and nonpositive radii | Documented null Z handled; invalid XY/geometry rejected or skipped with source diagnostic; no invalid DXF values |
| GE-07 | Export zero-length segments and vertical features collapsing in XY | Documented point/skip policy applied consistently; degeneracy reported; no invented geometry |
| GE-08 | Include symbols, mesh and complex civil types | Supported mapping exact; unsupported types reported. If enabled, symbol fallback reported and TIN shared edges emitted once; TIN off by default |
| GE-09 | Use coordinates near E=500000, N=6200000 with small offsets | Offset detail preserved within stated tolerance; no survey-coordinate shift from preview origin |
| GE-10 | Select metres versus millimetres as unit declaration for identical numeric input | Correct metadata; identical numeric geometry; no implicit factor of 1000 |
| GE-11 | Omit source units; try export, resolve units, then reload input | GUI blocks until resolved; CLI fails clearly; new input cannot inherit a stale unconfirmed decision |
| GE-12 | Enable approximation with several tolerances on independently known curves | Error bound met in drawing units; tighter tolerance does not worsen measured deviation |
| GE-13 | Project a tilted circle or supported non-circular alignment | Correct projection or reported approximation; never a false XY circle; unsupported variants skipped explicitly |
| GE-14 | Request invalid tolerance or force segment cap below required resolution | Invalid settings rejected; unmet tolerance clearly diagnosed; no unqualified exact-success claim |
| ST-01 | Select subset of models and use names colliding after sanitation/case folding | Only selected models exported; unique valid stable layer names and reversible report mapping |
| ST-02 | Use distinct entity colours within one model | Supported overrides survive layer mapping |
| ST-03 | Reference missing line style, font and symbol definitions | Documented fallback and source-linked diagnostic for each affected category |
| ST-04 | Convert same fixture twice with same options | Canonical geometry, layer names and diagnostic totals reproducible |

## Planned expanded geometry cases

Use independent analytical results or reference geometry produced outside the converter. Declare plan and elevation tolerances in drawing units before execution. For sampled curves, verify maximum plan deviation and, in Keep Z mode, elevation error at corresponding chainages using an independent bound where available. A midpoint-only check or dense sampling alone cannot establish a guaranteed maximum; record any unresolved evidence limitation and do not claim bounded-accuracy acceptance without supporting evidence.

| ID | Setup / action | Expected result |
| --- | --- | --- |
| EG-01 | Convert each qualified 2D, 3D and 4D string variant and polyline, including open/closed, reversed, repeated-vertex and intentional-gap cases | Ordering, breaks and closure preserved; Flatten 2D has zero Z; Keep Z preserves verified XYZ, including vertical segments. Fourth component is interpreted only by its verified semantics, with metadata mapping or omission reported; malformed records diagnosed |
| EG-02 | Convert mixed straight/arc/spiral super strings with segment colours and attributes | Correct segment joins, direction and supported styling; exact circular representation retained where valid; non-circular segments meet tolerance; omitted attributes reported |
| EG-03 | Convert existing open/closed contours at positive, negative and stacked elevations | Expected contour topology and XY retained; Keep Z preserves contour elevations; Flatten 2D has zero Z; no surface-derived contours invented |
| EG-04 | Convert qualified pipe objects with varying invert/centreline definitions and missing elevations | Centreline follows the documented source elevation convention; no assumed invert-to-centreline offset; unresolved required Keep Z elevations skip the whole object; omitted diameter/connectivity/civil attributes reported |
| EG-05 | Convert intersection-point super alignments with straights and clockwise/counterclockwise circular arcs | Reconstructed endpoints, centres, radii, tangent points, signed sweeps and positions at independent reference chainages match; intersection points are not exported as a substitute chord path |
| EG-06 | Convert each qualified transition-spiral family in forward/reversed direction, including short transitions and transitions between straights/arcs | Independent chainage positions and tangents match verified source conventions; required joins are continuous; sampled plan deviation meets tolerance |
| EG-07 | Combine horizontal straights/arcs/spirals with crest/sag parabolic vertical curves and grade changes | Keep Z matches independent elevations and grades at the same chainages, including vertical-curve boundaries and extrema; intermediate vertices capture a curved profile even along a straight plan segment; Flatten 2D retains horizontal geometry and reports vertical omission |
| EG-08 | Export the same curves at several plan/elevation tolerances and drawing-unit declarations, including large survey coordinates | Independent error checks meet each requested tolerance; tighter settings do not worsen measured error; no coordinate rescaling from unit declaration; endpoints, curve boundaries and required extrema retained |
| EG-09 | Supply missing/contradictory parameters, unknown curve families, missing Keep Z elevations and conflicting solved/intersection-point definitions | Documented precedence and consistency checks applied; unresolved objects skipped whole with specific source-linked diagnostics; no invented zero elevations, false chords or joins across missing segments |
| EG-10 | Force subdivision/vertex limits, invalid tolerances and cancellation during sampling | Invalid settings rejected; limits produce whole-object skips with unmet-tolerance diagnostics; tolerance never silently relaxed; cancellation meets PF-02 and DX-05; no partial object published |
| EG-11 | Reopen/audit output and independently inspect representative expanded fixtures in CAD | Planar sampled curves use expected LWPOLYLINE representation; varying-Z output uses 3D POLYLINE; actual WCS vertices, closure, connectivity and profiles match independent references within tolerance; exact arcs/bulges remain correct; clean audit alone is insufficient |
| EG-12 | Run equivalent GUI/CLI exports, change mode/tolerance, simplify preview and inspect reports for mixed supported/unsupported objects | Options propagate consistently; stale preview/summary invalidated; preview simplification does not change export; report includes mode, tolerance, error method/bound or estimate, vertex counts, omitted attributes and source-linked skips; selected = exact + approximated + skipped; warning exit status applied |

For TR-13 qualification, retain per-variant fixture provenance, source parameter conventions, independent reference calculations, error-verification method and CAD application/version with the result records. An unsupported variant must continue to use the reported skip path until its applicable cases pass.

## Output, reporting and CLI cases

| ID | Setup / action | Expected result |
| --- | --- | --- |
| DX-01 | Write and reopen every supported entity fixture | DXF 2018, expected model-space content/units, clean audit and zero-Z assertions pass |
| DX-02 | Open reference drawings in an independently selected CAD application | Geometry, text, colours, layers and extents match reviewed references; record application/version |
| DX-03 | Export to existing output; decline overwrite, then approve | Decline preserves existing hash; approved successful export replaces only intended destination |
| DX-04 | Inject permission denial, full-disk/write failure and validation failure before publication | Clear failure, existing output unchanged, no final partial file; job-owned temporary files cleaned |
| DX-05 | Cancel during scan, geometry processing and DXF writing before publication | Cancelled outcome, source/existing destination unchanged, worker stops and temporary files cleaned |
| DX-06 | Fail report save after a successful DXF publish | DXF success distinguished from report failure; output retained; visible warning and CLI warning status |
| DX-07 | Supply empty/unsupported-only selection and unselected source models | GUI blocks empty convertible selection; CLI gives non-success diagnostic without publishing an empty success drawing |
| RP-01 | Convert mixed exact/approximate/unsupported fixture | Source reconciliation equation holds; output count independently correct; all losses traceable |
| RP-02 | Split one source object into several DXF entities | Source count remains one; emitted count reflects actual entities |
| RP-03 | Generate repeated warnings beyond detail limit | Bounded detail storage; complete aggregate totals; truncation disclosed |
| RP-04 | Inspect saved JSON report | Valid schema/version, input hash, options, units, mappings, bounds, counts, timing and source references present |
| CL-01 | Run equivalent GUI and CLI options on one fixture | Canonical output and diagnostic classifications match |
| CL-02 | Exercise clean success, warning success, failure and cancellation | Exit codes 0, 2, 1 and 3 respectively; report-save warning returns 2 after DXF success |
| CL-03 | Use invalid arguments and existing output without overwrite flag | Actionable message and non-success status; no prompt in unattended use and no accidental replacement |

## GUI, performance and package cases

| ID | Setup / action | Expected result |
| --- | --- | --- |
| UI-01 | Launch, load, export, complete, fail and cancel | Valid state transitions; controls/actions enabled only when appropriate; no duplicate concurrent export |
| UI-02 | Search/check/uncheck models and change export options | Preview/export selection agrees; stale summaries invalidated; search does not silently alter hidden selections |
| UI-03 | Reload a different input during/after a job | Old results cannot overwrite new state; job identities respected; settings requiring confirmation reset |
| UI-04 | Pan/zoom/fit preview with large coordinates; activate simplification | Readouts consistent; simplification labelled; exported geometry unchanged |
| UI-05 | Test keyboard navigation, 100/150/200 percent scaling and minimum window size | Essential controls reachable and legible; warning meaning available without relying on colour |
| UI-06 | Export with warnings and inspect report/completion actions | Skips/approximations visible; report and correct output folder open; no false warning-free success |
| UI-07 | Open Help > About with no input and during conversion; copy notice; close using button and Escape | Application version/build and MIT identification visible; exact notice `Copyright (c) 2026 Lambda Technologies Pty Ltd ABN 22 647 165 160` selectable/copyable; no job or option changes; notice wraps without clipping at tested DPI settings |
| UI-08 | Open Help > User Guide and Help > License and Third-party Notices on offline packaged machine | Bundled guide, full MIT license with the same company/ABN notice, and dependency notices accessible by keyboard and mouse; no network required |
| UI-09 | Start normal GUI and skip/allow startup splash; invoke CLI, worker and packaged smoke paths | Splash auto-dismisses after about 1.8 seconds; Continue/Enter/Escape bypass it; no visible splash in noninteractive paths; startup errors remain detectable |
| UI-10 | Open support from splash and About; leave app idle without clicking; inspect README | All support links use https://paypal.me/12dazapp; browser opens only on explicit action; no embedded payments; support remains optional and MIT licensing is unchanged |
| PF-01 | Benchmark local sample on recorded reference Windows x64 machine | Provisional target: export within 120 seconds and total process-tree peak memory below 2 GiB; counts/fidelity unchanged |
| PF-02 | Cancel large scan/export and monitor worker/GUI | Acknowledgement within two seconds; UI remains interactive; cleanup time recorded separately; no orphan worker |
| PF-03 | Load large synthetic input, then repeat load/export/cancel cycle ten times | Preview stays bounded; resource limits respected; no orphan files/processes or growing retained memory after idle cleanup |
| PK-01 | Install/run release artifact on clean supported Windows x64 VM offline | Open, preview and export work without Python, 12d Model or AutoCAD installed |
| PK-02 | Launch from non-admin account with spaces/Unicode paths | No unexpected elevation; bundled resources found; user-writable output locations work |
| PK-03 | Inspect distribution and monitor network during conversion | Required runtime/dependency notices included; no conversion network traffic; private samples/secrets absent from package |

Cancellation boundary: the service must check cancellation immediately before atomic publication. Once publication commits, report completion rather than claiming cancellation reversed it. DX-05 must inject cancellation before that boundary and separately verify the after-commit outcome.

## Execution and evidence

Implemented tooling: unittest for parser/geometry/export cases, withdrawn Tk windows and spawned workers for GUI cases, and mock failure injection for publication checks. Windows CI is configured to run the suite and package the application. Process-tree resource monitoring and independent CAD review remain release qualification work. Keep independent CAD review and clean-machine installation as explicit release jobs.

Run focused tests with each implementation change. Run the full automated suite before release and when parser, coordinate handling or dependency changes can affect broad behaviour. Store release evidence outside ignored conversion-output folders, for example `docs/test-results/<version>/`, excluding private geometry and sensitive paths.

Each result record must contain case ID, requirement ID, application commit/build, fixture hash, enabled features, OS/dependency versions, steps, expected/actual result, Pass/Fail/Blocked/Not applicable status, diagnostic evidence, and defect reference where needed. Benchmarks also record CPU, RAM, storage, timing boundaries and measurement method. A blocked or unexecuted case is never a pass.

## Release gates

1. Freeze supported source variants, units policy, encodings and resource-limit settings, with fixture coverage for each advertised capability.
2. Pass all applicable required cases and automated checks on the release artifact. Optional enabled features must pass their conditional cases.
3. Resolve defects causing coordinate shifts, nonzero output Z in Flatten 2D, incorrect elevations in enabled Keep Z output, incorrect geometry, unmet advertised approximation tolerances, silent loss, input/output corruption or crashes before release. Record remaining lower-impact issues and acceptance rationale.
4. Complete independent CAD review and clean-machine offline installation checks.
5. Confirm source-object reconciliation for the survey benchmark and publish measured performance. Agree or revise provisional PF targets before assessing release; do not relabel a failed run as passed by changing thresholds afterward.
6. Publish the tested support matrix, known limitations and evidence summary. DWG support is not a release gate.
