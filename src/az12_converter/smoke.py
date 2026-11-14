"""Headless-window smoke check for a built Windows artifact."""
import json
from pathlib import Path
import tempfile
import time
import tkinter as tk

from . import COPYRIGHT, __version__
from .app import ConverterApp, resource_text
from .splash import StartupSplash
from .support import support_url


def run_smoke(report_path):
    root = tk.Tk()
    root.withdraw()
    app = ConverterApp(root)
    result = {"application_version": __version__, "passed": False}
    try:
        splash = StartupSplash(root, lambda: None, duration_ms=0, show=False)
        root.update()
        if not splash.dismissed:
            raise RuntimeError('Startup splash did not dismiss.')
        with tempfile.TemporaryDirectory(prefix='12daz-smoke-') as directory:
            source = Path(directory) / 'test.12da'
            source.write_text('// archive_version "15.1"\nmodel "Smoke" string super {breakline line data_3d {0 0 9 10 0 11} radius_data {-10} major_data {f}}', encoding='utf-8')

            def wait():
                deadline = time.monotonic() + 30
                while app.busy and time.monotonic() < deadline:
                    root.update()
                    time.sleep(.01)
                if app.busy:
                    app.cancel()
                    raise RuntimeError('Smoke worker timed out.')

            app.load_path(str(source))
            wait()
            if app.scan is None:
                raise RuntimeError(app.status.get())
            app.units.set('metres')
            app.export()
            wait()
            if not app.result or not app.result['published']:
                raise RuntimeError(app.status.get())
            if app.result['emitted_entities'] != 1:
                raise RuntimeError('Unexpected smoke entity count.')
            for method in [app.show_guide, app.show_about, app.show_licenses]:
                dialog = method()
                dialog.withdraw()
                root.update()
                if hasattr(dialog, 'support_button') and support_url() and dialog.support_button.instate(['disabled']):
                    raise RuntimeError('Configured support link is disabled in About.')
                dialog.destroy()
            if COPYRIGHT not in resource_text('LICENSE.txt'):
                raise RuntimeError('Bundled license copyright mismatch.')
            result.update(passed=True, checks=['Tk startup and splash dismissal', 'spawned archive scan', 'spawned DXF export and audit', 'offline Help/About/license resources', 'configured support button without opening a browser'], emitted_entities=1)
    except Exception as exc:
        result['error'] = str(exc)
    finally:
        if app.process:
            app.cancel_event.set()
            app.process.join(timeout=5)
            if app.process.is_alive():
                app.process.terminate()
                app.process.join()
        app.destroy()
        Path(report_path).write_text(json.dumps(result, indent=2), encoding='utf-8')
    return 0 if result['passed'] else 1
