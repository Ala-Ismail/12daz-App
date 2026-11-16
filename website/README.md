# 12dAZ Converter website

Static product and download page. No dependencies, JavaScript or build step.

## Preview locally

From the repository root:

```powershell
python -m http.server 8000 --directory website --bind 127.0.0.1
```

Open <http://127.0.0.1:8000>. Stop the server with Ctrl+C.

## Deploy to Vercel

Import `Ala-Ismail/12daz-App` as a new Vercel project with these settings:

| Setting | Value |
| --- | --- |
| Root Directory | `website` |
| Framework Preset | Other |
| Build Command | Override and leave empty |
| Output Directory | `.` |
| Install Command | Override and leave empty |

Deploy and check the resulting preview before attaching a production domain. The website is independent of the Python application and its GitHub Actions build. No Vercel configuration or account credentials are stored here.

Settings reference: [Vercel build configuration](https://vercel.com/docs/builds/configure-a-build).

## Verification

Local Chrome checks passed at viewport widths of 320, 375, 680, 768, 1024 and 1440 pixels: no horizontal page overflow, screenshot loading, internal section links, both download buttons and both support links. Keyboard checks covered the skip link and FAQ toggling, and reduced-motion styling was verified. No browser errors or local asset failures were observed. Desktop and mobile screenshots were visually reviewed.

The v0.1.3 ZIP and checksum URLs both returned HTTP 200 on 15 November 2026. Hosting on Vercel has not yet been verified.

## Updating a download

`index.html` currently links to the publicly available v0.1.3 pre-release. The ZIP is 28,321,416 bytes (28.3 MB). Releases and binaries stay on GitHub.

When publishing a new desktop version:

1. Verify the release and both its Windows ZIP and checksum are public.
2. Update both download buttons, the release-notes link and checksum link together.
3. Update the version labels, preview/release status and ZIP size.
4. Check product claims against `readme.md` and `docs/validation_status.md`.
5. If changing the screenshot, copy an actual application screenshot into `assets/`, update image dimensions and alternative text, and label its version and demonstration data accurately.

The support link is <https://paypal.me/12dazapp>, labelled **Buy me coffee** beneath the download panel and in the footer. A closing section above the footer pairs an actual application screenshot with **Would you buy me a cuppa?** and a **Buy me a cuppa** button. Its image, `assets/12daz-coffee-and-muffin-gui.png`, captures the v0.1.3 GUI after opening `outputs/coffee_and_muffin.12da` through the application's normal loading workflow, with all five models selected and units set to millimetres. The original synthetic drawing and generated DXF remain in ignored `outputs/`.

See [website_design.md](website_design.md) for the design brief.

## Code signing policy

`code-signing-policy.html` is the public policy page, linked from the download section and footer. Keep its summary aligned with `docs/code_signing_policy.md`. The website states that v0.1.3 is unsigned and multiple signing options are being explored; keep the wording provider-neutral until a provider is selected and signing is activated. The download section also displays the signing status, possible Unknown publisher / SmartScreen warning and checksum guidance. Update both pages and the download notice when a signed release becomes available.

## Hero image

The homepage hero uses `assets/surveyor-hero.webp` with a smaller 960px variant. It is an AI-generated field scene with a synthetic survey and an illustrative interface, labelled in the visible caption. It is not an application screenshot or evidence of survey fidelity. The actual application screenshot remains directly below the hero. On smaller screens the photo follows the headline and download controls so both remain readable.
