"""Compare Python and Rust CLI conversion on one Windows host and input."""

import argparse
import ctypes
from ctypes import wintypes
import json
import math
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

import ezdxf

from benchmark import MemoryCounters


def run(command, source, output, unit, keep_z):
    report = output.with_suffix(".stdout.json")
    errors = output.with_suffix(".stderr.txt")
    api = ctypes.WinDLL("psapi", use_last_error=True)
    sample = api.GetProcessMemoryInfo
    sample.argtypes = [wintypes.HANDLE, ctypes.POINTER(MemoryCounters), wintypes.DWORD]
    counters = MemoryCounters()
    counters.cb = ctypes.sizeof(counters)
    peak_working_set = peak_commit = 0
    args = ["convert", str(source), "--output", str(output), "--units", unit, "--overwrite"]
    if keep_z:
        args.extend(["--geometry-mode", "keep-z", "--linear-arc-elevations"])
    started = time.perf_counter()
    with report.open("wb") as stdout, errors.open("wb") as stderr:
        with subprocess.Popen([*command, *args], stdout=stdout, stderr=stderr,
                              env={**os.environ, "PYTHONIOENCODING": "utf-8"}) as process:
            try:
                while True:
                    if sample(int(process._handle), ctypes.byref(counters), counters.cb):
                        peak_working_set = max(peak_working_set, counters.PeakWorkingSetSize)
                        peak_commit = max(peak_commit, counters.PeakPagefileUsage)
                    if process.poll() is not None:
                        break
                    if time.perf_counter() - started >= 125:
                        raise TimeoutError("Conversion exceeded 125 seconds.")
                    time.sleep(0.025)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
    elapsed = time.perf_counter() - started
    if process.returncode not in (0, 2):
        raise RuntimeError(f"{command[0]} exited {process.returncode}. See {errors}.")
    if not peak_working_set:
        raise RuntimeError("No process memory samples were collected.")
    result = json.loads(report.read_text(encoding="utf-8"))
    return result, {"wall_seconds": round(elapsed, 3), "peak_working_set_bytes": peak_working_set,
                    "peak_commit_bytes": peak_commit, "exit_status": process.returncode}


def geometry(entity):
    kind = entity.dxftype()
    attr = entity.dxf
    if kind == "POINT":
        shape = tuple(attr.location)
    elif kind == "CIRCLE":
        shape = (tuple(attr.center), attr.radius)
    elif kind == "LWPOLYLINE":
        shape = (entity.closed, attr.elevation, entity.get_points("xyb"))
    elif kind == "POLYLINE" and entity.is_3d_polyline:
        shape = (entity.is_closed, [tuple(point) for point in entity.points()])
    elif kind == "TEXT":
        shape = (attr.text, tuple(attr.insert), tuple(attr.get("align_point", attr.insert)), attr.halign,
                 attr.valign, attr.height, attr.width, attr.oblique, attr.rotation % 360)
    else:
        raise AssertionError(f"Drawing contains unexpected {kind} entity.")
    return kind, attr.layer, attr.color, shape


def equivalent(expected, actual):
    if isinstance(expected, (tuple, list)):
        return isinstance(actual, (tuple, list)) and len(expected) == len(actual) and all(
            equivalent(a, b) for a, b in zip(expected, actual)
        )
    if isinstance(expected, float) and isinstance(actual, (int, float)):
        return abs(expected - actual) <= max(1e-8, 8 * math.ulp(expected))
    return expected == actual


def compare_dxf(python_path, rust_path):
    python = ezdxf.readfile(python_path)
    rust = ezdxf.readfile(rust_path)
    for drawing in (python, rust):
        audit = drawing.audit()
        assert not audit.errors and not audit.fixes, [issue.message for issue in (*audit.errors, *audit.fixes)]
        assert drawing.dxfversion == "AC1032"
    assert python.units == rust.units
    assert len(python.modelspace()) == len(rust.modelspace())
    for index, (expected, actual) in enumerate(zip(python.modelspace(), rust.modelspace()), 1):
        assert equivalent(geometry(expected), geometry(actual)), f"DXF mismatch at entity {index} ({expected.dxftype()})."
    return len(rust.modelspace())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("--rust-cli", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--units", required=True, choices=("metres", "millimetres", "feet", "inches"))
    parser.add_argument("--keep-z", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    directory = args.output.resolve()
    source = args.input.resolve()
    python_path = directory / "python.dxf"
    rust_path = directory / "rust.dxf"
    python_cli = Path(__file__).resolve().parents[1] / "run_cli.py"
    python, python_perf = run([sys.executable, str(python_cli)], source, python_path, args.units, args.keep_z)
    rust, rust_perf = run([str(args.rust_cli.resolve())], source, rust_path, args.units, args.keep_z)
    for key in ("input_sha256", "options", "models", "source_counts", "layer_mapping", "emitted_entities"):
        assert python[key] == rust[key], f"Reports differ in {key}."
    assert python["diagnostics"]["counts"] == rust["diagnostics"]["counts"], "Diagnostic totals differ."
    assert python_perf["exit_status"] == rust_perf["exit_status"]
    assert rust_perf["wall_seconds"] <= 120 and rust_perf["peak_working_set_bytes"] < 2 * 1024**3
    emitted = compare_dxf(python_path, rust_path)
    result = {"scope": "one direct CLI process per implementation; neither launches a conversion child",
              "host": platform.platform(), "processor": platform.processor(), "input_sha256": rust["input_sha256"],
              "mode": rust["options"]["geometry_mode"], "units": args.units, "source_counts": rust["source_counts"],
              "emitted_entities": emitted, "python": python_perf, "rust": rust_perf}
    evidence = json.dumps(result, indent=2)
    (directory / "benchmark.json").write_text(evidence, encoding="utf-8")
    print(evidence)


if __name__ == "__main__":
    main()
