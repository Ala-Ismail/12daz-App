"""Synthetic fixtures authored for this project; no private survey data."""
import contextlib
import io
import json
import math
import os
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch
import zipfile

import ezdxf

from az12_converter.archive import Limits, open_input
from az12_converter.cli import main
from az12_converter.errors import ConversionError, Cancelled
from az12_converter.geometry import arc_parameters
from az12_converter.parser import tokenize, iter_entities
from az12_converter.service import Options, Diagnostics, convert_file, inspect_file


HEADER = '// archive_version "15.01.08.60"\nnull null\n'
LINE = 'model "Survey" string super { name "Road" breakline line colour green data_3d { 500000 6200000 12 500001.25 6200002.5 null } }'


class CoreTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "survey.12da"
        self.output = self.root / "plan.dxf"

    def fixture(self, text=LINE, encoding="utf-8", zipped=False):
        payload = (HEADER + text).encode(encoding)
        if zipped:
            self.source = self.root / "survey.12daz"
            with zipfile.ZipFile(self.source, "w", compression=zipfile.ZIP_DEFLATED) as archive:
                archive.writestr("payload.12da", payload)
        else:
            self.source.write_bytes(payload)
        return self.source

    def convert(self, text=None, **kwargs):
        if text is not None or not self.source.exists():
            self.fixture(text or LINE)
        return convert_file(self.source, self.output, Options(units="metres", **kwargs))

    def entities(self):
        return list(ezdxf.readfile(self.output).modelspace())

    def test_IN01_plain_zip_equivalence(self):
        self.fixture()
        plain = self.convert()
        self.output.unlink()
        Path(str(self.output) + ".conversion.json").unlink()
        self.fixture(zipped=True, encoding="utf-16")
        zipped = self.convert()
        self.assertEqual(plain["source_counts"], zipped["source_counts"])
        self.assertEqual(plain["bounds"], zipped["bounds"])
        self.assertEqual(zipped["encoding"], "utf-16")

    def test_IN02_empty_missing_and_unsupported(self):
        for data in (b"", b"%PDF-1.7", b"\x1f\x8babc"):
            self.source.write_bytes(data)
            with self.assertRaises(ConversionError):
                inspect_file(self.source)
        with self.assertRaises(ConversionError):
            inspect_file(self.root / "missing.12da")
        self.assertFalse(self.output.exists())

    def test_IN02_signature_over_extension(self):
        self.fixture(zipped=True)
        wrong = self.source.with_suffix(".dat")
        self.source.rename(wrong)
        self.assertEqual(inspect_file(wrong)["source_counts"]["selected"], 1)

    def test_IN03_corrupt_zip(self):
        self.source.write_bytes(b"PK\x03\x04truncated")
        with self.assertRaises(ConversionError):
            inspect_file(self.source)

    def test_IN04_ambiguous_or_missing_payload(self):
        for names in (["one.txt"], ["a.12da", "b.12da"]):
            with zipfile.ZipFile(self.source, "w") as archive:
                for name in names:
                    archive.writestr(name, HEADER + LINE)
            with self.assertRaisesRegex(ConversionError, "exactly one"):
                inspect_file(self.source)

    def test_IN05_no_extraction(self):
        with zipfile.ZipFile(self.source, "w") as archive:
            archive.writestr("../../payload.12da", HEADER + LINE)
            archive.writestr("../../unrelated.txt", "not extracted")
        result = inspect_file(self.source)
        self.assertEqual(result["source_counts"]["selected"], 1)
        self.assertEqual(result["diagnostics"]["counts"]["EXTRA_ARCHIVE_MEMBERS"], 1)
        self.assertEqual(list(self.root.iterdir()), [self.source])

    def test_IN06_byte_boundary(self):
        self.fixture()
        size = self.source.stat().st_size
        self.assertEqual(inspect_file(self.source, limits=Limits(expanded_bytes=size))["source_counts"]["selected"], 1)
        with self.assertRaisesRegex(ConversionError, "byte limit"):
            inspect_file(self.source, limits=Limits(expanded_bytes=size - 1))

    def test_IN06_other_limits(self):
        self.fixture()
        for limits in [Limits(entities=0), Limits(token_length=5), Limits(depth=1), Limits(entity_tokens=2)]:
            with self.subTest(limits=limits), self.assertRaises(ConversionError):
                inspect_file(self.source, limits=limits)

    def test_IN07_unicode_paths(self):
        self.source = self.root / "測量 site.12da"
        self.output = self.root / "計画 2D.dxf"
        self.assertTrue(self.convert()["published"])

    def test_IN07_input_not_overwritten(self):
        self.fixture()
        original = self.source.read_bytes()
        with self.assertRaises(ConversionError):
            convert_file(self.source, self.source, Options(units="metres", overwrite=True))
        self.assertEqual(self.source.read_bytes(), original)

    def test_PA01_unicode_encodings(self):
        text = LINE.replace("Survey", "測量 modèle")
        for encoding in ("utf-8-sig", "utf-16", "utf-16-be"):
            self.fixture(text, encoding)
            if encoding == "utf-16-be":
                self.source.write_bytes(b"\xfe\xff" + self.source.read_bytes())
            result = inspect_file(self.source)
            self.assertEqual(result["models"][0]["name"], "測量 modèle")

    def test_PA02_legacy_and_strict_error(self):
        self.fixture(LINE.replace("Survey", "modèle"), "cp1252")
        with self.assertRaises(ConversionError):
            inspect_file(self.source)
        result = inspect_file(self.source, Options(encoding="cp1252"))
        self.assertEqual(result["models"][0]["name"], "modèle")

    def test_PA03_chunk_boundaries_quotes_and_comments(self):
        source = 'model "A { \\" quote" // comment\n string 3d { name "x\\\\y" data {1 2 0 3 4 0} }'
        expected = [(t.value, t.quoted) for t in tokenize([source])]
        actual = [(t.value, t.quoted) for t in tokenize(iter(source))]
        self.assertEqual(actual, expected)
        self.assertIn(('A { " quote', True), actual)
        self.assertNotIn(('comment', False), actual)

    def test_PA03_case_insensitive_model_block(self):
        self.fixture('MoDeL { NAME "Group" attributes { text "key" "value" } } StRiNg 3D { BREAKLINE line DATA { 1 2 0 3 4 0 } }')
        self.assertEqual(inspect_file(self.source)["models"][0]["name"], "Group")

    def test_PA03_adjacent_comment(self):
        self.fixture('model "M"//comment\nstring super { data_2d {1 2//comment\n} }')
        self.assertEqual(inspect_file(self.source)['source_counts']['selected'], 1)

    def test_PA05_unsupported_neighbour_reconciliation(self):
        self.fixture(LINE + ' string future { arbitrary { nested { a 1 } } } ' + LINE)
        result = self.convert()
        self.assertEqual(result["source_counts"], {"selected": 3, "exact": 2, "approximated": 0, "skipped": 1})
        self.assertEqual(len(self.entities()), 2)

    def test_PA06_malformed_fails_without_publish(self):
        for text in ['string super {', 'string super { name "unterminated }', 'string super { name a name b }']:
            self.fixture(text)
            with self.assertRaises(ConversionError):
                self.convert()
            self.assertFalse(self.output.exists())

    def test_PA06_version_warning(self):
        self.fixture()
        self.source.write_text(self.source.read_text().replace('15.01.08.60', '99.0'), encoding="utf-8")
        self.assertIn("ARCHIVE_VERSION_UNVERIFIED", inspect_file(self.source)["diagnostics"]["counts"])

    def test_GE01_large_coordinates_and_zero_Z(self):
        self.convert()
        polyline = self.entities()[0]
        self.assertEqual(polyline.dxftype(), "LWPOLYLINE")
        self.assertEqual(list(polyline.get_points("xy")), [(500000., 6200000.), (500001.25, 6200002.5)])
        self.assertEqual(polyline.dxf.elevation, 0)

    def test_EG01_keep_z_preserves_stacked_vertices_and_elevated_circle(self):
        self.convert(
            'string 3d { breakline line data_3d { 0 10 -2 0 10 3 10 10 5 } } '
            'string circle { xcentre 2 ycentre 4 zcentre 12 radius 3 }',
            geometry_mode="keep-z",
        )
        polyline, circle = self.entities()
        self.assertTrue(polyline.is_3d_polyline)
        self.assertEqual([tuple(v.dxf.location) for v in polyline.vertices],
                         [(0, 10, -2), (0, 10, 3), (10, 10, 5)])
        self.assertEqual(tuple(circle.dxf.center), (2, 4, 12))

    def test_EG05_ip_only_alignment_reconstructs_closed_plan(self):
        self.convert(
            'string super_alignment { horizontal_ips_only true valid_horizontal true closed 1 '
            'horizontal_parts { ip { x 0 y 0 } ip { x 10 y 0 } ip { x 10 y 5 } } }'
        )
        polyline, = self.entities()
        self.assertTrue(polyline.closed)
        self.assertEqual(list(polyline.get_points("xy")), [(0, 0), (10, 0), (10, 5)])

    def test_EG05_ip_only_alignment_rejects_conflicting_solved_plan(self):
        self.fixture(
            LINE + ' string super_alignment { horizontal_ips_only true valid_horizontal true '
            'horizontal_parts { ip { x 0 y 0 } ip { x 10 y 0 } } '
            'horizontal_data { breakline line data_2d { 0 0 10 1 } } }'
        )
        report = inspect_file(self.source)
        self.assertTrue(any("disagree with solved alignment" in d["message"]
                            for d in report["diagnostics"]["details"]))

    def test_EG05_ip_flag_preserves_solved_arc_for_arc_parts(self):
        self.convert(
            'string super_alignment { horizontal_ips_only true valid_horizontal true '
            'horizontal_parts { arc {} arc {} } '
            'horizontal_data { breakline line data_2d {0 0 10 0} radius_data {10} } }'
        )
        polyline, = self.entities()
        self.assertNotEqual(list(polyline.get_points("b"))[0][0], 0)

    def test_EG05_reconstructs_fixed_line_tangent_arc(self):
        self.convert(
            'string super_alignment { valid_horizontal true '
            'horizontal_parts { fixed_line { x1 0 y1 0 x2 20 y2 0 } '
            'free_arc_radius { r -5 } fixed_line { x1 20 y1 0 x2 20 y2 20 } } }'
        )
        polyline, = self.entities()
        points = list(polyline.get_points("xyb"))
        self.assertEqual(len(points), 4)
        for actual, expected in zip(points, [(0, 0, 0), (15, 0, math.sqrt(2) - 1),
                                             (20, 5, 0), (20, 20, 0)]):
            for value, target in zip(actual, expected):
                self.assertAlmostEqual(value, target)

    def test_EG07_solved_alignment_samples_parabolic_profile(self):
        report = self.convert(
            'string super_alignment { horizontal_data { breakline line data_2d {0 0 40 0} } '
            'vertical_data { data_2d {0 10 40 10} '
            'geometry_data { parabola { chainage 20 height 9.5 } } } }',
            geometry_mode="keep-z", elevation_tolerance=0.001,
        )
        polyline, = self.entities()
        points = [tuple(v.dxf.location) for v in polyline.vertices]
        self.assertEqual(points[0], (0, 0, 10))
        self.assertEqual(points[-1], (40, 0, 10))
        self.assertIn((20, 0, 9.75), points)
        for x, y, z in points:
            self.assertEqual(y, 0)
            self.assertAlmostEqual(z, 10 - x / 40 + x * x / 1600)
        for (a, _, za), (b, _, zb) in zip(points, points[1:]):
            midpoint = (a + b) / 2
            expected = 10 - midpoint / 40 + midpoint * midpoint / 1600
            self.assertLessEqual(abs((za + zb) / 2 - expected), 0.001)
        self.assertEqual(report["options"]["elevation_tolerance"], 0.001)

    def test_EG04_drainage_exports_pipe_spans_at_pit_indices(self):
        text = (
            'string drainage { data { 0 0 9 0 0  1 0 8 0 1  2 1 7 0 0  3 1 6 0 0 } '
            'pit { ip 0 x 0 y 0 } pit_v2 { ip 2 x 2 y 1 } pit { ip 3 x 3 y 1 } '
            'pipe { us_level 2 ds_level 1 } pipe { us_level 1 ds_level 0 } }'
        )
        report = self.convert(text)
        first, second = self.entities()
        self.assertEqual(list(first.get_points("xy")), [(0, 0), (1, 0), (2, 1)])
        self.assertEqual(list(second.get_points("xy")), [(2, 1), (3, 1)])
        self.assertEqual(first.dxf.elevation, 0)
        self.assertEqual(report["diagnostics"]["counts"]["DRAINAGE_PLAN_ONLY"], 1)
        keep_z = inspect_file(self.source, Options(geometry_mode="keep-z"))
        self.assertTrue(any("centreline elevations" in d["message"] for d in keep_z["diagnostics"]["details"]))
        self.fixture(text.replace("ip 2 x 2", "ip 2 x 5"))
        invalid = inspect_file(self.source)
        self.assertTrue(any("disagree with the solved vertices" in d["message"]
                            for d in invalid["diagnostics"]["details"]))

    def test_EG06_reports_active_transition_uses_solved_plan(self):
        report = self.convert(
            'string super_alignment { horizontal_parts { '
            'transition_compound_curve { r1 10 r2 -10 } } '
            'horizontal_data { breakline line data_2d { 0 0 10 0 } } }'
        )
        self.assertIn("ALIGNMENT_TRANSITION_SOLVED_ONLY", report["diagnostics"]["counts"])
        polyline, = self.entities()
        self.assertEqual(list(polyline.get_points("xy")), [(0, 0), (10, 0)])

    def test_GE01_breakline_points(self):
        self.convert(LINE.replace("breakline line", "breakline point"))
        entities = self.entities()
        self.assertEqual([e.dxftype() for e in entities], ["POINT", "POINT"])
        self.assertTrue(all(e.dxf.location.z == 0 for e in entities))

    def test_GE02_closure(self):
        self.convert('string super { breakline line closed 1 data_2d { 0 0 1 0 1 1 } }')
        self.assertTrue(self.entities()[0].closed)

    def test_GE03_signed_minor_and_major_arcs(self):
        for radius, major in [(10, 0), (-10, 0), (10, 1), (-10, 1)]:
            with self.subTest(radius=radius, major=major):
                self.fixture(f'string super {{ breakline line data_2d {{ 0 0 10 0 }} radius_data {{ {radius} }} major_data {{ {major} }} }}')
                self.convert(overwrite=True)
                bulge = self.entities()[0].get_points("xyb")[0][2]
                self.assertLess(bulge * radius, 0)
                self.assertEqual(abs(bulge) > 1, bool(major))
                centre, actual_radius, start, sweep = arc_parameters((0, 0), (10, 0), bulge)
                self.assertAlmostEqual(actual_radius, 10, places=10)
                end = (centre[0] + 10 * math.cos(start + sweep), centre[1] + 10 * math.sin(start + sweep))
                self.assertAlmostEqual(end[0], 10, places=10)
                self.assertAlmostEqual(end[1], 0, places=10)
                # Independently check the serialized bulge through ezdxf's arc reader.
                arc = list(self.entities()[0].virtual_entities())[0]
                self.assertAlmostEqual(arc.dxf.radius, 10, places=10)
                self.assertAlmostEqual(arc.dxf.center.x, 5, places=10)
                expected_y = math.copysign(math.sqrt(75), -radius if not major else radius)
                self.assertAlmostEqual(arc.dxf.center.y, expected_y, places=10)

    def test_GE03_circle(self):
        self.convert('string circle { xcentre 1 ycentre 2 zcentre 44 radius 3 }')
        circle = self.entities()[0]
        self.assertEqual(circle.dxftype(), "CIRCLE")
        self.assertEqual(tuple(circle.dxf.center), (1, 2, 0))
        self.assertEqual(circle.dxf.radius, 3)

    def test_GE04_geometry_data_repeated_segments(self):
        self.convert('string super { breakline line data_2d {0 0 1 0 2 0 3 0} geometry_data { straight {} straight {} arc {radius -1 major 0} } }')
        points = self.entities()[0].get_points("xyb")
        self.assertEqual(points[0][2], 0)
        self.assertGreater(points[2][2], 0)

    def test_GE04_alignment_horizontal_only(self):
        self.convert('string super_alignment { horizontal_data { breakline line data_2d {0 0 2 0} } vertical_data { data_2d {0 50 2 55} } }')
        self.assertEqual(list(self.entities()[0].get_points("xy")), [(0, 0), (2, 0)])

    def test_GE04_legacy_super_and_polyline_data(self):
        for kind in ['super', 'polyline']:
            self.fixture(f'string {kind} {{breakline line closed N data {{0 0 5 -10 F 10 0 9 0 F}}}}')
            result = self.convert(overwrite=True)
            self.assertEqual(result['source_counts']['skipped'], 0)
            self.assertAlmostEqual(self.entities()[0].get_points('xyb')[0][2], math.tan(math.pi / 12))

    def test_GE04_conflicting_geometry_skipped(self):
        self.fixture(LINE + 'string super {data_2d {1 2} data_3d {1 2 3}}')
        self.assertEqual(self.convert()['source_counts']['skipped'], 1)

    def test_GE05_text_placement(self):
        self.convert('string text { x 1 y 2 z 99 text "Café" worldsize 0.5 angle 30 justify "top-centre" }')
        entity = self.entities()[0]
        self.assertEqual(entity.dxf.text, "Café")
        self.assertEqual(entity.dxf.height, .5)
        self.assertEqual(entity.dxf.rotation, 30)
        self.assertEqual(entity.dxf.insert.z, 0)
        self.assertEqual(entity.dxf.align_point.z, 0)

    def test_GE06_invalid_geometry_skipped(self):
        for bad in ['string circle { xcentre 0 ycentre 0 radius -1 }', 'string super { data_3d { nan 1 0 } }', 'string super { data_3d { 1 2 inf } }']:
            self.fixture(LINE + bad)
            result = self.convert(overwrite=True)
            self.assertEqual(result["source_counts"]["skipped"], 1)
            self.assertEqual(result["emitted_entities"], 1)

    def test_GE07_vertical_collapses_to_point(self):
        result = self.convert('string super { breakline line data_3d {1 2 3 1 2 9} }')
        self.assertEqual(self.entities()[0].dxftype(), "POINT")
        self.assertIn("COLLAPSED_TO_POINT", result["diagnostics"]["counts"])

    def test_GE10_units_do_not_scale(self):
        self.fixture()
        for unit, code in [("metres", 6), ("millimetres", 4)]:
            convert_file(self.source, self.output, Options(units=unit, overwrite=True))
            document = ezdxf.readfile(self.output)
            self.assertEqual(document.units, code)
            self.assertEqual(document.modelspace()[0].get_points("xy")[0], (500000, 6200000))

    def test_GE11_units_required(self):
        self.fixture()
        with self.assertRaises(ConversionError):
            convert_file(self.source, self.output, Options())
        self.assertFalse(self.output.exists())

    def test_GE13_transition_skipped_whole(self):
        text = 'string super { breakline line data_2d {0 0 1 1} geometry_data { spiral { l1 0 l2 2 } } }'
        self.fixture(LINE + text)
        result = self.convert()
        self.assertEqual(result["source_counts"]["skipped"], 1)
        self.assertEqual(result["emitted_entities"], 1)

    def test_ST01_layers_selection_and_collision(self):
        self.fixture(LINE.replace("Survey", "A/B") + LINE.replace("Survey", "A\\\\B") + LINE.replace("Survey", "Other"))
        result = convert_file(self.source, self.output, Options(units="metres", models=["A/B", "A\\B"]))
        self.assertEqual(result["emitted_entities"], 2)
        self.assertEqual(set(result["layer_mapping"].values()), {"A_B", "A_B_2"})

    def test_ST02_per_segment_colour(self):
        self.convert('string super { breakline line data_2d {0 0 1 0 2 0} colour_data { red green } }')
        self.assertEqual([e.dxf.color for e in self.entities()], [1, 3])

    def test_ST03_substitutions_visible(self):
        result = self.convert(LINE.replace("colour green", 'colour "custom" style "custom"'))
        self.assertIn("COLOUR_SUBSTITUTION", result["diagnostics"]["counts"])
        self.assertIn("LINESTYLE_SUBSTITUTION", result["diagnostics"]["counts"])

    def test_DX01_version_audit(self):
        self.convert()
        doc = ezdxf.readfile(self.output)
        self.assertEqual(doc.dxfversion, "AC1032")
        self.assertFalse(doc.audit().has_errors)

    def test_DX03_no_overwrite(self):
        self.fixture()
        self.output.write_bytes(b"existing drawing")
        with self.assertRaises(ConversionError):
            self.convert()
        self.assertEqual(self.output.read_bytes(), b"existing drawing")
        self.convert(overwrite=True)
        self.assertNotEqual(self.output.read_bytes(), b"existing drawing")

    def test_DX04_failed_validation_preserves_destination(self):
        self.fixture()
        self.output.write_bytes(b"existing drawing")
        with patch('az12_converter.export.dxf.validate_dxf', side_effect=ConversionError('injected audit failure')):
            with self.assertRaises(ConversionError):
                self.convert(overwrite=True)
        self.assertEqual(self.output.read_bytes(), b"existing drawing")
        self.assertEqual(list(self.root.glob('.12daz-*')), [])

    def test_DX04_write_failure_cleans_temp(self):
        self.fixture()
        with patch('az12_converter.export.dxf.DxfWriter.save', side_effect=OSError('disk full')):
            with self.assertRaises(ConversionError):
                self.convert()
        self.assertFalse(self.output.exists())
        self.assertEqual(list(self.root.glob('.12daz-*')), [])

    def test_DX05_cancel_before_read(self):
        self.fixture()
        event = threading.Event()
        event.set()
        with self.assertRaises(Cancelled):
            convert_file(self.source, self.output, Options(units="metres"), cancel=event)
        self.assertFalse(self.output.exists())

    def test_DX05_cancel_at_publication_boundary(self):
        self.fixture()
        event = threading.Event()
        def validation(*args):
            event.set()
        with patch('az12_converter.export.dxf.validate_dxf', side_effect=validation):
            with self.assertRaises(Cancelled):
                convert_file(self.source, self.output, Options(units="metres"), cancel=event)
        self.assertFalse(self.output.exists())
        self.assertEqual(list(self.root.glob('.12daz-*')), [])

    def test_DX06_report_failure_keeps_dxf(self):
        self.fixture()
        with patch('az12_converter.service.atomic_json', side_effect=PermissionError('injected report failure')):
            result = self.convert()
        self.assertTrue(result["published"])
        self.assertIn("REPORT_SAVE_FAILED", result["diagnostics"]["counts"])
        self.assertTrue(self.output.exists())

    def test_DX07_unsupported_only_no_empty_output(self):
        self.fixture('string drainage { pit {} pit {} }')
        with self.assertRaisesRegex(ConversionError, "no convertible"):
            self.convert()
        self.assertFalse(self.output.exists())

    def test_ST04_repeated_conversion_stable(self):
        first = self.convert()
        old_points = list(self.entities()[0].get_points('xyb'))
        second = self.convert(overwrite=True)
        self.assertEqual(first['layer_mapping'], second['layer_mapping'])
        self.assertEqual(first['source_counts'], second['source_counts'])
        self.assertEqual(old_points, list(self.entities()[0].get_points('xyb')))

    def test_DX04_input_changed_no_publication(self):
        self.fixture()
        with patch('az12_converter.service.input_hash', side_effect=['before', 'after']):
            with self.assertRaisesRegex(ConversionError, 'changed'):
                self.convert()
        self.assertFalse(self.output.exists())

    def test_DX04_report_path_cannot_replace_source(self):
        self.fixture()
        with self.assertRaisesRegex(ConversionError, 'Report path'):
            convert_file(self.source, self.output, Options(units='metres'), report_path=self.source)
        self.assertFalse(self.output.exists())

    def test_RP01_count_reconciliation(self):
        result = self.convert(LINE + ' string super { attributes {text key value} data_2d {1 2} } string future {}')
        counts = result['source_counts']
        self.assertEqual(counts['selected'], counts['exact'] + counts['approximated'] + counts['skipped'])
        self.assertEqual(counts['skipped'], 1)

    def test_RP02_source_vs_emitted_counts(self):
        result = self.convert(LINE.replace('breakline line', 'breakline point'))
        self.assertEqual(result['source_counts']['selected'], 1)
        self.assertEqual(result['emitted_entities'], 2)

    def test_RP03_bounded_details_retain_categories(self):
        diagnostics = Diagnostics(detail_limit=60)
        for i in range(1000):
            diagnostics.add('REPEATED', str(i))
        diagnostics.add('IMPORTANT', 'later category')
        result = diagnostics.result()
        self.assertEqual(result['total'], 1001)
        self.assertLessEqual(len(result['details']), 60)
        self.assertTrue(result['details_truncated'])
        self.assertEqual(result['details'][-1]['code'], 'IMPORTANT')

    def test_RP04_report_matches_result(self):
        result = self.convert()
        report = json.loads(Path(result['report_path']).read_text(encoding='utf-8'))
        self.assertEqual(report['source_counts'], result['source_counts'])
        self.assertEqual(len(report['input_sha256']), 64)
        self.assertEqual(report['schema_version'], 1)
        self.assertNotIn('preview', report)

    def test_CL02_exit_codes(self):
        self.fixture()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(main(['convert', str(self.source), '--output', str(self.output), '--units', 'metres']), 0)
            self.fixture(LINE + ' string future {}')
            self.assertEqual(main(['convert', str(self.source), '--output', str(self.output), '--units', 'metres', '--overwrite']), 2)
            self.assertEqual(main(['convert', str(self.source), '--output', str(self.output), '--units', 'metres']), 1)
            with patch('az12_converter.cli.convert_file', side_effect=Cancelled()):
                self.assertEqual(main(['convert', str(self.source), '--output', str(self.output), '--units', 'metres', '--overwrite']), 3)


if __name__ == '__main__':
    unittest.main()
