# 12dAZ Converter design

Status: design baseline with initial version 0.1.2 implemented, 14 November 2026. See [validation status](docs/validation_status.md) for actual evidence and outstanding release checks. Some options described below remain future scope.

## Implemented scope in version 0.1.2

The working implementation uses Tk/ttk, a spawned conversion worker and ezdxf. It exports supported base plan geometry to DXF 2018 with explicit user-selected units. Model selection, preview, overwrite checks, cancellation, reports and Help/About are implemented. Curve approximation, TIN edges, symbol mapping, automatic unit metadata resolution and drag-and-drop are deferred. See [validation status](docs/validation_status.md) for test/build evidence; sections below also describe intended future behaviour.

## Purpose and scope

Build a standalone, offline Windows desktop application that reads a `.12daz` file and writes an editable **2D DXF** drawing. Installation of 12d Model, AutoCAD or a Python runtime must not be required on the user's computer. Uncompressed `.12da` input is also supported through the same parser.

DWG is a future enhancement and introduces no dependency in the first release. The initial application is a converter with a preview, not a CAD editor. It exports model-space geometry; it does not recreate layouts, civil object intelligence or the complete 12d rendering system.

## Platform decision: Python versus Rust

Use **Python** for the first release, with Tk/ttk for the GUI and ezdxf for DXF creation. Implementation uses the standard Tk toolkit because PySide6 downloads were unavailable; the original generated mockup remains a visual reference. Package a Windows x64 application with its runtime and required libraries. Validate packaging early on a clean machine.

| Consideration | Python | Rust |
| --- | --- | --- |
| DXF implementation | ezdxf provides a documented entity-writing API | A crate or native adapter needs a separate capability evaluation |
| Parser development | Fast iteration against varied archive fixtures | More implementation effort, with strong compile-time checks |
| CPU and memory control | Requires streaming and careful object allocation | Strong option for intensive parsing and geometry processing |
| Desktop distribution | Bundle runtime and Qt dependencies | Native core still needs a GUI and its dependencies |
| Proposed role | Complete initial application | Optional future parser/geometry accelerator after profiling |

This is an engineering recommendation, not a measured performance comparison. Avoid maintaining two implementations initially. Keep parser and geometry interfaces separate so measured bottlenecks can later move to Rust without replacing the GUI or DXF writer.

