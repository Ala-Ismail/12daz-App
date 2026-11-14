"""Run the test suite with the selected interpreter, without installing the project."""
from pathlib import Path
import os
import sys
import unittest

root = Path(__file__).resolve().parents[1]
if (root / '.runtime' / 'tcl8.6').is_dir():
    os.environ.setdefault('TCL_LIBRARY', str(root / '.runtime' / 'tcl8.6'))
    os.environ.setdefault('TK_LIBRARY', str(root / '.runtime' / 'tk8.6'))
sys.path.insert(0, str(root / 'src'))
suite = unittest.defaultTestLoader.discover(str(root / 'tests'))
if __name__ == '__main__':
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
