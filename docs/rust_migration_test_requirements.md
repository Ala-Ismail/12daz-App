# Rust migration test requirements

These requirements qualify each phase of the [Python-to-Rust migration plan](rust_migration_plan.md). The case IDs in the final column refer to the [existing product test requirements](test_requirements.md). Apply expanded-geometry cases to the source variants and output modes enabled for the Rust release. These are acceptance requirements for future execution.

## 1. Capture the Python baseline

| ID | Test | Pass criteria | Existing coverage |
| --- | --- | --- | --- |
| RUST-01 | Run `python scripts/test.py` from the baseline commit. Record the Python version, dependencies, OS, and results. | The baseline is reproducible. Investigate failures and document any known Python defect that affects expected results. | [Python test runner](../scripts/test.py) |
| RUST-02 | Run Python `inspect` and `convert` on versioned fixtures for plain and ZIP input, supported encodings, both geometry modes, all four units, invalid input, Unicode paths, cancellation, and existing destinations. Use the service or worker for cases that need deterministic cancellation or failure injection. | Each fixture has a hash, provenance, invocation or test setup, exit status or error, expected diagnostics and counts, and canonical DXF properties where output is expected. | IN-01 to IN-07, PA-01 to PA-06, GE-10, DX-03 to DX-06, CL-02 |
| RUST-03 | Establish independent geometry and DXF expectations for representative supported entities. Reopen baseline drawings with `ezdxf` and audit them. | Fixture expectations specify coordinates, entity types, curves, units, colours, layers, and applicable tolerances. Audit results and differences from independently established expectations are recorded. | GE-01 to GE-11, ST-01 to ST-04, DX-01, applicable EG cases |

**Phase exit.** The recorded baseline, fixtures, and comparison rules are ready for reuse by the Rust tests.

## 2. Port the core

| ID | Component and test | Pass criteria | Existing coverage |
| --- | --- | --- | --- |
| RUST-04 | Archive and decoding. Exercise equivalent plain/ZIP payloads, BOMs, legacy overrides, split multibyte characters, corrupt or ambiguous archives, and exact limits plus one. Cancel during hashing and decoding. | Decoded data, hash, metadata, limit boundaries, and failure categories match the contract. Progress advances for streamed reads, cancellation stops the job, and archive entries create no filesystem output. | IN-01 to IN-07, PA-01, PA-02, DX-05 |
| RUST-05 | Parser. Vary chunk boundaries through quoted text, comments, and nested blocks. Include invalid syntax, unknown commands, model changes, and a supported record on each side of an unsupported record. | Tokens and source entities preserve values, model context, ordering, and diagnostic locations. Malformed input fails cleanly and supported neighbours remain available where the grammar allows recovery. | PA-03 to PA-06 |
| RUST-06 | Geometry. Compare primitives before DXF export. Include open/closed strings, positive/negative and major/minor arcs, circles, text, null or invalid values, large coordinates, segment colours, alignments, drainage, and both output modes. | Exact geometry meets the product tolerances. Flatten 2D has zero Z. Keep Z preserves verified elevations and meets the configured bounds for enabled sampling. Unsupported inputs and unmet tolerances produce source-linked diagnostics. | GE-01 to GE-14, ST-02, applicable EG-01 to EG-10 |
| RUST-07 | Preview. Fill the point budget with an early model, add later models, then regenerate for a selected subset. Include a closed outline, a sharp bend, and an isolated feature. Compare exports with preview enabled and disabled. | At most 30,000 display points are retained with the default budget. Model availability, omission and simplification reporting, and shape retention match the fixture expectations. The selected subset can reuse the available budget. Canonical exports match. | UI-02, UI-04, PF-03, EG-12, [preview regression tests](../tests/test_preview.py) |
| RUST-08 | DXF backend and validation. Start with an early prototype containing aligned text, signed bulges, a constant-elevation polyline, and a varying-Z polyline. Extend it to all supported entities, layer collisions, units, and invalid DXF fixtures. | The independent reader reopens DXF 2018 (`AC1032`) and audits it without errors or fixes. Entity types and geometry match the fixture. Runtime validation rejects incorrect versions or counts, malformed content, invalid coordinates, nonstandard extrusion, nonzero thickness, and nonzero Z in Flatten 2D before publication. | ST-01 to ST-04, DX-01, DX-04, EG-11 |
| RUST-09 | Service and publication. Inject write/validation failures, cancellation before publication, source mutation, conflicting paths, and a race between two no-overwrite jobs. Fail report writes during inspect and after a successful DXF publication. | Source and previously existing destination contents are preserved on pre-publication failure. Only one no-overwrite job publishes to a free destination. Each job cleans up its own temporary files. Inspect report failure is an error. A report failure after DXF publication keeps the drawing and returns a warning. | IN-07, DX-03 to DX-07 |
| RUST-10 | Reports. Inspect and convert fixtures containing exact, partial, skipped, and split objects. Generate warnings beyond the detail cap. | Schema and behavior match the baseline. `selected = exact + approximated + skipped`, emitted counts match the reopened DXF, input hashes match, and capped details retain complete aggregate counts and disclose truncation. | RP-01 to RP-04, applicable EG-12 |

