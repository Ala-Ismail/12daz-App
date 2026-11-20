# 12dAZ Converter website

Static product and download page. No dependencies or build step. A small local JavaScript file controls the hero slideshow.

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

Deploy and check the resulting preview before attaching a production domain. The static site deploys independently of the converter's Windows build and contains no conversion service.

Settings reference: [Vercel build configuration](https://vercel.com/docs/builds/configure-a-build).

## Verification

Local Chrome checks passed at viewport widths of 320, 375, 680, 768, 1024 and 1440 pixels: no horizontal page overflow, screenshot loading, internal section links, both download buttons and both support links. Keyboard checks covered the skip link and FAQ toggling, and reduced-motion styling was verified. No browser errors or local asset failures were observed. Desktop and mobile screenshots were visually reviewed.

The v0.2.0 homepage was checked locally at 320, 500, 768 and 1280 pixels. The history table fits at all four widths, the approved GUI screenshot loads at 1330 × 849 pixels, and both download buttons target the same Windows ZIP.

## Updating a download

`index.html` links to the v0.3.0 pre-release, Windows x64 ZIP and checksum on GitHub. The main [desktop screenshot](assets/12daz-converter-rust-0.2.0.png) shows the Rust application with synthetic data. The [migration validation](../docs/rust_migration_validation.md) records the checks.

When publishing a new desktop version:

1. Verify the release and both its Windows ZIP and checksum are public.
2. Update both download buttons, the release-notes link and checksum link together.
3. Update the version labels and preview/release status.
4. Check product claims against `readme.md` and the validation document for the release.
5. Get maintainer approval for the revised GUI before updating the public sample. Copy the approved application screenshot into `assets/`, update image dimensions and alternative text, and label its version and demonstration data accurately.

The support link is <https://paypal.me/12dazapp>, labelled **Buy me coffee** beneath the download panel and in the footer. A closing section above the footer pairs an actual application screenshot with **Would you buy me a cuppa?** and a **Buy me a cuppa** button. Its image, `assets/12daz-coffee-and-muffin-gui.png`, captures the v0.1.5 GUI after opening `outputs/coffee_and_muffin.12da` through the application's normal loading workflow, with all five models selected and units set to millimetres. The original synthetic drawing and generated DXF remain in ignored `outputs/`.

See [website_design.md](website_design.md) for the design brief.

## Code signing policy

`code-signing-policy.html` is the public policy page, linked from the download section and footer. Keep its summary aligned with `docs/code_signing_policy.md`. The v0.3.0 package is unsigned. Update the policy page and download notice together when the signing status changes.

## Hero image

The homepage hero uses `assets/surveyor-hero.webp` with a smaller 960px variant. It is an AI-generated field scene with a synthetic survey and an illustrative interface, labelled in the visible caption. It is not an application screenshot or evidence of survey fidelity. The actual application screenshot remains directly below the hero. The slideshow forms a shared background behind the top navigation and hero title, with dark overlays for readable text on desktop and mobile.

The hero cycles through road, port and open-pit mining survey scenes every five seconds with a short crossfade. Its pause/resume button supports keyboard use. Automatic cycling stops when the tab is hidden and defaults to paused when reduced motion is requested. Without JavaScript the first scene remains visible. Images share a fixed layout so changing slides does not move the content.
