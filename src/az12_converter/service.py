"""Shared inspect/convert jobs, bounded diagnostics and atomic publication."""
from collections import Counter
from dataclasses import dataclass, asdict
import json
import math
import os
from pathlib import Path
import tempfile
import time
import uuid

from . import __version__
from .archive import Limits, input_hash, open_input
from .errors import ConversionError, checkpoint
from .geometry import UnsupportedGeometry, convert_geometry, preview_points
from .parser import iter_entities, fields, scalar
from .preview import PreviewCollector


@dataclass
class Options:
    units: str | None = None
    models: list[str] | None = None
    overwrite: bool = False
    encoding: str | None = None
    geometry_mode: str = "flatten-2d"
    plan_tolerance: float = 0.01
    elevation_tolerance: float = 0.01
    linear_arc_elevations: bool = False


class Diagnostics:
    def __init__(self, detail_limit=500):
        self.counts = Counter()
        self.details = []
        self.limit = detail_limit
        self.detail_counts = Counter()

    def add(self, code, message, source=None):
        self.counts[code] += 1
        if len(self.details) < self.limit and self.detail_counts[code] < 50:
            detail = {"code": code, "message": message}
            if source:
                detail.update(source_id=source.ordinal, model=source.model, line=source.line)
            self.details.append(detail)
            self.detail_counts[code] += 1

    def result(self):
        return {"counts": dict(self.counts), "total": sum(self.counts.values()), "details": self.details,
                "details_truncated": sum(self.counts.values()) > len(self.details)}


def same_path(a, b):
    a, b = Path(a), Path(b)
    if a.resolve() == b.resolve():
        return True
    return a.exists() and b.exists() and os.path.samefile(a, b)


def check_destinations(source, output, report=None):
    if same_path(source, output):
        raise ConversionError("Output must not replace the input file.")
    if Path(output).suffix.lower() != ".dxf":
        raise ConversionError("Output filename must end in .dxf.")
    if not Path(output).parent.is_dir():
        raise ConversionError("Output directory does not exist.")
    if report and (same_path(source, report) or same_path(output, report)):
        raise ConversionError("Report path must differ from input and DXF output.")


def atomic_json(path, report, overwrite=True):
    path = Path(path)
    temp = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", suffix=".tmp", prefix=".12daz-", dir=path.parent, delete=False) as stream:
            temp = Path(stream.name)
            json.dump(report, stream, indent=2, ensure_ascii=False)
            stream.flush()
            os.fsync(stream.fileno())
        publish(temp, path, overwrite)
    finally:
        if temp:
            temp.unlink(missing_ok=True)


def publish(temp, output, overwrite):
    if overwrite:
        os.replace(temp, output)
    else:
        # Link creates a new name atomically and fails if another process won the race.
        os.link(temp, output)
        temp.unlink()