**Phase exit.** Pass the DXF prototype before depending on the chosen backend. Pass the core tests at function and service level before qualifying the CLI. For each ported function, exercise applicable error, warning, cancellation, or side-effect branches as well as successful calls.

## 3. Deliver the Rust CLI

| ID | Test | Pass criteria | Existing coverage |
| --- | --- | --- | --- |
| RUST-11 | Run the Python and Rust executables on the same `inspect` and `convert` fixtures. Exercise documented flags and defaults, repeated model filters, Unicode paths, unit declarations, both geometry modes, and tolerances. | Canonical stdout JSON, reports, diagnostics, and DXF match the accepted baseline. Unit declarations change metadata while numeric coordinates remain identical. `inspect` creates only a requested report. | GE-10, RP-04, CL-01, applicable EG-12 |
| RUST-12 | Run clean and warning-producing conversions, invalid input, cancellation, invalid arguments, and overwrite attempts as subprocesses. | Clean success returns `0`, warning success `2`, conversion failure `1`, and cancellation `3`. Argument errors follow the baseline status and message behavior. Unattended commands return without prompts and preserve existing files when overwrite is disabled. | CL-02, CL-03, DX-03, DX-05, DX-06 |

**Phase exit.** The CLI passes end-to-end parity for supported inputs and failure paths and is ready to serve as the reference entry point for GUI checks.

## 4. Rebuild the Windows GUI

| ID | Test | Pass criteria | Existing coverage |
| --- | --- | --- | --- |
| RUST-13 | Open valid and invalid input, cancel a large scan or export, attempt a duplicate export, and open another input during a pending preview. | Controls reflect the active job, progress and failure messages appear, and the window remains responsive. Cancellation meets PF-02. Older jobs cannot overwrite the latest state, and reload resets settings that require confirmation. | UI-01, UI-03, DX-05, PF-02 |
| RUST-14 | Select models, filter the list, rapidly change selection, then pan, zoom, and fit the latest preview at large survey coordinates. | Filtering preserves hidden selections. Only the latest preview is applied, the model inventory remains intact, the point budget holds, and cursor/readout coordinates remain consistent. | UI-02 to UI-04, PF-03 |
| RUST-15 | Attempt export without units or convertible selections, then export with valid settings in both modes. Exercise overwrite, warnings, report viewing, and the output-folder action. | Invalid states prevent export. Equivalent GUI and CLI options yield equivalent audited DXF and diagnostics. Warnings remain visible, and report and folder actions target the correct files. | GE-11, DX-03, DX-07, UI-01, UI-06, CL-01, applicable EG-12 |
| RUST-16 | Check normal launch, splash dismissal and noninteractive launch paths, Help/About/notices, support actions, keyboard navigation, minimum window size, and 100/150/200 percent display scaling. | Controls remain reachable and readable. Offline resources open and About shows the required selectable notice and build version. The startup splash follows the current launch behavior, and PayPal opens only on explicit action. | UI-05, UI-07 to UI-10 |

**Phase exit.** Pass the automated workflow checks and manual Windows interaction checks. Verify GUI and CLI export parity before qualifying the package.

## 5. Package, qualify, and switch releases

