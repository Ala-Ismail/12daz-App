# Python baseline for the Rust migration

The application source under test is 12dAZ Converter 0.1.5 at commit `9cb51a0edf52772331375a936b99ee4b9825d15a`, with the migration tests in this working tree. The baseline was run on Windows x64 build 26300, Python 3.11.5, Tcl/Tk 8.6.12, and `ezdxf` 1.3.5. The installed runtime dependency versions matched [requirements.lock](../requirements.lock). The first attempt with Python 3.13 selected Tcl 8.6.15 against the repository's 8.6.12 scripts and could not start Tk. Python 3.11 with the matching scripts resolved the mismatch.

## RUST-01 Suite results

`python scripts/test.py` with the baseline interpreter and matching Tcl/Tk passed all 96 tests. The run includes the existing core, GUI and worker, preview, support, and Windows version tests plus four CLI baseline tests. [Test runner](../scripts/test.py) · [Migration cases](../tests/test_rust_migration.py)

## RUST-02 CLI and failure fixtures

The migration test module creates these synthetic archives from the repository's supported grammar, with archive version `15.01.08.60`. It records the CLI flags and checks exit status, diagnostics, reports, file effects, and hash. ZIP evidence below uses the Python 3.11 baseline compressor.

| Fixture | SHA-256 | Observed outcome |
| --- | --- | --- |
| UTF-8 plain road | `a2809a9d0193a7cdedba4a84523f0779e5b76143a4c461353ec95bcf17c83a3a` | `inspect` and each eligible mode/unit conversion return `0`. The `Café` model contains one exact object with no skips. Unicode input and output paths work. |
| UTF-16 single-member ZIP | `414a30df19ddb7bd58f19cee309be99f07b71159bfcb9116402b555f18b97623` | `inspect` returns `0`, retaining the same `Café` model and exact object. Only the requested report is written. |
| CP1252 plain road | `512eece46ae7a359ca8d605989fc486aeb1512f6a3178a7b90b8265c25bdcb95` | `inspect --encoding cp1252` returns `0`. The model name and exact object are retained. |
| Mixed models | `44261bcda87215541b0f763eb845df8c595f6dc8793230e9439e7bedd772b7cd` | Selecting `Café` and `Other` returns `2`. One exact object is exported, one unsupported object is skipped with `UNSUPPORTED_OR_INVALID_GEOMETRY`, and the unselected model emits no layer. |
| Truncated source block | `c512a7ca519bc4dda0d40352615457664ee653e3cdfd28bb9af0354715aa952b` | Conversion returns `1` and creates neither drawing nor report. |

The CLI also returns `1` for a missing input and for an existing destination without overwrite. The latter leaves the existing bytes intact. Omitting mandatory units produces argument exit status `2`. The core suite independently exercises cancellation before read and at the publication boundary, simulated CLI cancellation status `3`, damaged archives, decoding limits, and report-write failure. The GUI suite exercises spawned-worker cancellation. [Core cases](../tests/test_core.py) · [GUI cases](../tests/test_gui.py)

## RUST-03 Geometry and audit

The CLI road fixture produces one green entity (ACI 3) on layer `Café` in each of eight exports. Both modes retain XY vertices `(500000, 6200000)` and `(500001.25, 6200002.5)`. Flatten 2D produces a zero-elevation `LWPOLYLINE` with zero bulges. Keep Z produces a 3D `POLYLINE` with elevations 12 and 16. Metres, millimetres, feet and inches set `$INSUNITS` to 6, 4, 2 and 1 respectively. `ezdxf` independently reopened and audited the eight DXF 2018 drawings with no errors or repairs.

The core suite also checks signed minor and major arcs against a radius of 10 and the analytic midpoint-offset magnitude `sqrt(75)`, a circle at `(1, 2, 0)` with radius 3, and Unicode text at zero elevation with height 0.5 and rotation 30 degrees. It covers expanded geometry in the supported fixtures, including alignment reconstruction and parabolic profile sampling. The [product numerical assertions](test_requirements.md#numerical-and-output-assertions) govern additional comparisons. [Core geometry expectations](../tests/test_core.py) · [Expanded geometry fixture inventory](../tests/fixtures/expanded_geometry.md)
