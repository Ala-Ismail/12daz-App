# 12dAZ Converter

Offline Windows desktop converter from **12daz / 12da to editable DXF 2018**. Export in **Flatten 2D** mode to set Z to zero, or choose **Keep Z** to retain known source elevations on supported geometry. The converter writes one layer per model and runs without 12d Model or AutoCAD installed.

Version **0.1.3** is an initial working implementation. Independent CAD comparison and clean-machine release qualification remain pending. Read the conversion report: unsupported geometry, omitted labels/metadata and style substitutions are explicitly reported. DWG remains future work.

![12dAZ Converter dark grey desktop interface](docs/assets/12daz-converter-gui-v0.1.2.png)

*Screenshot uses synthetic demonstration data.*

## Code signing status and Windows warning

We are working toward trusted code signing for 12dAZ Converter and exploring the SignPath Foundation open-source programme. Signing is not yet approved or active, and the current **v0.1.3 Windows executable is unsigned**. In the meantime, Windows may display an **Unknown publisher** notice or a Microsoft Defender SmartScreen warning such as: **“Microsoft Defender SmartScreen prevented an unrecognized app from starting. Running this app might put your PC at risk.”**

Download only from the project's [official GitHub Releases](https://github.com/Ala-Ismail/12daz-App/releases) and verify the ZIP against its published `SHA256SUMS.txt`. A matching checksum verifies download integrity; it does not replace a publisher signature or establish that a program is safe. See the [Code signing policy](docs/code_signing_policy.md) for current status and release responsibilities.

## Run the Windows application

The local build is `dist/12dAZ-Converter-0.1.3/12dAZ-Converter.exe`. Distribute the complete `dist/12dAZ-Converter-0.1.3-Windows-x64.zip`; extract the whole archive and run the executable inside its folder. Keep its `_internal` folder beside it. End users do not need Python.

1. Open a `.12daz` or `.12da` file.
2. Review models and preview; select the models to export.
3. Choose drawing units, **Flatten 2D** or **Keep Z** under **Geometry**, and a `.dxf` destination. Declaring units does not rescale coordinates. The preview shows the source plan in either mode.
4. Select **Export DXF** and review warnings in **View report**. A `.dxf.conversion.json` report is saved beside the drawing.

**Help** provides an offline User Guide, About dialog, MIT license and dependency notices. A brief startup splash includes optional support through PayPal; it closes automatically after about 1.8 seconds, or immediately with Continue, Enter or Escape. Help > About contains the same support link. Cancel requests safe cleanup before output publication.

## Run from source

Use Python 3.11 or later with Tcl/Tk available. Install the pinned DXF dependency:

```powershell
python -m pip install -e .
python run_gui.py
```

The implementation uses **Tk/ttk**, Python's bundled desktop toolkit, and **ezdxf 1.3.5**. Tk replaced the proposed PySide6 dependency because downloads were unavailable in the development environment. Conversion runs in a separate process so the GUI stays responsive. A future Qt UI can reuse the conversion service.

Command line:

```powershell
python run_cli.py inspect "survey.12daz" --report "inspect.json"
python run_cli.py convert "survey.12daz" --output "survey_2D.dxf" --units metres
python run_cli.py convert "survey.12daz" --output "survey_3D.dxf" --units metres --geometry-mode keep-z
```

Repeat `--model "Model name"` to select models. Use `--overwrite` to replace existing drawing/report files, `--encoding cp1252` for a legacy no-BOM file, and `--max-expanded-mib` to change the default 1024 MiB expanded-input limit. Exit codes: 0 clean completion, 2 completion with warnings, 1 failure, 3 cancellation. A warning exit code can also indicate omitted attributes or a substituted style despite correct base plan geometry.

## Supported geometry and limits

