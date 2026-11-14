import argparse
import json
import sys

from . import __version__
from .archive import Limits
from .errors import Cancelled, ConversionError
from .service import Options, convert_file, inspect_file


def main(argv=None):
    parser = argparse.ArgumentParser(description="Convert local 12daz/12da archives to 2D DXF 2018.")
    parser.add_argument("--version", action="version", version=__version__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("inspect", "convert"):
        sub = commands.add_parser(name)
        sub.add_argument("input")
        sub.add_argument("--report")
        sub.add_argument("--encoding", help="Explicit fallback encoding for files without a BOM")
        sub.add_argument("--model", action="append", dest="models", help="Select model by exact name; repeat to select several")
        sub.add_argument("--overwrite", action="store_true")
        sub.add_argument("--max-expanded-mib", type=int, default=1024)
        if name == "convert":
            sub.add_argument("--output", required=True)
            sub.add_argument("--units", required=True, choices=["metres", "millimetres", "feet", "inches"], help="Declare units; never rescale coordinates")
    args = parser.parse_args(argv)
    if args.max_expanded_mib <= 0:
        parser.error("--max-expanded-mib must be positive")
    options = Options(units=getattr(args, "units", None), models=args.models, overwrite=args.overwrite, encoding=args.encoding)
    try:
        kwargs = {"limits": Limits(expanded_bytes=args.max_expanded_mib * 1024 * 1024), "report_path": args.report, "preview_limit": 0}
        if args.command == "inspect":
            result = inspect_file(args.input, options, **kwargs)
        else:
            result = convert_file(args.input, args.output, options, **kwargs)
        print(json.dumps({k: v for k, v in result.items() if k != "preview"}, indent=2, ensure_ascii=False))
        return 2 if result["diagnostics"]["total"] else 0
    except (Cancelled, KeyboardInterrupt):
        print("Cancelled.", file=sys.stderr)
        return 3
    except (ConversionError, OSError) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