def _job(path, options, *, output=None, report_path=None, limits=Limits(), cancel=None, progress=None, preview_limit=30000):
    if options.geometry_mode not in {"flatten-2d", "keep-z"}:
        raise ConversionError("Unknown output geometry mode.")
    keep_z = options.geometry_mode == "keep-z"
    for label, value in (("Plan", options.plan_tolerance), ("Elevation", options.elevation_tolerance)):
        if not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
            raise ConversionError(f"{label} tolerance must be a positive finite number in drawing units.")
    start = time.perf_counter()
    source_path = Path(path)
    if output:
        report_path = Path(report_path) if report_path else Path(str(output) + ".conversion.json")
        check_destinations(path, output, report_path)
        if not options.overwrite and (Path(output).exists() or report_path.exists()):
            raise ConversionError("DXF or report already exists; choose another name or explicitly enable overwrite.")
    elif report_path and same_path(path, report_path):
        raise ConversionError("Report must not replace the input file.")
    writer = None
    if output:
        from .export.dxf import DxfWriter
        writer = DxfWriter(options.units)
    digest = input_hash(source_path, cancel)
    diagnostics = Diagnostics()
    summaries = {}
    preview = PreviewCollector(preview_limit, cancel)
    selected = exact = approximated = skipped = 0
    selected_models = {m.casefold() for m in options.models} if options.models is not None else None
    bounds = None

    def command_warning(key, value, line):
        diagnostics.add("UNKNOWN_COMMAND", f"Unrecognized top-level command {key!r} at line {line}.")

    with open_input(path, limits=limits, encoding=options.encoding, cancel=cancel, progress=progress) as archive:
        for entity in iter_entities(archive.chunks(), limits, cancel, command_warning):
            checkpoint(cancel)
            model_key = entity.model.casefold()
            summary = summaries.setdefault(model_key, {"name": entity.model, "count": 0, "convertible": 0, "types": Counter()})
            summary["count"] += 1
            summary["types"][entity.kind] += 1
            if selected_models is not None and model_key not in selected_models:
                continue
            entity.model = summary["name"]
            selected += 1
            warning_before = sum(diagnostics.counts.values())

            def warn(code, message):
                diagnostics.add(code, message, entity)

            try:
                geometry = convert_geometry(entity, warn, keep_z, cancel, plan_tolerance=options.plan_tolerance,
                                            elevation_tolerance=options.elevation_tolerance,
                                            linear_arc_elevations=options.linear_arc_elevations)
            except UnsupportedGeometry as exc:
                skipped += 1
                warn("UNSUPPORTED_OR_INVALID_GEOMETRY", str(exc))
                continue
            summary["convertible"] += 1
            for primitive in geometry:
                if writer:
                    writer.write(primitive, entity.model, warn)
                points = preview_points(primitive)
                # Bounds include display-sampled curves; DXF geometry itself remains exact.
                for x, y in points:
                    if bounds is None:
                        bounds = [x, y, x, y]
                    else:
                        bounds = [min(bounds[0], x), min(bounds[1], y), max(bounds[2], x), max(bounds[3], y)]
                preview.add(entity.model, primitive, points)
            if sum(diagnostics.counts.values()) > warning_before:
                approximated += 1
            else:
                exact += 1
            if progress:
                progress("Converting" if output else "Inspecting", selected)
        metadata = archive.metadata
        if archive.extra_members:
            diagnostics.add("EXTRA_ARCHIVE_MEMBERS", f"Ignored {archive.extra_members} non-payload archive entries.")
    if selected == 0:
        raise ConversionError("Input contains no objects in the selected models.")
    if not metadata.get("archive_version"):
        diagnostics.add("ARCHIVE_VERSION_UNVERIFIED", "No archive_version comment; compatibility is determined by recognized syntax only.")
    elif not metadata["archive_version"].startswith("15."):
        diagnostics.add("ARCHIVE_VERSION_UNVERIFIED", "Archive version is outside the V15 grammar baseline; recognized syntax only is converted.")
    if input_hash(source_path, cancel) != digest:
        raise ConversionError("Input changed during the job; output was not published.")
    result = {"schema_version": 1, "job_id": str(uuid.uuid4()), "application_version": __version__,
              "input": str(source_path.resolve()), "input_sha256": digest, **metadata, "options": asdict(options),
              "models": [{**v, "types": dict(v["types"])} for v in summaries.values()],
              "source_counts": {"selected": selected, "exact": exact, "approximated": approximated, "skipped": skipped},
              "bounds": bounds, "bounds_note": "Preview bounds; curved extents sampled for display.",
              "preview": preview.result(), "preview_simplified": preview.limited,
              "preview_omitted_models": preview.omitted_models(),
              "preview_note": "Preview prioritizes path shape and spatial coverage within the shared display allowance. DXF export uses independent geometry.",
              "units": options.units, "emitted_entities": writer.count if writer else None,
              "layer_mapping": writer.layers if writer else {}, "diagnostics": diagnostics.result(), "published": False}
    if output:
        if not writer.count:
            raise ConversionError("Selection contains no convertible geometry; no DXF was published.")
        temp = None
        try:
            with tempfile.NamedTemporaryFile(dir=Path(output).parent, prefix=".12daz-", suffix=".dxf", delete=False) as file:
                temp = Path(file.name)
            if progress:
                progress("Writing and validating DXF", writer.count)
            checkpoint(cancel)
            writer.save(temp)
            from .export.dxf import validate_dxf
            validate_dxf(temp, writer.count, cancel, **({"keep_z": True} if keep_z else {}))
            checkpoint(cancel)  # The final cancellable boundary, immediately before commit.
            publish(temp, Path(output), options.overwrite)
            result["published"] = True
            result["output"] = str(Path(output).resolve())
        finally:
            if temp:
                temp.unlink(missing_ok=True)
    result["elapsed_seconds"] = round(time.perf_counter() - start, 3)
    result["status"] = "complete_with_warnings" if diagnostics.counts else "complete"
    if report_path:
        try:
            atomic_json(report_path, {k: v for k, v in result.items() if k != "preview"}, options.overwrite)
            result["report_path"] = str(Path(report_path).resolve())
        except OSError as exc:
            if not result["published"]:
                raise ConversionError(f"Cannot save inspect report: {exc}") from exc
            diagnostics.add("REPORT_SAVE_FAILED", f"DXF was published, but its report could not be saved: {exc}")
            result["diagnostics"] = diagnostics.result()
            result["status"] = "complete_with_warnings"
    return result


def inspect_file(path, options=None, **kwargs):
    try:
        return _job(path, options or Options(), **kwargs)
    except OSError as exc:
        raise ConversionError(f"Cannot inspect input: {exc}") from exc


def convert_file(path, output, options, **kwargs):
    try:
        return _job(path, options, output=output, **kwargs)
    except OSError as exc:
        raise ConversionError(f"Cannot write output: {exc}") from exc