The ezdxf documentation explicitly supports creating DXF documents and explicitly excludes native DWG conversion: [ezdxf introduction](https://ezdxf.readthedocs.io/en/stable/introduction.html).

## Available sample and format evidence

The local sample is `sample/1.7 SHRP-SMEC-WGI-RBT-M3D-COMBINED.12daz`.

Initial inspection only, without a geometry parse:

- Compressed file size: 3,915,027 bytes.
- ZIP signature: `50 4B 03 04`.
- Contains one entry: `SHRP-SMEC-WGI-RBT-M3D-COMBINED.12da`.
- Declared uncompressed size: 88,923,538 bytes.
- Payload starts with a UTF-16 little-endian BOM and `null null`, followed by archive comments.

These observations validate this sample's container and encoding, not universal compatibility. Real model names, supported string types, counts, colours, units and export fidelity still require a parser and inspection. Numbers and geometry in the GUI image are illustrative.

Use the published archive grammar rather than inferring syntax solely from examples. The vendor documentation describes archive versions, commands, attributes and string types: [12d documentation](https://www.12d.com/learningtools/12d_Documentation.html) and [V15 archive specification](https://downloads.12dmodel.com/v15/12d_Model_15_C1e/12d_a_File_Format_V15_C1e.pdf). Autodesk also describes `.12daz` as zipped `.12da`: [12d extension release notes](https://up1.autodesk.com/2022/CIV3D/Autodesk_12d_Extension_2022_ReleaseNotes.htm).

## Startup and optional support in version 0.1.3

Normal desktop startup shows a brief dark grey splash that closes automatically after about 1.8 seconds. Continue, Enter and Escape skip it. CLI, worker and smoke-test paths do not show the startup screen. The splash and Help > About offer a voluntary Buy us a coffee (PayPal) button linking to https://paypal.me/12dazapp. The README includes the same link. Payment pages open in the browser only when clicked; the converter remains free, MIT licensed and does not process payments.

## Current implemented GUI

Version 0.1.2 keeps the proposal's three-panel layout and uses the requested dark grey theme with blue actions. The original concept below remains a reference. Unsupported settings, including curve tolerance, are not exposed as functioning controls.

![Current dark grey desktop GUI using synthetic data](docs/assets/12daz-converter-gui-v0.1.2.png)

The preview shares its 30,000-point allowance across selected models. Checkbox changes trigger a debounced background rebuild, with obsolete results discarded. Selecting one model makes the full allowance available to that model; full-geometry export remains independent of display sampling.

## Proposed GUI

![Proposed 12dAZ Converter desktop interface](docs/assets/12daz-converter-gui-proposal.png)

The generated image proposes the layout and visual direction. Behaviour below is the implementation specification; image details are not evidence of implemented features.

Use a resizable three-pane desktop window, initially around 1440 x 900 pixels, with a practical minimum size and DPI-aware scaling. Light control panels surround a dark plan preview. Blue indicates the primary action, amber indicates warnings, and status messages always include text.

| Area | Controls and behaviour |
| --- | --- |
| Menu bar | Help: User Guide, About 12dAZ Converter, License and Third-party Notices |
| Top toolbar | Open file, current filename, replace input, application settings |
| Left: Models | Searchable list, entity counts, select all/none, model export checkboxes, colour indicators |
| Centre: 2D preview | Selected geometry, pan, zoom, fit extents, scale and coordinate readout |
| Right: DXF export | Output path, DXF version, units, layer policy, preserve arcs, approximation tolerance, Export DXF |
| Bottom | Current stage, progress, Cancel during work, warning count, View report, Open output folder after completion |

Model checkboxes control both export and preview in version one. Preview simplification is allowed for speed but must be labelled when active; it must not alter exported geometry. North means source +Y unless orientation information is known. Display a scale only when units are resolved.

### Help menu and copyright

Add a standard **Help** menu above the toolbar, accessible by mouse and keyboard:

- **User Guide**: open bundled offline instructions for opening files, selecting models, resolving units, exporting DXF and reading warnings.
- **About 12dAZ Converter**: open a dialog showing the application name, installed version/build, purpose (`12daz / 12da to 2D DXF`), MIT license identification and the copyright notice below.
- **License and Third-party Notices**: open the bundled full MIT license and dependency notices without requiring a network connection.

Display this notice in the About dialog as selectable, copyable text:

```text
Copyright (c) 2026 Lambda Technologies Pty Ltd ABN 22 647 165 160
```

Use the same notice in the bundled LICENSE. Keep Help available when no file is loaded and while a conversion runs. Opening help must not change conversion options or job state. The About dialog must have an accessible Close button and support Escape to dismiss. Allow notice text to wrap at smaller window sizes and high DPI without truncating the company name or ABN.

The existing GUI concept image predates this menu; the Help menu and About dialog specified here are required in the implemented GUI.

### User workflow and states

1. Open or drag in a `.12daz` or `.12da` file.
2. Validate the container and encoding, scan models and supported entity counts, then populate a plan preview.
3. Review selected models, skipped types and warnings. Resolve units if absent from the source.
4. Choose output path and conversion options. Export selected models into one DXF.
5. Display written, approximated and skipped counts, with a link to the report and output folder.

States: Empty, Loading, Ready, Exporting, Complete, Failed and Cancelled. Disable export while loading, with no selected convertible geometry, with unresolved required settings, or with an invalid output path. Show indeterminate progress when a reliable percentage is unavailable. Prompt before replacing an existing output file.

Changing input invalidates previous scan and preview state. Changing selection or export options invalidates the export summary. Completion with skipped source objects is visibly reported as completion with warnings. A structural parse error prevents publishing a misleading partial drawing.

## Architecture

```text
Tk/ttk GUI / command-line entry point
                  |
           Conversion service
                  |
Container reader -> Decoder -> Tokenizer/parser -> Source entities
                                                    |
                                       Selection and style mapping
                                                    |
                                         2D geometry conversion
                                           /                \
                                   Preview geometry      DXF writer
                                                            |
                                                 Validation and publish

Each stage emits progress, diagnostics and source references.
```

The GUI never parses input directly. Run conversion in a worker process, with progress and cancellation messages, to keep the UI responsive and permit reliable resource cleanup. Pass paths, settings and compact preview data between processes; avoid copying the complete geometry graph through IPC.

Suggested package structure:

```text
src/az12_converter/
  app.py             # GUI entry point
  cli.py             # automation entry point
  service.py         # job orchestration
  archive.py         # container and encoding handling
  parser/            # tokenizer, grammar and supported entity adapters
  geometry/          # source types, flattening and curve approximation
  export/dxf.py      # ezdxf backend
  preview/           # render-ready geometry and spatial filtering
  ui/                # Tk/ttk widgets and state
  diagnostics.py     # report schema and warning codes
tests/               # parser, geometry, export and integration fixtures
```

Core conversion code must not import GUI widgets. Pin tested dependencies in project metadata and a lockfile. Review Python, Tcl/Tk and dependency distribution obligations before release.

## Input and parser design

- Detect container bytes instead of trusting the extension. For version one, accept validated ZIP `.12daz` and plain `.12da`; explicitly reject other container types until supported by verified fixtures.
- Read archive members as streams. Do not extract arbitrary paths. Reject encrypted archives, corrupted members and ambiguous archives with multiple candidate `.12da` entries. Ignore unrelated non-payload entries with a diagnostic.
- Enforce configurable limits on member count, actual decompressed bytes, token length, nesting depth and entity count. Proposed initial expanded-input limit: 1 GiB, to be reviewed after benchmarking. Do not rely only on ZIP metadata.
- Honour a BOM for UTF-16 or UTF-8. Decode incrementally with strict error reporting. For legacy files without a BOM, use a validated default or an explicit encoding override; never silently replace undecodable characters.
- Tokenize quoted strings, comments, braces, numeric values and keywords according to the archive specification. Do not parse nested blocks with one regular expression.
- Record archive version, model context, style definitions, source entity identifier or ordinal, and line/offset information. Handle documented null values before validating coordinate values.
- Skip unknown balanced blocks only where the grammar establishes a safe boundary. Report unsupported semantic objects. Fail on malformed syntax where further parsing cannot be trusted.
- Use two passes over the payload where needed: scan definitions/model summaries first, then convert selected entities. This supports forward style references without retaining the full source document.

## Source and output data model

Keep source records independent of ezdxf: `SourceModel`, `SourceEntity`, `SourceStyle`, `Point3D`, `Segment`, `Arc`, `Text`, `Diagnostic`, and `ConversionOptions`. Complex source entities may produce several output entities. Preserve source identifiers in the report so approximation and omission are traceable.

The conversion report records source object counts separately from emitted CAD entity counts; splitting one object into many entities must not inflate the apparent number of converted source objects.

## 2D conversion rules

- Use source XY plan coordinates and set all output geometry to Z = 0, elevation = 0 and thickness = 0, with standard +Z extrusion where applicable.
- Preserve full survey coordinates by default. No automatic shift, rotation or CRS transformation. A temporary display origin may improve preview precision but must never leak into output coordinates.
- Use double precision and avoid rounding coordinates to display precision.
- Units must be obtained from trusted source metadata or explicitly chosen by the user. Declaring units sets drawing metadata; it does not rescale coordinates. Any unit conversion must be a separately labelled scale operation with a recorded factor.
- Preserve closure flags. Detect repeated vertices and degeneracy without inventing missing geometry. A vertical feature collapsing to a single XY location produces a diagnostic and, where meaningful, a point according to the documented entity policy.
- Retain exact XY circular arcs when the source definition permits it. A tilted 3D circle generally projects to an ellipse; do not flatten its centre and radius into a false circle.
- Approximate supported non-circular projected curves with polylines using a maximum XY chord-deviation tolerance. Default proposal: 0.01 metre converted to the selected units. Cap segment generation and report any cap that prevents meeting tolerance.
- Validate finite numbers, positive radii and valid curve definitions before output. Source Z values may appear in optional report metadata; they do not become drawing elevations.

### Geometry support plan

| Source geometry | 2D DXF output | Initial policy |
| --- | --- | --- |
| Point | POINT | Supported; configurable visible point style |
| Straight strings / 3D strings | LWPOLYLINE or LINE | Flatten XY, preserve ordering and closure |
| Circular arc / circle in XY plane | ARC, CIRCLE or polyline bulge | Preserve exact geometry |
| Text | TEXT; MTEXT where needed | Flatten insertion, preserve supported height/rotation/alignment; report font substitution |
| Super strings | LWPOLYLINE with bulges where valid | Implement only documented segment types; unsupported segments are reported |
| Non-circular curves / alignments | Approximated LWPOLYLINE | Later support within version one only after verified curve fixtures |
| TIN / mesh | Optional unique triangle edges as 2D LINE entities | Off by default; deduplicate shared source edges and warn about size |
| Symbols | Documented simple symbol mapping or POINT fallback | Report fallback; no invented block geometry |
| Pipes, drainage, complex civil objects | Only explicitly implemented plan representations | Unsupported types listed in report |

The release support matrix must name tested source variants. Loading a file successfully does not imply every object type is supported. Closed strings are boundaries unless fill export is explicitly implemented. Contour generation, paper-space layouts, external references and full civil semantics are outside the initial scope.

## Layers, styling and DXF

Default to one CAD layer per selected 12d model. Sanitize invalid characters and length, resolve case-insensitive collisions with stable suffixes, and record original-to-output names. Optional mapping rules can group models or use source attributes later.

Resolve colours and line styles from available definitions. Use layer defaults and per-entity overrides when needed to preserve differing colours within a model. Report unresolved styles and use documented fallbacks such as CONTINUOUS. Avoid external font or symbol dependencies where possible; report substitutions.

Use ezdxf to write DXF 2018 initially. Older DXF output versions are later compatibility options, not a reason to expose untested formats in the first release. Set units metadata when resolved, configure useful point display, save model-space entities and fit initial drawing extents. No layouts or title blocks are generated automatically.

Write to a temporary file beside the destination, reopen and audit it, and verify the supported entities' Z/elevation/thickness properties. Publish by atomic replacement only after validation and any overwrite confirmation. On failure or cancellation remove job-owned temporary files and preserve existing output. Reports use the same job identifier; disclose a report-save failure separately from DXF success.

## Diagnostics and command line

Write a readable summary and a JSON report containing input hash, application version, archive version, options, model/layer mappings, units, bounding boxes, source counts, output counts, approximations, skips, elapsed time and diagnostics with source references. Bound verbose warning samples while retaining aggregate counts. Keep all processing local.

Proposed CLI contract, not yet implemented:

```powershell
12daz-converter convert "survey.12daz" --output "survey_2D.dxf" --units metres
12daz-converter inspect "survey.12daz" --report "survey-inspect.json"
```

Exit codes: 0 = completed without loss warnings; 2 = completed with reported approximations, substitutions or skips; 1 = failed; 3 = cancelled. Explicit overwrite is required in unattended mode. Share the same conversion service and defaults with the GUI.

## Performance and verification

Streaming input is mandatory, but ezdxf can still retain a large drawing in memory. Measure peak memory across parsing, preview and DXF writing before claiming large-file support. Keep the preview bounded by spatial filtering and drawing simplification. Never discard export geometry to satisfy a preview limit.

Use the available 89 MB expanded sample as the first realistic benchmark. Record machine specifications, elapsed time, source/output counts, peak memory and cancellation latency. Provisional goals on a documented reference Windows x64 machine: UI remains responsive, cancel acknowledged within two seconds, and the sample exports within two minutes under 2 GiB peak memory. These are targets, not measured results; revise after the first benchmark.

The detailed [test requirements and cases](docs/test_requirements.md) define traceable acceptance criteria, fixtures, tolerances and release gates. Executable tests now cover a subset of these cases; [validation status](docs/validation_status.md) distinguishes automated coverage from pending manual qualification.

Required validation before release:

- Archive/encoding fixtures: current UTF-16 ZIP sample, plain input, UTF-8, corrupt ZIP, ambiguous members, malformed nesting and decompression limits.
- Geometry fixtures: large survey coordinates, closure, clockwise/counterclockwise arcs, bulges, null Z, tilted circles, zero-length segments, text alignment and unit handling.
- Verify arc endpoints and tessellation error against analytic reference geometry, not only serialized output snapshots.
- Reopen DXF and verify entity counts, layer mapping, XY bounds and all supported Z-bearing properties; independently open representative drawings in a CAD application and compare against known reference exports.
- Test Windows installation and launch without Python, 12d Model or AutoCAD; exercise Unicode paths, existing output, cancellation and disk-write failure.
- Account for every selected source object as exported, approximated or skipped. Fatal parse errors must not publish a successful output.

## Delivery stages

Follow [implementation steps](docs/implementation_steps.md) for ordered tasks, deliverables and completion criteria for each stage.

1. Format spike: inventory sample types and definitions, validate grammar/encoding, produce scan report and benchmark baseline.
2. CLI conversion: point, line, polyline, exact arc and basic text output; layer mapping, diagnostics and output validation.
3. GUI: implement the proposed layout, model selection, bounded plan preview and worker cancellation.
4. Release: expand fixture coverage for required real-world types, verify CAD interoperability and standalone packaging.

A first release is complete when the supported sample geometry converts correctly, unsupported content is clearly reported, the output is genuinely 2D, and the packaged program works offline on a clean Windows machine.

## Future enhancements

- **DWG output:** introduce a separate writer/conversion adapter after DXF reliability is established. Evaluate ODA Drawings SDK or a supported converter and review redistribution terms. Do not rename a DXF file to `.dwg`. [ODA Drawings SDK](https://www.opendesign.com/products/drawings), [ODA File Converter](https://www.opendesign.com/GUESTFILES/ODA_FILE_CONVERTER).
- Batch conversion, saved model/layer presets and additional DXF versions.
- Expanded alignment, symbol and style support; optional contour generation.
- Rust acceleration only where benchmarks justify the integration and maintenance cost.

## GUI concept provenance

Generated using the built-in image generation tool with the imagegen skill. Image: `docs/assets/12daz-converter-gui-proposal.png`. The exact generation prompt is saved in `docs/assets/12daz-converter-gui-prompt.txt`. The image depicts sample values and a proposed interface, not running software.
