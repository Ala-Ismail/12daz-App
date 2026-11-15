# PyInstaller recipe: deliberately excludes optional CAD visualization stacks.
from pathlib import Path
import runpy

root = Path(SPECPATH)
version = runpy.run_path(str(root / 'src' / 'az12_converter' / '__init__.py'))['__version__']
version_info = runpy.run_path(str(root / 'scripts' / 'windows_version.py'))['build_version_info'](version)
a = Analysis(
    [str(root / 'run_gui.py')],
    pathex=[str(root / 'src')],
    datas=[(str(root / 'src' / 'az12_converter' / 'resources'), 'az12_converter/resources')],
    hiddenimports=['az12_converter.worker'],
    excludes=['PyQt5', 'PyQt6', 'PySide2', 'PySide6', 'matplotlib', 'scipy', 'pandas', 'IPython'],
    noarchive=False,
)
pyz = PYZ(a.pure)
exe = EXE(pyz, a.scripts, [], exclude_binaries=True, name='12dAZ-Converter', console=False,
          version=version_info,
          icon=str(root / 'build' / '12daz.ico') if (root / 'build' / '12daz.ico').exists() else None,
          disable_windowed_traceback=False)
coll = COLLECT(exe, a.binaries, a.datas, strip=False, upx=False, name=f'12dAZ-Converter-{version}')
