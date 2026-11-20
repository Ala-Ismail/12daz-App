# 12dAZ Converter Python-to-Rust migration plan

Build the Rust converter alongside the Python app. Port the core function by function, verify DXF and JSON output through the Rust CLI, then rebuild the desktop GUI on the same core. Keep the Python build available until the Rust Windows package passes the release checks below.

Use the [Rust migration test requirements](rust_migration_test_requirements.md) to verify each phase before moving on.

## 1. Capture the Python baseline

See the [recorded Python baseline](rust_migration_baseline.md) for the tested revision, fixture hashes, and outcomes.

Record the baseline commit and run `python scripts/test.py`. Capture `inspect` and `convert` results using the existing synthetic fixtures in both Flatten 2D and Keep Z modes. Include invalid inputs, cancellation, existing destinations, Unicode paths, and each supported unit. Keep private survey inputs and generated drawings in the existing ignored directories.

Record exit codes, report fields, diagnostics, model and entity counts, and the properties of reopened DXF entities. Compare DXF version, units, layers, colours, entity types, coordinates, elevations, text placement, and bulges. Use explicit tolerances for computed geometry. Use Python's `ezdxf` as an independent reader and auditor during migration. Record any intentional departure from Python behavior alongside its tests. [Core tests](../tests/test_core.py) · [Expanded geometry cases](../tests/fixtures/expanded_geometry.md)

## 2. Port the core in dependency order

For each function, read its callers, capture representative Python results, define the Rust signature and error behavior, port the branches, then compare both implementations before wiring up the next caller. Include warnings, cancellation, and filesystem effects where callers depend on them.

| Step | Function order | Acceptance check |
| --- | --- | --- |
| 1. Errors and [archive input](../src/az12_converter/archive.py) | `checkpoint`, `input_hash`, `DecodedInput.__init__`, `_read`, `chunks`, `open_input` | Plain and ZIP sources decode equivalently. Encoding overrides, resource limits, malformed inputs, progress, and cancellation behave as expected. |
| 2. [Streaming parser](../src/az12_converter/parser.py) | `tokenize`, `TokenStream.peek` and `pop`, `read_block`, `fields`, `scalar`, `block_values`, `iter_entities` | Match source entities and diagnostics across chunk boundaries, comments, quoted fields, malformed blocks, and unsupported records. |
| 3. [Geometry](../src/az12_converter/geometry.py) | `number`, `flag`, `radius_bulge`, `arc_parameters`, `default_alignment_modifiers`, `tangent_arc_plan`, `intersection_vertices`, `drainage_plan`, `alignment_elevations`, `sample_elevated_string`, `convert_geometry` | Compare primitives and warnings before export. Add point, straight string, circle, and text branches first, then signed arcs, closed strings, segment colours, alignments, drainage spans, and Keep Z sampling. |
| 4. [Preview](../src/az12_converter/preview.py) | `preview_points` in `geometry.py`, then `segment_distance`, `anchors`, `spatial_records`, `simplify`, and `PreviewCollector` | Verify the 30,000-point budget, visibility of later models, and the independence of full export geometry from display sampling. |
| 5. [DXF export](../src/az12_converter/export/dxf.py) | `DxfWriter.__init__`, `layer`, `write`, `save`, `validate_dxf` | Reopen and independently audit Rust output as DXF 2018. Check layer collisions, text alignment, signed bulges, and constant-elevation and varying-Z polylines. The Rust app reopens and validates temporary DXF output before publication. |
| 6. [Conversion service](../src/az12_converter/service.py) | `Diagnostics.add` and `result`, `same_path`, `check_destinations`, `publish`, `atomic_json`, `_job`, `inspect_file`, `convert_file` | Match results and reports. Test a changed source, cancellation before publication, competing no-overwrite exports, and report failure after a successful DXF publication. |

Evaluate the Rust DXF backend early, before committing to the full port. Use a small fixture containing aligned text, a signed bulge, a constant-elevation polyline, and a varying-Z polyline. Require both a clean independent audit and support for the checks the Rust app must perform before publishing output.

## 3. Deliver the Rust CLI

Port `cli.main` over the Rust conversion service. Preserve `inspect` and `convert`, flags and defaults, JSON output, and exit codes. Verify that declaring units leaves coordinates unchanged. Run both CLIs against the same files and compare results before starting the GUI implementation. [CLI entry point](../src/az12_converter/cli.py)

## 4. Rebuild the Windows GUI

Prototype the model list, pan-and-zoom canvas, and background job handling before choosing a GUI toolkit. Preserve the current three-panel layout, dark grey theme, and blue primary actions. Port `worker.run_job` and the `ConverterApp` workflows in this order.

1. **Open and scan.** Port the behavior of `load_path`, `start_job`, `poll`, and `finish`. Verify progress, error messages, and cancellation.
2. **Search and select models.** Port `populate_models`, `toggle_model`, and `select_all`. Verify that search preserves selections hidden by the filter.
3. **Refresh and draw the preview.** Port `request_selected_preview`, `refresh_selected_preview`, `finish_selected_preview`, `fit`, `zoom`, `pan`, and `draw`. Test rapid selection changes and opening a new file during a refresh. Apply only the latest preview result.
4. **Export DXF.** Port `export` and `cancel`. Check units gating, overwrite behavior, report viewing, and agreement with CLI output.
5. **Finish desktop integration.** Carry over normal-launch splash behavior, icons, offline Help, About, notices, and the optional support link. Check keyboard navigation and Windows display scaling.

[GUI implementation](../src/az12_converter/app.py) · [Worker](../src/az12_converter/worker.py) · [GUI tests](../tests/test_gui.py)

## 5. Package, qualify, and switch releases

The maintainer disabled the clean-machine test for this migration run and will verify it separately. See the [qualification record](rust_migration_validation.md).

Replace the PyInstaller distribution with a Rust Windows x64 build. Carry forward executable version metadata, bundled notices, the portable ZIP, checksums, and the release signing workflow. Run packaged scan, preview, export, and offline Help smoke tests on a clean Windows machine without Python or CAD installed. Compare conversion time, peak process-tree memory, and cancellation latency on the same hardware and inputs. Complete the applicable checks in the test requirements, including independent CAD interoperability.

Switch the release to Rust once the CLI and GUI open the same supported inputs, produce equivalent audited DXF and reports, and pass clean-machine package checks. Document the tested scope and carry forward the existing geometry limitations into the first Rust release.

[Build script](../scripts/build.py) · [Packaged smoke test](../src/az12_converter/smoke.py) · [Release workflow](../.github/workflows/publish-release.yml) · [Test requirements](test_requirements.md)

## 6. Clean up the README and update the websites

After the Rust release candidate passes Phase 5, update the README's download, install, source-build, CLI, testing, and packaging instructions. Update `.agent` with the qualified Rust project structure, test commands, and release checks. Remove obsolete Python runtime instructions from the normal Rust workflow and retain the Python baseline commands where developers need them for migration evidence. [Project agent guidance](../.agent)

Update the repository website and any other project-controlled website in the documented release workflow. Keep the release version, download links, Windows requirements, signing status, supported geometry, and known limitations consistent with the qualified package. Smoke-test links and the download flow before calling the public documentation ready. [README](../readme.md) · [Website source](../website/index.html) · [Website maintenance notes](../website/README.md)
