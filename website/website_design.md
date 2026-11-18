# 12dAZ Converter website design

## Purpose

Create a public website that introduces 12dAZ Converter and helps visitors download and start using the Windows application. Keep the website in this repository under `website/` and deploy that directory as a separate Vercel project.

This document is the design brief. The static website is implemented in `index.html` and `styles.css`; see `README.md` for local preview and Vercel deployment instructions. Vercel deployment is still to be done.

## Audience and primary action

The audience is surveyors, civil designers and CAD users who need to convert `.12daz` or `.12da` files into editable 2D DXF without installing 12d Model or AutoCAD.

The primary action is to download the Windows x64 application from a published GitHub release. Secondary actions are to see how it works, read supported geometry and limitations, and view the source or report an issue.

## Scope

Build one responsive landing page with navigation links to page sections. All conversion happens in the downloaded desktop application. The website needs no accounts, file uploads, conversion service, database or payment processing.

Start with plain HTML and CSS. Add JavaScript only if a specific interaction needs it. No framework or build step is required for this scope.

## Visual direction

- Use the application's dark grey theme with blue primary actions, readable light text and restrained borders.
- Make the application screenshot the main visual. Use generous spacing and a clear hierarchy instead of decorative illustrations.
- Use a system font stack, comfortable line lengths and strong heading contrast.
- Keep the download action prominent in the hero and repeat it near the bottom.
- On mobile, stack text and screenshots, keep buttons easy to tap and avoid horizontal page scrolling.
- Provide visible keyboard focus, descriptive links, appropriate colour contrast and reduced-motion support if animation is introduced.

## Page structure and proposed content

### 1. Header

Show the product name, **12dAZ Converter**, and links to Features, How it works, Download and GitHub. Add a **Buy me cuppa** button beside Download, pointing to <https://paypal.me/12dazapp>. Keep the two buttons together when the navigation wraps on small screens, without requiring a JavaScript menu.

### 2. Hero

**Headline:** Convert 12daz and 12da files into editable 2D DXF.

**Description:** An offline Windows application for exporting supported geometry to DXF 2018. No 12d Model, AutoCAD or Python installation needed.

**Primary action:** Download for Windows.

**Secondary action:** See how it works.

Show Windows x64 and offline operation beside the action. Show a version only when it matches the linked published release.

### 3. Application preview

Use the actual application capture at `website/assets/12daz-converter-gui.png`. Caption it as an application screenshot using synthetic demonstration data, and identify its version if it differs from the download.

Do not use the GUI proposal image as evidence of implemented behaviour. Preserve the screenshot's aspect ratio and provide useful alternative text.

### 4. Features

Present a compact set of concrete benefits:

- Open `.12daz` archives and `.12da` files.
- Review models and a 2D preview before export.
- Select the models to include in the drawing.
- Preserve source XY coordinates and supported circular arcs, with Z flattened to zero.
- Export editable DXF 2018 with one layer per model.
- Review conversion warnings and the saved conversion report.
- Convert locally, keeping survey files on the user's computer.

Avoid claims about universal compatibility, certified fidelity or performance without supporting validation.

### 5. How it works

1. Download the Windows ZIP and extract the complete archive.
2. Run `12dAZ-Converter.exe`, keeping its `_internal` folder beside it.
3. Open a source file, review the preview and select models.
4. Choose drawing units and a DXF destination, export and review the report.

Explain that choosing units declares drawing units; it does not rescale coordinates.

### 6. Supported geometry and limitations

Summarise the supported straight and circular string geometry, points and basic text using the current README as the source of truth.

Make the limits visible: output is 2D DXF; DWG is future work. TINs, meshes, drainage, transition curves, symbols, rendered annotations and paper/screen-size text are not implemented. Unsupported content can be omitted and styles substituted, with warnings reported.

State that the application is an initial working implementation and that independent CAD comparison and clean-machine release qualification remain pending, unless the repository's validation evidence has been updated before publication.

### 7. Download

Show the version, Windows x64 platform, ZIP format and extraction instructions. Host application packages and checksums on GitHub Releases, keeping large binaries out of the website directory.

