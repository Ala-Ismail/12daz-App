"""Exercise the CLI baseline and optionally compare a Rust candidate against it."""

import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import zipfile

import ezdxf

ROOT = Path(__file__).resolve().parents[1]
HEADER = '// archive_version "15.01.08.60"\nnull null\n'
ROAD = (
    'model "Café" string super { name "Road" breakline line colour green '
    'data_3d { 500000 6200000 12 500001.25 6200002.5 16 } }'
)


class RustMigrationCliTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.commands = {"Python": [sys.executable, str(ROOT / "run_cli.py")]}
        if "RUST_MIGRATION_CLI" in os.environ:
            candidate = Path(os.environ["RUST_MIGRATION_CLI"]).resolve()
            if not candidate.is_file():
                raise FileNotFoundError(f"RUST_MIGRATION_CLI does not name an executable file: {candidate}")
            cls.commands["Rust"] = [str(candidate)]

    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)

    def fixture(self, content=ROAD, encoding="utf-8", zipped=False):
        payload = (HEADER + content).encode(encoding)
        if zipped:
            path = self.root / "測量 input.12daz"
            with zipfile.ZipFile(path, "w") as archive:
                member = zipfile.ZipInfo("payload.12da")
                member.create_system = 0
                archive.writestr(member, payload, compress_type=zipfile.ZIP_DEFLATED)
        else:
            path = self.root / "測量 input.12da"
            path.write_bytes(payload)
        return path

    def compare(self, check):
        results = {}
        for implementation, command in self.commands.items():
            with self.subTest(implementation=implementation):
                work = Path(tempfile.mkdtemp(dir=self.root))
                results[implementation] = check(command, work)
        if len(results) == 2:
            self.assertEqual(results["Rust"], results["Python"])

    def invoke(self, command, work, *args, status=0):
        result = subprocess.run(
            [*command, *map(str, args)], cwd=work, capture_output=True,
            encoding="utf-8", timeout=30,
            env={**os.environ, "PYTHONIOENCODING": "utf-8"},
        )
        self.assertEqual(result.returncode, status, result.stderr or result.stdout)
        return result

    def canonical_report(self, report, work, source):
        report = dict(report)
        self.assertEqual(report["schema_version"], 1)
        self.assertIsInstance(report.pop("job_id"), str)
        self.assertIsInstance(report.pop("application_version"), str)
        self.assertGreaterEqual(report.pop("elapsed_seconds"), 0)
        self.assertEqual(Path(report["input"]), source.resolve())
        self.assertEqual(report["input_sha256"], hashlib.sha256(source.read_bytes()).hexdigest())
        for key, name in (("output", "計画 output.dxf"), ("report_path", "report.json")):
            if key in report:
                self.assertEqual(Path(report[key]), (work / name).resolve())
                report[key] = name
        self.assertNotIn("preview", report)
        return report

    def test_RUST_02_11_inspect_encoded_archives(self):
        for encoding, zipped, flags in (("utf-8", False, []), ("utf-16", True, []),
                                        ("cp1252", False, ["--encoding", "cp1252"])):
            with self.subTest(encoding=encoding, zipped=zipped):
                source = self.fixture(encoding=encoding, zipped=zipped)

                def check(command, work):
                    result = self.invoke(command, work, "inspect", source, *flags, "--report", "report.json")
                    self.assertEqual({path.name for path in work.iterdir()}, {"report.json"})
                    stdout = self.canonical_report(json.loads(result.stdout), work, source)
                    saved = self.canonical_report(
                        json.loads((work / "report.json").read_text(encoding="utf-8")), work, source,
                    )
                    self.assertEqual(stdout, {**saved, "report_path": "report.json"})
                    self.assertFalse(stdout["published"])
                    self.assertEqual([model["name"] for model in stdout["models"]], ["Café"])
                    self.assertEqual(stdout["source_counts"], {"selected": 1, "exact": 1, "approximated": 0, "skipped": 0})
                    return stdout

                self.compare(check)

    def test_RUST_03_11_units_and_geometry_modes(self):
        source = self.fixture()
        for unit, code in {"metres": 6, "millimetres": 4, "feet": 2, "inches": 1}.items():
            for mode in ("flatten-2d", "keep-z"):
                with self.subTest(unit=unit, mode=mode):

                    def check(command, work):
                        output = "計画 output.dxf"
                        result = self.invoke(command, work, "convert", source, "--output", output,
                                             "--report", "report.json", "--units", unit, "--geometry-mode", mode)
                        report = self.canonical_report(json.loads(result.stdout), work, source)
                        saved = self.canonical_report(
                            json.loads((work / "report.json").read_text(encoding="utf-8")), work, source,
                        )
                        self.assertEqual(report, {**saved, "report_path": "report.json"})
                        self.assertTrue(report["published"])
                        self.assertEqual(report["emitted_entities"], 1)
                        doc = ezdxf.readfile(work / output)
                        audit = doc.audit()
                        self.assertFalse(audit.errors or audit.fixes)
                        self.assertEqual((doc.dxfversion, doc.units), ("AC1032", code))
                        entity, = doc.modelspace()
                        self.assertEqual((entity.dxf.layer, entity.dxf.color), ("Café", 3))
                        if mode == "keep-z":
                            self.assertEqual(entity.dxftype(), "POLYLINE")
                            self.assertTrue(entity.is_3d_polyline)
                            self.assertEqual([tuple(point) for point in entity.points()],
                                             [(500000, 6200000, 12), (500001.25, 6200002.5, 16)])
                        else:
                            self.assertEqual(entity.dxftype(), "LWPOLYLINE")
                            self.assertEqual(entity.dxf.elevation, 0)
                            self.assertEqual(entity.get_points("xyb"),
                                             [(500000, 6200000, 0), (500001.25, 6200002.5, 0)])
                        return report

                    self.compare(check)

    def test_RUST_11_12_warning_and_repeatable_model_filter(self):
        source = self.fixture(ROAD + '\nmodel "Other" string future {}\n' + ROAD.replace('"Café"', '"Excluded"'))

        def check(command, work):
            result = self.invoke(command, work, "convert", source, "--output", "計画 output.dxf", "--report",
                                 "report.json", "--units", "metres", "--model", "Café", "--model", "Other", status=2)
            report = self.canonical_report(json.loads(result.stdout), work, source)
            self.assertEqual(report["source_counts"], {"selected": 2, "exact": 1, "approximated": 0, "skipped": 1})
            self.assertEqual(report["diagnostics"]["counts"], {"UNSUPPORTED_OR_INVALID_GEOMETRY": 1})
            self.assertEqual(report["layer_mapping"], {"Café": "Café"})
            self.assertTrue((work / "計画 output.dxf").is_file())
            return report

        self.compare(check)

    def test_RUST_12_rejects_invalid_input_and_protects_existing_output(self):
        source = self.fixture()
        malformed = self.root / "malformed.12da"
        malformed.write_bytes((HEADER + 'model "Broken" string super {').encode("utf-8"))

        def check(command, work):
            output = work / "計画 output.dxf"
            args = ("--output", output.name, "--report", "report.json", "--units", "metres")
            for path in (malformed, self.root / "missing.12da"):
                result = self.invoke(command, work, "convert", path, *args, status=1)
                self.assertTrue(result.stderr.strip())
                self.assertEqual(list(work.iterdir()), [])
            output.write_bytes(b"existing drawing")
            result = self.invoke(command, work, "convert", source, *args, status=1)
            self.assertTrue(result.stderr.strip())
            self.assertEqual(output.read_bytes(), b"existing drawing")
            self.assertEqual(list(work.iterdir()), [output])
            result = self.invoke(command, work, "convert", source, "--output", output.name, status=2)
            self.assertIn("--units", result.stderr)
            self.assertEqual(output.read_bytes(), b"existing drawing")

        self.compare(check)

    @unittest.skipUnless("RUST_MIGRATION_CLI" in os.environ, "requires a built Rust CLI")
    def test_RUST_12_rust_subprocess_cancellation(self):
        source = self.root / "large.12da"
        output = self.root / "cancelled.dxf"
        output.write_bytes(b"existing drawing")
        with source.open("wb") as stream:
            stream.write(HEADER.encode("utf-8"))
            block = (ROAD + "\n").encode("utf-8") * 8000
            for _ in range(32):
                stream.write(block)
        options = {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == "nt" else {"start_new_session": True}
        event = signal.CTRL_BREAK_EVENT if os.name == "nt" else signal.SIGINT
        with subprocess.Popen(
            [*self.commands["Rust"], "convert", str(source), "--output", str(output),
             "--units", "metres", "--overwrite"], stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, encoding="utf-8", **options,
        ) as process:
            try:
                time.sleep(0.3)
                self.assertIsNone(process.poll(), "conversion must still be running when cancellation is sent")
                process.send_signal(event)
                stdout, stderr = process.communicate(timeout=15)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate()
        self.assertEqual(process.returncode, 3, stderr)
        self.assertEqual(stdout, "")
        self.assertEqual(stderr.strip(), "Cancelled.")
        self.assertEqual(output.read_bytes(), b"existing drawing")
        self.assertEqual(set(self.root.iterdir()), {source, output})
