"""Release identity used by SignPath's executable metadata restrictions."""
import importlib.util
from pathlib import Path
import runpy
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
build_version_info = runpy.run_path(str(ROOT / 'scripts/windows_version.py'))['build_version_info']


class WindowsVersionTests(unittest.TestCase):
    def test_rejects_unrepresentable_release_versions(self):
        for value in ('v1.2.3', '1.2', '1.2.3.4', '1.2.3-beta', '-1.2.3', '65536.0.0'):
            with self.subTest(version=value), self.assertRaises(ValueError):
                build_version_info(value)

    @unittest.skipUnless(sys.platform == 'win32' and importlib.util.find_spec('PyInstaller'),
                         'Windows build dependencies required')
    def test_release_identity_survives_version_resource_serialization(self):
        from PyInstaller.utils.win32.versioninfo import VSVersionInfo
        version = runpy.run_path(str(ROOT / 'src/az12_converter/__init__.py'))['__version__']
        major, minor, patch = map(int, version.split('.'))
        resource = VSVersionInfo()
        resource.fromRaw(build_version_info(version).toRaw())
        values = {item.name: item.val for item in resource.kids[0].kids[0].kids}
        self.assertEqual(values['ProductName'], '12dAZ Converter')
        self.assertEqual(values['ProductVersion'], version)
        self.assertEqual(values['FileVersion'], version)
        self.assertEqual(values['CompanyName'], 'Lambda Technologies Pty Ltd')
        self.assertEqual(values['OriginalFilename'], '12dAZ-Converter.exe')
        self.assertEqual(resource.ffi.productVersionMS, major << 16 | minor)
        self.assertEqual(resource.ffi.productVersionLS, patch << 16)
        self.assertEqual(resource.ffi.fileVersionMS, resource.ffi.productVersionMS)
        self.assertEqual(resource.ffi.fileVersionLS, resource.ffi.productVersionLS)


if __name__ == '__main__':
    unittest.main()
