"""Collect notices from exactly the environment used to build the application."""
from importlib import metadata
from pathlib import Path
import shutil
import sys

ROOT = Path(__file__).resolve().parents[1]
RESOURCES = ROOT / 'src' / 'az12_converter' / 'resources'


def collect():
    destination = RESOURCES / 'dependency_licenses'
    destination.mkdir(exist_ok=True)
    sections = ['Third-party license notices\n\nCollected from the build environment. Original files are in dependency_licenses.\n']
    for name in ['ezdxf', 'numpy', 'fonttools', 'pyparsing', 'typing_extensions', 'charset-normalizer', 'pyinstaller']:
        dist = metadata.distribution(name)
        candidates = [f for f in (dist.files or []) if any(word in f.name.lower() for word in ('license', 'copying', 'notice')) and '.py' not in f.name.lower()]
        found = 0
        for relative in candidates:
            source = Path(dist.locate_file(relative))
            if source.is_file():
                # Preserve original paths so component licenses never overwrite one another.
                safe_parts = [p for p in relative.parts if p not in {'.', '..'}]
                target = destination / name / Path(*safe_parts)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)
                text = source.read_text(encoding='utf-8', errors='replace')
                sections.append(f'\n\n=== {name} {dist.version} / {relative} ===\n\n{text}')
                found += 1
        if not found:
            raise RuntimeError(f'No license found for {name}; distribution must not proceed.')
    runtime = Path(sys.base_prefix)
    candidates = [runtime / 'LICENSE.txt']
    candidates.extend((runtime / 'tcl').glob('*/license.terms'))
    candidates.extend((runtime / 'tcl').glob('*/license*'))
    seen = set()
    for source in candidates:
        if source.is_file() and source not in seen:
            seen.add(source)
            target = destination / 'runtime' / source.relative_to(runtime)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            sections.append(f'\n\n=== Python/Tcl/Tk / {source.relative_to(runtime)} ===\n\n' + source.read_text(encoding='utf-8', errors='replace'))
    if runtime / 'LICENSE.txt' not in seen or not any('tcl' in p.parts for p in seen):
        raise RuntimeError('Python or Tcl/Tk runtime notices missing.')
    (RESOURCES / 'LICENSE.txt').write_text((ROOT / 'LICENSE').read_text(encoding='utf-8-sig'), encoding='utf-8')
    (RESOURCES / 'THIRD_PARTY_NOTICES.txt').write_text(''.join(sections), encoding='utf-8')
    print(f'Collected notices from {len(sections) - 1} files.')


if __name__ == '__main__':
    collect()
