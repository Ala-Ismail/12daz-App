"""Windows release identity enforced by the signing artifact configuration."""
import re


def build_version_info(version):
    """Return a PyInstaller VERSIONINFO resource for a numeric release version."""
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError("Windows product version must be numeric major.minor.patch")
    parts = tuple(int(part) for part in version.split(".")) + (0,)
    if any(part > 65535 for part in parts):
        raise ValueError("Windows version components must fit an unsigned 16-bit field")
    from PyInstaller.utils.win32.versioninfo import (
        FixedFileInfo, StringFileInfo, StringStruct, StringTable,
        VarFileInfo, VarStruct, VSVersionInfo,
    )
    values = {
        "CompanyName": "Lambda Technologies Pty Ltd",
        "FileDescription": "12dAZ Converter",
        "FileVersion": version,
        "InternalName": "12dAZ-Converter",
        "LegalCopyright": "Copyright (c) 2026 Lambda Technologies Pty Ltd ABN 22 647 165 160",
        "OriginalFilename": "12dAZ-Converter.exe",
        "ProductName": "12dAZ Converter",
        "ProductVersion": version,
    }
    return VSVersionInfo(
        ffi=FixedFileInfo(filevers=parts, prodvers=parts),
        kids=[
            StringFileInfo([StringTable("040904B0", [StringStruct(k, v) for k, v in values.items()])]),
            VarFileInfo([VarStruct("Translation", [1033, 1200])]),
        ],
    )
