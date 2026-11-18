from pathlib import Path
import tempfile
import unittest

import ezdxf

from az12_converter.geometry import Primitive
from az12_converter.preview import PreviewCollector
from az12_converter.service import Options, inspect_file, convert_file


class PreviewTests(unittest.TestCase):
    def test_large_first_model_does_not_hide_later_models(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'models.12da'
            points = ' '.join(f'{i} {i % 4}' for i in range(100))
            source.write_text('// archive_version "15.1"\n'
                f'model "Early" string super {{breakline line data_2d {{{points}}}}}'
                'model "Late" string super {breakline line data_2d {1000 1000 1010 1020}}', encoding='utf-8')
            scan = inspect_file(source, preview_limit=12)
            self.assertEqual({item['model'] for item in scan['preview']}, {'Early', 'Late'})
            self.assertLessEqual(sum(len(item['points']) for item in scan['preview']), 12)
            self.assertTrue(scan['preview_simplified'])
            early = next(item for item in scan['preview'] if item['model'] == 'Early')
            self.assertEqual(early['points'][0], (0, 0))
            self.assertEqual(early['points'][-1], (99, 3))

    def test_model_shares_stay_bounded_as_models_arrive(self):
        collector = PreviewCollector(80)
        primitive = Primitive('polyline')
        for model in range(20):
            collector.add(str(model), primitive, [(float(i), float(model)) for i in range(100)])
            self.assertLessEqual(sum(len(p['points']) for p in collector.result()), 80)
        self.assertEqual({p['model'] for p in collector.result()}, {str(i) for i in range(20)})

    def test_preview_reduction_does_not_reduce_dxf(self):
        with tempfile.TemporaryDirectory() as directory:
            source, output = Path(directory) / 'models.12da', Path(directory) / 'plan.dxf'
            points = ' '.join(f'{i} 0' for i in range(100))
            source.write_text('// archive_version "15.1"\n'
                f'model "Early" string super {{breakline line data_2d {{{points}}}}}'
                'model "Late" string super {breakline line data_2d {1000 0 1010 0}}', encoding='utf-8')
            result = convert_file(source, output, Options(units='metres'), preview_limit=6)
            self.assertLessEqual(sum(len(p['points']) for p in result['preview']), 6)
            self.assertEqual([len(e) for e in ezdxf.readfile(output).modelspace()], [100, 2])
            self.assertEqual(result['emitted_entities'], 2)

    def test_sampling_never_modifies_original_display_path(self):
        points = [(float(i), 0.) for i in range(100)]
        collector = PreviewCollector(4)
        collector.add('Model', Primitive('polyline'), points)
        self.assertEqual(len(points), 100)
        self.assertEqual(points[-1], (99., 0.))
        self.assertEqual(len(collector.result()[0]['points']), 4)

    def test_budget_keeps_sharp_bend_and_redistributes_small_model_share(self):
        collector = PreviewCollector(5)
        collector.add("Sparse", Primitive("point"), [(500, 500)])
        path = [(x, 20 if x == 7 else 0) for x in range(100)]
        collector.add("Dense", Primitive("polyline"), path)
        dense = next(record["points"] for record in collector.result() if record["model"] == "Dense")
        self.assertEqual(len(dense), 4)
        self.assertIn((7, 20), dense)
        self.assertEqual((dense[0], dense[-1]), (path[0], path[-1]))

    def test_closed_outline_retains_three_distinct_vertices_and_closure(self):
        collector = PreviewCollector(4)
        collector.add("Model", Primitive("polyline", closed=True),
                      [(0, 0), (1, 0), (2, 0), (2, 2), (0, 2), (0, 0)])
        outline = collector.result()[0]["points"]
        self.assertEqual(outline[0], outline[-1])
        self.assertEqual(len(set(outline)), 3)

    def test_spatial_coverage_keeps_isolated_feature_when_budget_is_full(self):
        collector = PreviewCollector(4)
        for x in range(40):
            collector.add("Model", Primitive("point"), [(x, 0)])
        collector.add("Model", Primitive("point"), [(1000, 1000)])
        points = [point for record in collector.result() for point in record["points"]]
        self.assertEqual(len(points), 4)
        self.assertIn((1000, 1000), points)
        self.assertTrue(any(x < 40 for x, _ in points))


if __name__ == '__main__':
    unittest.main()
