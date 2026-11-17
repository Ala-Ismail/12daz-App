# Validation status - 0.1.3

Date: 14 November 2026. This is an initial working build, not a fully qualified release. Results below distinguish executable tests from the broader acceptance cases in [test_requirements.md](test_requirements.md).

## Implemented

- ZIP/plain archive reader, Unicode decoding, resource limits and balanced streaming parser.
- Source model selection, supported straight/circular plan geometry and basic text conversion.
- DXF 2018 writer, layer/style mapping, zero-Z checks, reopen/audit validation and atomic publication.
- CLI inspect/convert commands with explicit units, reports, overwrite control and warning exit codes.
- Tk/ttk GUI with model search/selection, pan/zoom preview, separate worker process, cancellation and reports.
- Offline Help, About and bundled licenses with the requested company/ABN notice.
- Windows executable folder/ZIP, pinned build environment and CI workflow.

Tk/ttk replaces the proposed PySide6 GUI. The development environment blocked package downloads; its installed Python 3.11 already supplied Tk, ezdxf and PyInstaller. The layout follows the original concept while using standard Tk widgets. Tests use unittest instead of the proposed pytest/pytest-qt stack.

## Automated tests

Command: `python scripts/test.py`, using Python 3.11.5, ezdxf 1.3.5, Tk 8.6 and Windows x64.

**77 tests pass.** See [test_core.py](../tests/test_core.py) and [test_gui.py](../tests/test_gui.py). They use synthetic data authored for this project. Method names reference acceptance case IDs; a passing method may cover only part of a multi-scenario acceptance case.

| Group | Evidence executed | Remaining qualification |
| --- | --- | --- |
| TR-01 Input | Plain/ZIP equivalence, invalid/missing input, corrupt ZIP, ambiguous payload, no extraction, input identity, Unicode paths and resource limits | Encrypted/CRC-damaged archive variants and exact boundaries for every resource limit |
| TR-02 Parser | Unicode BOMs, legacy override, strict decoding, tiny chunk boundaries, comments/quotes, model blocks, malformed structures and unsupported neighbours | Broader archive/version fixtures and external style-definition support |
| TR-03/TR-04 Geometry/units | Points, closure, large XY, signed minor/major circular segments, legacy data, text, null Z, invalid geometry, collapse and units without scaling | Independently reviewed reference exports, more text variants and tilted source curves |
| TR-05 Approximation | Unsupported transition objects skipped whole | Approximation not enabled; GE-12/GE-14 conditional future work |
| TR-06 Styling | Selected models, layer sanitation/collisions, segment colours, substitutions and reproducibility | External/custom definitions and CAD visual review |
| TR-07 Output | DXF version/audit, overwrite prevention, injected write/audit/report failures, changed input, pre-publication cancellation and unsupported-only selection | Real full-disk/permission scenarios and independent CAD review |
| TR-08 Reporting | Source/output count reconciliation, bounded details, aggregate totals, report fields and hashes | Formal external JSON schema and broader inventories |
| TR-09 GUI | Spawned scan/export, units gating, selection/search, reload invalidation, zoom invariance, cancellation, selectable About notice and offline resources | Manual keyboard/DPI/minimum-size review and additional post-commit race scenarios |
| TR-10 CLI | Exit codes and direct service versus GUI-worker conversion agreement | Additional invalid-argument and unattended subprocess scenarios |
| TR-11 Performance | Direct sample timing and native Windows process memory counters | GUI/process-tree peak, repeated-job retention, RAM/storage specification and measured cancellation latency |
| TR-12 Distribution | PyInstaller build, frozen-app synthetic scan/export/audit and Help smoke test | Clean offline VM without Python/CAD, non-admin and network monitoring qualification |

The suite tests supported base geometry. It does not certify all 12d variants, civil semantics or styling fidelity.

## Local survey run

Source SHA-256: `6334f436cc1cc78c5773b7f6cd366f0cfcf3787ef823b2fd8cc02ed261edf65c`.