| ID | Test | Pass criteria | Existing coverage |
| --- | --- | --- | --- |
| RUST-17 | Clean-machine test disabled for this migration run at the maintainer's request. For separate verification, unpack the candidate ZIP on a clean supported Windows x64 machine and run the package test offline as a standard user. | Scan, preview, export, and Help work without Python, AutoCAD, or 12d Model. CLI and GUI run without elevation, can write to paths with spaces and Unicode, and make no conversion network requests. | PK-01 to PK-03 |
| RUST-18 | Check the EXE metadata, archive contents, checksum, and signature verification for any signed candidate. | Version and executable identity match release metadata. The checksum matches the final ZIP. Bundled licenses and notices are present and private fixtures and secrets are absent. A signed candidate passes the configured publisher and timestamp checks. | PK-03, [release workflow](../.github/workflows/publish-release.yml) |
| RUST-19 | Benchmark both builds on the same hardware and inputs. Measure conversion time, process-tree peak memory, cancellation acknowledgement, and cleanup. Repeat the load/export/cancel workflow ten times. | Meet PF-01 and PF-02 targets applicable to the release and the PF-03 repeatability checks. Preserve source reconciliation and output fidelity. Record observed resource trends, cleanup, and any deviations against the Python baseline. | PF-01 to PF-03 |
| RUST-20 | Independently inspect representative Rust DXFs in CAD, including enabled Keep Z output. Review the resulting supported-variant matrix and known limitations. | Geometry, extents, text, units, layers, colours, and applicable Z/profile behavior match reviewed references. CAD application/version and results are recorded. All applicable product cases and release gates pass before switching releases. | DX-02, applicable EG-11, [release gates](test_requirements.md#release-gates) |

**Phase exit.** Release qualification applies to the actual candidate package. The maintainer authorized continuation with RUST-17 disabled for this migration run and will handle clean-machine verification separately. Record that status in the release evidence. All other applicable checks are required.

## 6. Clean up the README and update the websites

| ID | Test | Pass criteria | Existing coverage |
| --- | --- | --- | --- |
| RUST-21 | Follow the updated README's install, CLI, source-build, test, and package commands from the release candidate. | Each command works from the documented working directory, and download names, flags, and runtime requirements match the qualified build. Python instructions used for baseline verification are still discoverable. | CL-01 to CL-03, PK-01 to PK-03 |
| RUST-22 | Review the project website source and any other documented project site against the release. Check links, assets, and download flow. | Pages agree on the release version, package, signing status, and supported geometry. Links resolve to the intended destinations, screenshots load, and download links identify the qualified release artifact. | PK-01, PK-03, [website maintenance notes](../website/README.md) |
| RUST-23 | Compare `.agent` with the qualified Rust source tree, README, and release workflow. | Contributor instructions point to the actual Rust modules and verified test, build, and release commands. Python baseline guidance remains available, and supported limitations are accurately described. | [Project agent guidance](../.agent) |

**Phase exit.** README, website, and `.agent` checks pass against the qualified release and its documented support scope.

## Evidence and comparison rules

The [executable CLI cases](../tests/test_rust_migration.py) cover parts of RUST-02, RUST-03, RUST-11, and RUST-12 using synthetic archives. They exercise encoding and Unicode paths, both geometry modes and all four units, audited DXF, JSON reports, model filters, warnings, and rejected exports. The regular Python suite runs the baseline cases. Run them on their own with the project's Python environment active.

```powershell
python -m unittest discover -s tests -p test_rust_migration.py -v
```

Set `RUST_MIGRATION_CLI` to the Rust CLI executable path before running the same command. The test runner then executes both implementations against the same fixtures, compares the stable report fields, and checks cancellation of the Rust subprocess. `cargo test --manifest-path rust/Cargo.toml --locked --features desktop` runs the Rust core and desktop tests.

- Record the migration case ID, Python and Rust commits/builds, fixture hash, options, test command or manual steps, expected and actual results, and status. Link relevant logs, audited entity comparisons, or manual evidence. Keep private data in the repository's ignored locations.
- Compare DXF semantically using reopened entities and the product's [numerical assertions](test_requirements.md#numerical-and-output-assertions). For JSON, compare stable fields and normalize environment-specific paths where needed. Check the presence and type of volatile fields such as job ID and elapsed time while allowing their values to differ. Expect each build to report its own version.
- Run focused cases for each migrated component and rerun all applicable earlier-phase cases before release. Document any intentional behavior change and its expected result. Record blocked or unexecuted checks as pending and investigate mismatches before passing the phase.
