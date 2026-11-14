"""Tk widget/state and spawned-worker integration tests; windows stay withdrawn."""
import json
from pathlib import Path
import tempfile
import time
import tkinter as tk
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from az12_converter import COPYRIGHT, __version__
from az12_converter.app import ConverterApp, resource_text
from az12_converter.service import Options, convert_file, inspect_file


class GuiTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = tk.Tk()
        self.root.withdraw()
        self.app = ConverterApp(self.root)
        self.addCleanup(self.cleanup)
        self.source = Path(self.temp.name) / 'survey.12da'
        self.source.write_text('// archive_version "15.1"\nmodel "Survey" string super {breakline line data_3d {0 0 5 10 20 9}}', encoding='utf-8')

    def cleanup(self):
        if self.app.process:
            self.app.cancel_event.set()
            self.app.process.join(timeout=5)
            if self.app.process.is_alive():
                self.app.process.terminate()
                self.app.process.join()
        self.root.update_idletasks()
        self.app.destroy()

    def wait_job(self):
        deadline = time.monotonic() + 15
        while (self.app.busy or self.app.preview_pending) and time.monotonic() < deadline:
            self.root.update()
            time.sleep(.01)
        self.assertFalse(self.app.busy, 'worker did not finish')
        self.assertFalse(self.app.preview_pending, 'selected preview did not finish')
        self.root.update()

    def load(self):
        self.app.load_path(str(self.source))
        self.wait_job()
        self.assertIsNotNone(self.app.scan, self.app.status.get())

    def test_UI01_units_required_and_selection_controls(self):
        self.assertTrue(self.app.export_button.instate(['disabled']))
        self.load()
        self.assertTrue(self.app.export_button.instate(['disabled']))
        self.app.units.set('metres')
        self.assertTrue(self.app.export_button.instate(['!disabled']))
        self.app.select_all(False)
        self.assertTrue(self.app.export_button.instate(['disabled']))
        self.app.select_all(True)
        self.assertTrue(self.app.export_button.instate(['!disabled']))

    def test_UI02_search_preserves_hidden_selection(self):
        self.load()
        selected = self.app.selected.copy()
        self.app.search.set('no matching models')
        self.assertEqual(self.app.tree.get_children(), ())
        self.assertEqual(self.app.selected, selected)
        self.app.search.set('')
        self.assertEqual(len(self.app.tree.get_children()), 1)

    def test_UI03_reloading_resets_units_and_result(self):
        self.load()
        self.app.units.set('feet')
        self.app.load_path(str(self.source))
        self.wait_job()
        self.assertNotEqual(self.app.units.get(), 'feet')
        self.assertIsNone(self.app.result)

    def test_UI04_zoom_does_not_mutate_export_geometry(self):
        self.load()
        before = json.dumps(self.app.scan['preview'])
        self.app.zoom(2)
        self.app.fit()
        self.assertEqual(before, json.dumps(self.app.scan['preview']))

    def test_UI04_late_model_selection_has_preview_and_fit(self):
        points = ' '.join(f'{i} 0' for i in range(100))
        self.source.write_text('// archive_version "15.1"\n'
            f'model "Early" string super {{breakline line data_2d {{{points}}}}}'
            'model "Late" string super {breakline line data_2d {1000 1000 1010 1020}}', encoding='utf-8')
        self.app.scan = inspect_file(self.source, preview_limit=12)
        self.app.selected = {'Late'}
        self.app.populate_models()
        self.app.fit()
        self.assertEqual({p['model'] for p in self.app.selected_preview()}, {'Late'})
        self.assertEqual(self.app.origin, (1005, 1010))
        self.assertTrue(self.app.canvas.find_all())

    def test_UI04_selected_models_reclaim_full_preview_budget(self):
        points = ' '.join(f'{i} {i % 10}' for i in range(31000))
        self.source.write_text('// archive_version "15.1"\n'
            f'model "Early" string super {{breakline line data_2d {{{points}}}}}'
            f'model "Late" string super {{breakline line data_2d {{{points}}}}}', encoding='utf-8')
        self.load()
        initial = sum(len(p['points']) for p in self.app.scan['preview'] if p['model'] == 'Late')
        self.assertEqual(initial, 15000)
        self.app.tree.focus('0')
        self.app.toggle_model(SimpleNamespace(keysym='space', y=0))
        self.wait_job()
        self.assertEqual(self.app.selected, {'Late'})
        self.assertEqual({p['model'] for p in self.app.scan['preview']}, {'Late'})
        self.assertEqual(sum(len(p['points']) for p in self.app.scan['preview']), 30000)
        self.assertEqual(len(self.app.scan['models']), 2, 'model inventory must survive preview replacement')

    def test_UI04_rapid_changes_apply_latest_selection(self):
        self.source.write_text('// archive_version "15.1"\n' + ''.join(
            f'model "{name}" string super {{breakline line data_2d {{0 0 10 20}}}}'
            for name in ['A', 'B', 'C']), encoding='utf-8')
        self.load()
        self.app.tree.focus('0')
        self.app.toggle_model(SimpleNamespace(keysym='space', y=0))
        deadline = time.monotonic() + 5
        while not self.app.busy and time.monotonic() < deadline:
            self.root.update()
            time.sleep(.01)
        self.assertTrue(self.app.busy)
        self.app.select_all(False)
        self.app.tree.focus('1')
        self.app.toggle_model(SimpleNamespace(keysym='space', y=0))
        self.wait_job()
        self.assertEqual(self.app.scan['preview_selected_models'], ['B'])
        self.assertEqual({p['model'] for p in self.app.scan['preview']}, {'B'})

    def test_UI04_zoom_selector_matches_fit(self):
        self.load()
        original = self.app.scale
        self.app.zoom_percent.set('200%')
        self.app.set_zoom_percent()
        self.assertAlmostEqual(self.app.scale, original * 2)
        self.app.fit()
        self.assertEqual(self.app.zoom_percent.get(), '100%')

    def test_UI03_new_input_cancels_pending_preview_refresh(self):
        self.source.write_text('// archive_version "15.1"\n' + ''.join(
            f'model "{name}" string super {{breakline line data_2d {{0 0 10 20}}}}'
            for name in ['A', 'B']), encoding='utf-8')
        self.load()
        self.app.tree.focus('0')
        self.app.toggle_model(SimpleNamespace(keysym='space', y=0))
        self.assertTrue(self.app.preview_pending)
        replacement = self.source.with_name('replacement.12da')
        replacement.write_text('// archive_version "15.1"\nmodel "New" string super {data_2d {100 200}}', encoding='utf-8')
        self.app.load_path(str(replacement))
        self.wait_job()
        self.assertEqual(self.app.selected, {'New'})
        self.assertEqual({p['model'] for p in self.app.scan['preview']}, {'New'})

    def test_DX05_spawned_job_cancellation(self):
        self.app.load_path(str(self.source))
        self.app.cancel()
        self.wait_job()
        self.assertIn('Cancelled', self.app.status.get())
        self.assertFalse(Path(self.app.output.get()).exists())

    def test_UI07_about_copyable_notice_and_version(self):
        dialog = self.app.show_about()
        text = next(widget for frame in dialog.winfo_children() for widget in frame.winfo_children() if isinstance(widget, tk.Text))
        contents = text.get('1.0', 'end')
        self.assertIn(COPYRIGHT, contents)
        self.assertIn(__version__, contents)
        self.assertIn('MIT', contents)
        text.tag_add('sel', '1.0', 'end')
        self.assertTrue(text.tag_ranges('sel'))
        dialog.destroy()

    def test_UI08_offline_help_and_license(self):
        self.assertIn('Export DXF', resource_text('user_guide.txt'))
        self.assertIn(COPYRIGHT, resource_text('LICENSE.txt'))
        self.assertIn('Third-party', resource_text('THIRD_PARTY_NOTICES.txt'))
        self.app.show_guide().destroy()
        self.app.show_licenses().destroy()

    def test_UI07_about_support_link(self):
        from az12_converter import support
        with patch.object(support, 'SUPPORT_URL', 'https://example.com/support'), patch.object(support, 'open_support') as opened:
            dialog = self.app.show_about()
            self.assertTrue(dialog.support_button.instate(['!disabled']))
            dialog.support_button.invoke()
            opened.assert_called_once_with(dialog)
            dialog.destroy()

    def test_CL01_gui_worker_matches_core_conversion(self):
        self.load()
        self.app.units.set('metres')
        self.app.export()
        self.wait_job()
        result = self.app.result
        self.assertIsNotNone(result, self.app.status.get())
        self.assertTrue(result['published'])
        other = Path(self.temp.name) / 'other.dxf'
        direct = convert_file(self.source, other, Options(units='metres', models=['Survey']))
        self.assertEqual(result['source_counts'], direct['source_counts'])
        self.assertEqual(result['layer_mapping'], direct['layer_mapping'])
        self.assertEqual(result['emitted_entities'], direct['emitted_entities'])


if __name__ == '__main__':
    unittest.main()