Private input stays in ignored `sample/`; generated drawings and detailed reports stay in ignored `outputs/`. No survey geometry is included in tests or the distribution.

| Observation | Result |
| --- | --- |
| Container / encoding | Single-payload ZIP / UTF-16 LE BOM |
| Archive version | 15.01.08.60 |
| Models | 17 |
| Source objects | 536: 455 super strings, 61 super alignments, 20 drainage objects |
| Converted source objects | 500; classified partial/approximated because of omitted metadata/labels or style substitutions |
| Skipped objects | 36: 20 drainage objects and 16 alignments with empty solved vertex data |
| Emitted DXF entities | 599 |
| Conversion time | 5.730 seconds including save/reopen/audit |
| Peak working set | 65,863,680 bytes, Windows GetProcessMemoryInfo |
| Peak process commit | 693,821,440 bytes, Windows GetProcessMemoryInfo |
| Host | Windows build 26300, AMD64, Intel Family 6 Model 186, Python 3.11.5 |
| Outcome | Published DXF 2018; clean reopen/audit and supported zero-Z/elevation/thickness/extrusion checks |

This measurement excludes the GUI and worker process tree. It is not the PF-01 release benchmark: RAM/storage are not recorded and independent source-fidelity review is outstanding. Exact supported arcs are written as bulges even when the source object is classified partial due to omitted labels/styles. Preview bounds are display-sampled, not certified exact curve bounds.

Units were explicitly set to metres for this run, not inferred from coordinates. Independently enumerated source-type totals agree with the scanner, but geometry has not been compared against an authorized reference CAD export.

Reproduce with:

```powershell
python scripts/benchmark.py "sample/your-file.12daz" --output "outputs/sample_2D.dxf" --units metres --evidence "outputs/benchmark.json"
```

## Packaged smoke test

Artifact: `dist/12dAZ-Converter-0.1.2/12dAZ-Converter.exe`. Portable ZIP: `dist/12dAZ-Converter-0.1.2-Windows-x64.zip`.

The frozen executable returned exit code 0 with `passed: true` for Tk startup, spawned archive scan, spawned DXF export/audit and offline Help/About/license resources. It processed synthetic circular geometry without using a Python command to launch the application. Local evidence for the current build is `outputs/packaged-smoke-0.1.2.json`.

This ran on the development machine, which has Python installed. It demonstrates frozen runtime/resource handling, not clean-machine qualification. The earlier unversioned folder was preserved because its executable was running; use the versioned artifact for the updated implementation.

## SignPath preparation checks — 15 November 2026

The application suite was rerun with Python 3.11.5 and the installed pinned build stack: **79 tests passed**, including two Windows VERSIONINFO checks. `scripts/build.py` produced the Windows package with the expected product name, company, original filename and file/product version in the actual executable. Its packaged smoke test passed; local evidence is `outputs/signpath-preparation-smoke.json`.

Both release workflows passed actionlint. The SignPath artifact XML validated against SignPath's published `v1.xsd`. Inline PowerShell and the packaging helper parsed successfully. Local checks exercised 144 provider/result combinations from the actual release expression, plus signing exclusions for PRs, forks, feature branches, untagged requests, cancellation and failed builds. Missing SignPath settings and conflicting providers were rejected. The real unsigned executable, with valid product metadata, was rejected before producing any purported signed distribution.

The packaging helper's downstream success path was also exercised with a test double only at the signature-verification boundary: the real packaged application smoke test, complete bundle ZIP round-trip and regenerated checksum passed. No certificate was created, trusted or used for this check; it is packaging evidence, not a successful signed release.

These checks do not establish SignPath approval, server-side configuration, a valid publisher/timestamp success or live signing. No signing request or new release was submitted. The regenerated local v0.1.3 build is not a replacement for the published v0.1.3 assets; a future signed release requires a new version/tag and the pending manual qualification.

