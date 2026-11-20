# Rust migration validation

The Windows x64 v0.2.0 Rust candidate was built locally. The [migration plan](rust_migration_plan.md) records the phase order, and the [test requirements](rust_migration_test_requirements.md) define the acceptance cases.

| Phase | Results |
| --- | --- |
| 1. Python baseline | 96 tests passed on Python 3.11.5 with Tcl/Tk 8.6.12. The baseline record includes the application commit, fixture hashes and expected DXF output. |
| 2. Rust core | 20 tests passed for bounded archive reads, streaming parsing, geometry, preview, DXF validation, reports and atomic publication. The production DXF writer's probe reopened in Rust and passed `ezdxf` audit without errors or repairs. |
| 3. Rust CLI | Five cases passed against the optimized CLI, including Python/Rust parity on synthetic fixtures and real subprocess cancellation. With the Rust CLI selected, the full Python suite passed 97 tests. |
| 4. Windows GUI | Six automated desktop tests passed. A native smoke run loaded an archive, rendered a preview, exported DXF and compared its drawing and report with the Rust CLI. The minimum-size layout was exercised at 100, 150 and 200 percent in the accessibility tree. Native window captures at those effective scales were visually reviewed using synthetic geometry. |
| 5. Windows package | The unsigned ZIP passed metadata, checksum, file round-trip, extracted GUI/CLI smoke and local CAD checks. **RUST-17 clean-machine testing was disabled for this run at the maintainer's request** and is reserved for separate verification. The hosted release workflow was checked locally with Actionlint. |
| 6. Documentation and website | The README build, package verification and CLI examples passed. The website's 36 local targets, candidate screenshot and FAQ were checked, including renders at 500 and 1280 pixels. The published v0.1.5 ZIP and checksum URLs returned HTTP 200. `.agent` and the README specify Rust-only feature development. |

[Python baseline](rust_migration_baseline.md) · [Independent DXF probe audit](../scripts/audit_rust_dxf_probe.py) · [Package test](../scripts/test_rust_windows_package.ps1)

## Candidate package

The portable archive contains `12dAZ-Converter.exe`, `az12-converter.exe`, `LICENSE.txt`, `THIRD_PARTY_NOTICES.txt`, and `user_guide.txt`. Both binaries carry v0.2.0 Windows version resources. The extracted package ran from a Unicode path as a standard user on the development host with only the Windows system directory on `PATH`. The GUI started a bundled CLI subprocess during smoke testing to check conversion parity. The local archive was unsigned. The release configuration continues to support the existing Azure and SignPath routes for the GUI executable when enabled. No live signing run has been performed for v0.2.0. [Windows build script](../scripts/build_rust_windows.ps1) · [Release build workflow](../.github/workflows/test.yml)

To run the separately deferred clean-machine check, copy the ZIP, its `SHA256SUMS.txt` and `test_rust_windows_package.ps1` to an offline Windows x64 test machine without Python or CAD installed. Run the script from a standard-user session and keep the evidence file with the OS and connection state.

```powershell
.\test_rust_windows_package.ps1 -Archive .\12dAZ-Converter-0.2.0-Windows-x64.zip -Evidence .\smoke.json
```

## Conversion and repeatability

Same host and reference archive in both runs, Windows x64 build 26300, Intel Core i9-13905H, approximately 31.7 GiB usable RAM, and NVMe SSD. Input SHA-256 `6334f436cc1cc78c5773b7f6cd366f0cfcf3787ef823b2fd8cc02ed261edf65c`. The benchmark measures subprocess wall time and peak working set from Windows process memory counters. Each CLI does the conversion in its own process. Both exited with status 2 for disclosed warnings. The benchmark compared reports and reopened DXF entities, including styles and geometry, against the same Python options and drawing units.

| Mode | Source reconciliation | Emitted DXF entities | Python wall time / peak working set | Rust wall time / peak working set |
| --- | --- | ---: | ---: | ---: |
| Flatten 2D | 536 selected, 509 approximated, 27 skipped | 624 | 5.851 s / 63.1 MiB | 0.620 s / 13.9 MiB |
| Keep Z with linear arc grades | 536 selected, 496 approximated, 40 skipped | 628 | 9.281 s / 154.9 MiB | 0.744 s / 20.1 MiB |

The desktop stress test completed ten load/export/cancel cycles with a 30,300-point synthetic source. Preview stayed inside its 30,000-point allowance. The longest observed cancellation through cleanup was about 11 ms. Private bytes after cleanup ranged from 2.0 to 3.2 MiB during this worker-state test, with fluctuations after warm-up and no orphan job files. This measures the worker and desktop state in-process. The native GUI renderer's memory is outside this measurement. [Benchmark runner](../scripts/benchmark_rust_migration.py) · [Desktop stress case](../rust/src/desktop/state.rs)

CLI parity covers the synthetic fixtures and reference archive exercised here. The documented and tested input encodings are UTF-8, UTF-16 LE/BE and CP1252. Python's broader codec catalogue is outside the tested parity scope.

## Independent CAD check

AutoCAD 2022 Core Console (`ACADVER` 24.1) opened both v0.2.0 candidate drawings read-only. The synthetic fixtures declare millimetres (`INSUNITS` 4). The model-space records preserved the Survey layer, entity colours, circle centre and radius, aligned text and rotation, and the signed lightweight-polyline bulge. Flatten 2D had zero output elevation. Keep Z retained the level arc at elevation 12, circle and text at elevation 7, and the varying-Z polyline vertices `(1, 2, 12)` and `(3, 4, 16)`. FreeCAD 1.0.2 also opened the files through its desktop importer and retained the varying-Z vertices. Its import of the elevated lightweight arc displayed it at zero, so AutoCAD provided the independent confirmation for that entity. [AutoCAD Core Console](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-Core-Console/files/AUTOCAD_CORE_CONSOLE.html)