| Source geometry | Flatten 2D (default) | Keep Z |
| --- | --- | --- |
| 2D/3D strings, super strings and legacy polylines | Exports straight segments and supported circular segments in XY. | Preserves known XYZ on straight strings and supports circular strings at a constant elevation. |
| Point strings, standalone arcs and circles, and basic single-line world-size text | Exports supported entities in XY. | Keeps explicit source elevations. Arcs must have a constant elevation. |
| Super alignments | Exports solved horizontal lines and circular arcs. If solved vertices are absent, it can reconstruct valid straight intersection-point layouts and valid open layouts with two fixed lines and a free-radius tangent arc. | Samples supported horizontal geometry against solved vertical grades and symmetric parabolas. A usable solved vertical profile is required. |
| Drainage pipe spans | Exports one polyline per verified pipe span, retaining intermediate plan vertices. | Pending verification of the source pipe centreline elevations. |

For Keep Z alignment sampling, **Plan tolerance** and **Elevation tolerance** each default to **0.01 drawing units**. Set them in the GUI or with `--plan-tolerance` and `--elevation-tolerance`. For active transitions with supported solved line-and-arc geometry, the report identifies that the export used the solved plan. Reconstruction of the original transition spirals is pending.

**4D strings and contours are deferred.** TINs, meshes, symbols, rendered annotations and paper/screen-size text are unsupported. The report identifies skipped objects, omitted alignment labels and drainage attributes, and style substitutions. The GUI preview shares a 30,000-point display allowance across selected models. Export uses the selected geometry and tolerances independently of that allowance.

Local sample checks found 91 convertible Keep Z alignments and reopened and audited 103 Flatten 2D pipe spans. See [validation status](docs/validation_status.md).

## Test and build

```powershell
python scripts/test.py
python -m pip install -r requirements.lock
python scripts/build.py
```

The test suite uses Python's standard `unittest`, synthetic fixtures and withdrawn Tk windows; no pytest dependency is required. Windows CI runs the suite and creates a ZIP artifact. Build-time license collection fails if required dependency/runtime notices are unavailable.

To smoke-test a built executable without showing its main window:

```powershell
& .\dist\12dAZ-Converter-0.1.3\12dAZ-Converter.exe --smoke-test "$PWD\outputs\packaged-smoke.json"
```

Create the report's parent directory first. This check runs Tk startup, spawned scanning/export, DXF audit and offline Help/About resources on synthetic data. It does not replace clean-machine qualification.

## GitHub builds and releases

Push to `main` to build the Windows package in GitHub Actions. Download the ZIP and checksum from the successful run's **Artifacts** section.

Push a matching version tag to prepare a **draft pre-release** automatically. For the existing `v0.1.2` tag, run **Actions > Build and release > Run workflow** on `main` and enter `v0.1.2` in `release_tag`. Review and publish the resulting draft from **Releases**. Existing published releases are preserved.

See [GitHub build and release steps](docs/github_release.md) for details.

## Project documents

- [Design and original GUI concept](12daz_design.md).
- [Current GUI screenshot](docs/assets/12daz-converter-gui-v0.1.2.png) using synthetic demonstration data.
- [Implementation steps](docs/implementation_steps.md).
- [Test requirements and cases](docs/test_requirements.md).
- [Validation status and remaining release checks](docs/validation_status.md).
- [Code signing policy](docs/code_signing_policy.md): current downloads are unsigned; SignPath is being explored, with no approval or active signing yet.
- [SignPath signing flow and setup](docs/signpath_setup.md), with an [application preparation packet](docs/signpath_application.md) for maintainer review.
- [Project agent guidance](.agent).

Private input belongs in ignored `sample/`; generated drawings and local reports belong in ignored `outputs/`. The GUI concept image contains illustrative data and predates the implemented Help menu and Tk controls.

## Support development

12dAZ Converter is free and open source. If you find it useful, [buy us a coffee via PayPal](https://paypal.me/12dazapp) to support ongoing development and maintenance.

Support is entirely optional. The same link is available on the startup splash screen and in **Help > About**. It opens your browser only when clicked; the converter does not process payments.

## License

Copyright (c) 2026 Lambda Technologies Pty Ltd ABN 22 647 165 160

Licensed under the [MIT License](LICENSE). Third-party dependencies retain their own licenses; private survey data is not licensed for redistribution by this project.