## Deferred scope and release checks

DWG, TIN/mesh export, transition approximation, custom symbols, rendered labels, external style definitions, paper/screen-size text and text offset/raise placement remain unsupported or deferred. Unsupported curves are reported rather than replaced by false straight segments. Drag-and-drop input is not implemented. The GUI excludes unsupported-only models by default; CLI inspection reports all source objects.

Flatten 2D drainage qualification reopened and audited 103 zero-elevation pipe polylines from 51 local drainage strings. The other 11 strings had no pipe blocks. Drainage Keep Z remains unsupported pending verification of the centreline elevations.

Complete the remaining applicable checks above, independently review reference drawings and meet the release gates before calling this a qualified release. The 77 passing executable tests do not imply all 59 acceptance-case rows have passed in full.

## Preview correction in 0.1.1

Reproduced a preview bug where `RD 11 DES STR` consumed the entire 30,000-point allowance and all later models had no preview geometry. The collector now rebalances display allowances whenever a model is encountered, reducing displayed paths/points when necessary. Exact DXF primitives are not modified. Model selection also fits the view to the selected geometry.

The local sample now represents all 16 convertible models within the preview cap (4,801 retained display points in the recorded run). Regression tests cover a large first model followed by later models, aggregate point limits, endpoint preservation, unchanged DXF vertex counts and selection/fit in the GUI. Sample evidence is in ignored `outputs/preview-regression.json`. The conversion benchmark above was recorded with 0.1.0; this preview-only correction does not change its geometry results.

## GUI and selection-based preview in 0.1.2

The GUI now follows the design proposal's bordered three-panel structure with a dark grey theme requested by the user. It includes code-drawn icons and application branding, blue checkbox imagery, alternating model rows, a zoom percentage selector, a graduated scale bar, a prominent Export DXF action and a status strip with warning/report controls. Input encoding is in a collapsible Input options section. Settings scroll on smaller windows; export and status controls remain available. The Windows title bar and Help/About dialogs use the dark theme as well.

The current [GUI screenshot](assets/12daz-converter-gui-v0.1.2.png) was captured from the application using project-authored synthetic survey geometry. It contains no private survey geometry. The layout was visually inspected at 1440 x 900 and 1080 x 720; full DPI and accessibility qualification remains pending.

Model selection changes now trigger a debounced background inspection restricted to selected models. The entire 30,000-point display allowance is shared among those selected models. The previous preview remains available during regeneration, stale worker results are ignored, and only the latest selection is applied. The full model inventory remains intact. Export still uses full supported geometry, independently of preview sampling.

On the supplied private sample, selecting only RD 11 DES STR increases its retained preview from 1,875 to 30,000 points while preserving all 17 models in the inventory. Local evidence is `outputs/selected-preview-validation.json`. Four additional tests cover full allowance recovery, rapid selection changes during a refresh, resetting zoom to fitted extents, and opening new input while a preview refresh is pending. The complete suite now passes 69 tests.

## Startup splash and optional support in 0.1.3

A dark grey startup splash now appears on normal GUI launch. It closes automatically after about 1.8 seconds and can be skipped with Continue, Enter or Escape. Background conversion workers, CLI commands and packaged smoke tests do not display it. The hidden smoke test constructs and dismisses a splash to verify its bundled code.

The splash, Help > About and the README all use the owner's supplied public support URL: https://paypal.me/12dazapp. The buttons explicitly identify PayPal. Support remains optional; conversion stays free and no browser or payment page opens automatically. No payment processing is embedded in the app.

The full local suite passed 77 tests. Focused support tests also passed after wiring the final URL, and the frozen v0.1.3 executable passed the expanded smoke test covering splash dismissal, spawned conversion, DXF audit, offline help and an enabled support button without opening a browser. Local evidence is outputs/packaged-smoke-0.1.3.json. The splash was also visually inspected. Published v0.1.2 artifacts are unchanged; v0.1.3 is a new build.
