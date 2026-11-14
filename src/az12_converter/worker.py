"""Spawn-safe process entry point; no GUI imports."""
import time

from .errors import Cancelled
from .service import Options, convert_file, inspect_file


def run_job(messages, cancel, job_id, action, path, output, option_values):
    last = [0.]

    def progress(stage, amount):
        now = time.monotonic()
        if now - last[0] >= 0.15:
            messages.put((job_id, "progress", (stage, amount)))
            last[0] = now

    try:
        options = Options(**option_values)
        if action in {"inspect", "preview"}:
            result = inspect_file(path, options, cancel=cancel, progress=progress)
        else:
            result = convert_file(path, output, options, cancel=cancel, progress=progress)
        messages.put((job_id, "result", result))
    except Cancelled as exc:
        messages.put((job_id, "cancelled", str(exc)))
    except Exception as exc:
        messages.put((job_id, "error", f"{type(exc).__name__}: {exc}"))
