# Implementation steps

Status: delivery checklist, 14 November 2026. Initial scanning, geometry conversion, CLI, Tk GUI and Windows packaging are implemented. Completion of a stage still requires its stated criteria; independent CAD/clean-machine checks remain pending. See [validation status](validation_status.md) for evidence.

Scope: Python/Tk standalone Windows x64 application, `.12daz`/`.12da` input, editable 2D DXF 2018 output. DWG remains a future enhancement. Follow [the design](../12daz_design.md), [test requirements](test_requirements.md) and [project guidance](../.agent).

## 1. Inventory the sample and freeze the first support matrix

Actions:

- Read the vendor archive grammar and inspect the local sample without modifying it.
- Record input hash, archive version, encoding, model names, source object types/counts, units evidence and referenced styles.
- Identify a minimal useful export set: points, straight strings, XY circular arcs/circles and basic text. List unsupported variants explicitly.
- Decide how no-BOM encodings, unknown archive versions, null values and degenerate geometry will be handled.
- Define default input/resource limits and the reference Windows benchmark machine. Confirm synthetic fixture redistribution rights.

Deliverables: scanner requirements, fixture manifest, initial compatibility matrix and confirmed decisions linked from the design.

Complete when: each advertised source variant has a known grammar and independently specified fixture; uncertainties and unsupported types are visible. The real survey is not an acceptance fixture until expected counts and geometry are established.

Tests prepared: IN-01 to IN-07, PA-01 to PA-06 and fixture expectations for the chosen GE cases.

## 2. Establish the Python project and package a minimal desktop shell

Actions:

- Create `pyproject.toml`, `src/az12_converter/` and test folders; select and pin a mutually supported Python/Tk/ezdxf toolchain.
- Add development dependencies for meaningful automated tests and formatting/static checks, with documented commands and a lockfile.
- Establish CLI and GUI entry points with no conversion claims until implemented.
- Build a minimal Windows application artifact and verify startup on a clean VM without Python. Resolve dependency notices and Tcl/Tk resource discovery early.

Deliverables: installable development project, repeatable development/build commands, basic CI and a packaging feasibility result.

Complete when: fresh development setup works and the packaged shell launches offline on the reference Windows machine. This does not satisfy full converter packaging acceptance.

Relevant tests: initial PK-01/PK-02 startup checks; retain full conversion/package checks for step 10.

## 3. Implement bounded archive reading and incremental decoding

Actions:

- Identify ZIP/plain input by content, locate one unambiguous payload and stream its bytes.
- Implement BOM-aware decoding, explicit legacy encoding override and strict errors.
- Enforce actual decompressed-byte and member limits; define reusable limit/error types for the parser.
- Add progress and cancellation hooks without GUI dependencies.
- Reject same input/output paths and avoid arbitrary archive extraction.

Deliverables: `archive.py`, input diagnostics and synthetic valid/invalid archive fixtures.

Complete when: supported content is decoded consistently and corrupt/ambiguous/oversized input fails without publishing output or modifying source files.

Tests: applicable IN-01 to IN-07 and PA-01/PA-02; parser limits from IN-06 finish in step 4.

## 4. Implement the tokenizer, parser and inspect command

Actions:

- Parse comments, quoted strings, nested blocks and numeric forms using the documented grammar.
- Introduce source records, model/style context and source references independent of ezdxf.
- Resolve forward style references through scan/convert passes or an equivalent bounded design.
- Implement safely delimited unsupported-object handling and fatal malformed-structure errors.
- Add `inspect` output with model/type counts, archive version and encoding. Validate scan counts against independently reviewed fixtures.

Deliverables: parser package, inspect command, versioned scan report and updated real-sample inventory.

Complete when: parser cases pass and the sample inventory is repeatable; unknown content cannot silently vanish from accounting.

Tests: PA-01 to PA-06, remaining IN-06 limits, RP-02/RP-03 foundations and inspect report checks.

## 5. Implement exact 2D geometry conversion

Actions:

- Implement points, straight/open/closed strings, exact XY arcs/circles and supported text placement.
- Flatten every relevant output property to Z/elevation/thickness zero and preserve survey XY.
- Resolve units explicitly without implicit rescaling; define documented handling of null/invalid/degenerate values.
- Retain curve direction, bulges, closure and source references.
- Validate against analytic expectations before adding export serialization.

Deliverables: geometry records/converters and numerical regression fixtures.

Complete when: the required GE-01 to GE-11 variants pass independent geometry assertions, with honest diagnostics for unsupported variants.

Tests: GE-01 to GE-11, numerical acceptance rules and source-level reconciliation checks.

## 6. Write and validate DXF through the CLI

Actions:

