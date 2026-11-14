"""Create the standalone Windows GUI folder and a ZIP distribution."""
from pathlib import Path
import os
import base64
import runpy
import shutil
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    version = runpy.run_path(str(ROOT / 'src' / 'az12_converter' / '__init__.py'))['__version__']
    sys.path.insert(0, str(ROOT / 'src'))
    from az12_converter.icons import icon_data
    png = base64.b64decode(icon_data('logo', '#0969da', 64))
    icon = ROOT / 'build' / '12daz.ico'
    icon.parent.mkdir(exist_ok=True)
    icon.write_bytes(struct.pack('<HHH', 0, 1, 1) + struct.pack('<BBBBHHII', 64, 64, 0, 0, 1, 32, len(png), 22) + png)
    if (ROOT / '.runtime' / 'tcl8.6').is_dir():
        os.environ.setdefault('TCL_LIBRARY', str(ROOT / '.runtime' / 'tcl8.6'))
        os.environ.setdefault('TK_LIBRARY', str(ROOT / '.runtime' / 'tk8.6'))
    subprocess.run([sys.executable, str(ROOT / 'scripts' / 'collect_licenses.py')], cwd=ROOT, check=True)
    subprocess.run([sys.executable, '-m', 'PyInstaller', '--noconfirm', '--clean', str(ROOT / '12daz-converter.spec')], cwd=ROOT, check=True)
    folder = ROOT / 'dist' / f'12dAZ-Converter-{version}'
    shutil.copyfile(ROOT / 'LICENSE', folder / 'LICENSE')
    shutil.copyfile(ROOT / 'src' / 'az12_converter' / 'resources' / 'user_guide.txt', folder / 'USER_GUIDE.txt')
    (folder / 'README.txt').write_text(f'12dAZ Converter {version}\n\nRun 12dAZ-Converter.exe. Keep _internal beside it.\nOpen a 12daz/12da file, choose models and drawing units, then Export DXF.\nSee USER_GUIDE.txt and Help > About / License for details.\n\nThis initial version reports unsupported content and style substitutions.\nIndependent CAD and clean-machine qualification remain pending.\n', encoding='utf-8')
    archive = shutil.make_archive(str(ROOT / 'dist' / f'12dAZ-Converter-{version}-Windows-x64'), 'zip', folder.parent, folder.name)
    print(f'Windows distribution: {archive}')


if __name__ == '__main__':
    main()
