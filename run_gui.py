"""Development and PyInstaller entry point."""
import multiprocessing
import os
from pathlib import Path
import sys

if not getattr(sys, "frozen", False):
    root = Path(__file__).resolve().parent
    sys.path.insert(0, str(root / "src"))
    if (root / '.runtime' / 'tcl8.6').is_dir():
        os.environ.setdefault('TCL_LIBRARY', str(root / '.runtime' / 'tcl8.6'))
        os.environ.setdefault('TK_LIBRARY', str(root / '.runtime' / 'tk8.6'))

if __name__ == "__main__":
    multiprocessing.freeze_support()
    if len(sys.argv) == 3 and sys.argv[1] == '--smoke-test':
        from az12_converter.smoke import run_smoke
        raise SystemExit(run_smoke(sys.argv[2]))
    else:
        from az12_converter.app import main
        main()
