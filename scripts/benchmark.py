"""Measure a direct conversion; Windows peak working set includes this process only."""
import argparse
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import platform
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'src'))
from az12_converter.service import convert_file, Options


class MemoryCounters(ctypes.Structure):
    _fields_ = [(name, typ) for name, typ in [
        ('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD),
        ('PeakWorkingSetSize', ctypes.c_size_t), ('WorkingSetSize', ctypes.c_size_t),
        ('QuotaPeakPagedPoolUsage', ctypes.c_size_t), ('QuotaPagedPoolUsage', ctypes.c_size_t),
        ('QuotaPeakNonPagedPoolUsage', ctypes.c_size_t), ('QuotaNonPagedPoolUsage', ctypes.c_size_t),
        ('PagefileUsage', ctypes.c_size_t), ('PeakPagefileUsage', ctypes.c_size_t)]]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('input')
    parser.add_argument('--output', required=True)
    parser.add_argument('--evidence', required=True)
    parser.add_argument('--units', required=True, choices=['metres', 'millimetres', 'feet', 'inches'])
    args = parser.parse_args()
    started = time.perf_counter()
    result = convert_file(args.input, args.output, Options(units=args.units, overwrite=True), preview_limit=0)
    evidence = {k: result[k] for k in ['application_version', 'input_sha256', 'encoding', 'archive_version', 'source_counts', 'emitted_entities', 'elapsed_seconds', 'status']}
    evidence['diagnostic_counts'] = result['diagnostics']['counts']
    evidence['environment'] = {'python': sys.version, 'os': platform.platform(), 'machine': platform.machine(), 'processor': platform.processor()}
    evidence['wall_seconds'] = round(time.perf_counter() - started, 3)
    evidence['measurement_scope'] = 'Direct CLI-equivalent conversion process only; GUI/process-tree peak and clean-VM performance not measured.'
    if sys.platform == 'win32':
        kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        kernel.GetCurrentProcess.restype = wintypes.HANDLE
        psapi = ctypes.WinDLL('psapi', use_last_error=True)
        psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(MemoryCounters), wintypes.DWORD]
        counters = MemoryCounters()
        counters.cb = ctypes.sizeof(counters)
        if psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(), ctypes.byref(counters), counters.cb):
            evidence['peak_working_set_bytes'] = counters.PeakWorkingSetSize
            evidence['peak_commit_bytes'] = counters.PeakPagefileUsage
    Path(args.evidence).write_text(json.dumps(evidence, indent=2), encoding='utf-8')
    print(json.dumps(evidence, indent=2))


if __name__ == '__main__':
    main()
