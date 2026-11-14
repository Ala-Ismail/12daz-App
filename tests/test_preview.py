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


if __name__ == '__main__':
    unittest.main()