Repository: <https://github.com/Ala-Ismail/12daz-App>

Release listing: <https://github.com/Ala-Ismail/12daz-App/releases>

Before implementation is published, verify which release and ZIP asset are publicly available. Pin the download button to that verified release asset and link its release notes and checksum when available. Update the displayed version and links together for each release. Do not infer public availability from a local build, source version or draft release.

If no public package is available, use **View releases** and state that a download is not yet available. Do not show an active direct-download button for a missing asset.

### 8. FAQ

- **Do I need Python, AutoCAD or 12d Model?** No, the packaged Windows application includes its runtime.
- **Are my files uploaded?** Conversion takes place locally in the desktop application; the website does not accept survey files.
- **Does it preserve elevations?** Output is 2D and Z is flattened to zero.
- **Does it export DWG?** No, the current output is DXF 2018.
- **Does changing units rescale my drawing?** No, unit selection declares drawing units without rescaling coordinates.
- **Where can I report a problem?** Link to the repository's GitHub Issues page, and ask users to describe the issue without posting private survey data.

### 9. Closing support section

At the bottom of the page, after the FAQ and before the footer, show an actual screenshot of the application GUI with `outputs/coffee_and_muffin.12da` open beside the 12dAZ Converter name and the heading **Would you buy me a cuppa?**. Capture the real loaded application, including model selection, drawing preview and export controls; use `website/assets/12daz-coffee-and-muffin-gui.png` and label its version and synthetic demonstration data. Link the **Buy me a cuppa** button to <https://paypal.me/12dazapp> and make clear that support is optional. Stack the image and text on mobile. Keep the main application screenshot in the hero section.

### 10. Footer

Link to GitHub, release notes, issues and the MIT license.

Also link **Code signing policy** to `code-signing-policy.html` from the footer and download section. The policy page explains current signing status, team responsibilities and privacy. State that v0.1.3 is unsigned and multiple signing options are under exploration; keep the wording provider-neutral and update that status only when evidence supports it. Keep the public page aligned with `docs/code_signing_policy.md`.

Include the exact notice:

Copyright (c) 2026 Lambda Technologies Pty Ltd ABN 22 647 165 160.

Include a **Buy me coffee** link using the owner-supplied URL, <https://paypal.me/12dazapp>. Supporting development is optional for visitors. Keep the link visible in the footer and repeat it beneath the download section, secondary to the download action.

## File layout

```text
website/
  README.md
  website_design.md
  index.html
  code-signing-policy.html
  styles.css
  assets/
    favicon.svg
    12daz-coffee-and-muffin-gui.png
    12daz-converter-gui.png
```

The page and assets are self-contained in this directory. Add a `vercel.json` only if deployment needs configuration beyond the static project settings.

## Vercel deployment plan

1. Implement and review the page locally.
2. Import the existing `Ala-Ismail/12daz-App` repository into Vercel as a new project.
3. Set the project's Root Directory to `website`.
4. Configure it as a static HTML site without an install or build command; serve the website directory containing `index.html`.
5. Verify a preview deployment, including screenshot paths, mobile layout and all release links.
6. Publish the production deployment and attach a custom domain if desired.

The desktop build and GitHub release workflows remain independent of website hosting. No new repository is required, and no Sites hosting registration is needed for the requested Vercel deployment.

## Acceptance criteria

- Visitors can understand the input formats, output format and Windows requirement from the first screen.
- Download links resolve to a verified public package, or the page clearly explains that a package is not yet available.
- The page provides complete extraction and launch instructions.
- A **Buy me coffee** link appears beneath the download section and in the footer, pointing to `https://paypal.me/12dazapp`.
- Product claims match `readme.md` and `docs/validation_status.md` at publication time.
- The actual application screenshot is readable and labelled as synthetic demonstration data.
- The layout works on mobile and desktop, and links are usable with a keyboard.
- Page title, meta description, semantic headings and image alternative text are present.
- No private survey files, credentials, generated desktop builds or unnecessary tracking are included.
- The static page deploys from `website/` on Vercel without requiring the Python application or its dependencies.