- Implement ezdxf model-space export in DXF 2018, unit metadata and deterministic layer sanitation/collision handling.
- Preserve supported colours, line styles and text properties; report substitutions.
- Implement `convert` using the same options/service that the GUI will use.
- Write beside the destination, reopen/audit, assert 2D output and atomically publish only after validation and overwrite authorization.
- Save versioned JSON reports with hashes, options, mappings, counts and diagnostics. Distinguish report failure from DXF publication success.
- Implement documented exit codes, cancellation boundary and job-owned temporary-file cleanup.

Deliverables: usable command-line converter, validated DXF fixtures and report schema.

Complete when: end-to-end CLI tests pass, output opens independently, and failure/cancellation tests preserve source and existing destination hashes.

Tests: ST-01 to ST-04, DX-01 to DX-07, RP-01 to RP-04 and CL-02/CL-03. CL-01 completes after the GUI exists.

## 7. Add optional geometry only where required by the sample

Actions:

- Prioritize useful sample variants such as mixed super strings or non-circular plan curves after exact geometry is reliable.
- Add bounded approximation with a verified XY deviation bound and explicit segment-cap diagnostics.
- Add optional TIN edges or symbol mappings only with documented semantics and fixtures. Keep TIN output off by default.
- Update the support matrix and distinguish exact, approximated, partially converted and skipped objects.

Deliverables: individually documented and tested optional adapters, or a deliberate deferred list.

Complete when: each enabled feature passes its conditional cases. Deferred types remain visible in reports and are not advertised as supported. This step may be deferred without blocking a release that does not advertise these features.

Tests: applicable GE-04/GE-08 and GE-12 to GE-14; repeat affected DX/RP checks.

## 8. Implement the desktop workflow and preview

Actions:

- Build the proposed model list, plan preview, export settings and status/report areas in Tk/ttk.
- Implement explicit UI states and a worker process using job IDs, bounded preview messages and cancellation.
- Add model search/selection, pan/zoom/fit, scale/readouts and labelled preview simplification.
- Keep preview selection consistent with export, validate units/output settings and prompt for destination overwrite.
- Connect completed jobs to report viewing and output-folder actions; prevent stale job results from replacing a newer input's state.
- Add Help > User Guide, About 12dAZ Converter, and License and Third-party Notices. Bundle offline help/license resources; show the installed version/build and `Copyright (c) 2026 Lambda Technologies Pty Ltd ABN 22 647 165 160` in About.
- Verify keyboard use, DPI scaling and meaningful warning text.

Deliverables: working GUI backed by the same conversion service as the CLI.

Complete when: UI cases pass and equivalent GUI/CLI settings produce matching canonical output.

Tests: UI-01 to UI-08, CL-01 and GUI paths through DX-03/DX-05/DX-07.

## 9. Benchmark and resolve measured bottlenecks

Actions:

- Run the authorized survey and large synthetic fixtures with fixed hardware/settings.
- Measure whole-job elapsed time, total process-tree peak memory, cancellation acknowledgement/cleanup and repeated-job resource retention.
- Check that streaming input and bounded preview do not conceal ezdxf memory growth.
- Optimize demonstrated bottlenecks and rerun affected fidelity tests. Consider Rust only if evidence supports the additional integration cost.
- Freeze agreed performance targets before release assessment and publish actual results separately from targets.

Deliverables: benchmark evidence, retained-memory investigation and documented supported limits.

Complete when: PF-01 to PF-03 meet the agreed criteria with unchanged geometry/count accounting, or release scope/limits have been explicitly revised and retested.

Tests: PF-01 to PF-03 with geometry and report regression checks after optimizations.

## 10. Qualify and package the first release

Actions:

- Build from pinned dependencies and run all applicable required cases against the release candidate.
- Open representative/reference DXFs in an independent CAD application and retain review evidence.
- Install and convert offline on a clean Windows x64 VM without Python, 12d Model or AutoCAD.
- Verify non-admin operation, Unicode paths, dependency notices and absence of private samples/secrets in the distribution.
- Publish installation/use instructions, supported variants, known limitations, test evidence and measured performance. Retain the MIT license and third-party notices.

Deliverables: standalone release artifact and evidence summary linked from the README.

Complete when: all release gates in the test requirements document are met. Unrun or blocked required tests prevent claiming a validated release.

Tests: PK-01 to PK-03, DX-02 and the full applicable regression suite.

## Progress tracking

Maintain a checklist or issue per step containing deliverables, test IDs, actual commands/results, unresolved decisions and evidence links. Mark steps complete only when their completion criteria are met. Implement in dependency order; keep optional-feature work from delaying a useful, accurately scoped DXF release. No time estimates are committed before the format spike establishes source complexity.
