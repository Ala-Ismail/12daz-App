# Background map test requirements

These are acceptance tests for the [12dAZ background map design](../design.md). Run each phase’s automated tests before progressing. Complete the live provider verification before a public map release. Use synthetic archives and mock responses for the automated cases. Use authorised provider accounts only for the final manual checks.

## Fixtures and test evidence

- **MGA reference points.** Use independently published or independently computed controls for GDA2020 and GDA94 in at least two MGA zones, including a GDA2020 / MGA zone 55 point away from the central meridian. Record the CRS, axis order, source units, datum operation, expected geographic coordinates and tolerance before running the test. Test round trips separately from absolute agreement so a paired forward and inverse error cannot pass.
- **Equivalent unit archives.** Generate a small survey containing a point, a closed outline and a varying-Z line. Store the same locations in metres and millimetres. Record the expected original DXF coordinates and unit declarations separately for each archive. Add a confirmed feet/inches example when those map unit options are exposed.
- **Local-grid controls.** Use drawing point `(100, 200)` and MGA point `(300000, 5800000)` for the translation case. For rotation and scale, add drawing point `(110, 200)` and MGA point `(300000, 5800020)`. This gives scale 2 and rotation +90°. The unused check point `(100, 210)` must map to `(299980, 5800000)` with both pairs, and to `(300000, 5800010)` with the first pair alone. Check these synthetic projected positions within `1e-6 m`.
- **Tile responses.** Generate labelled corner tiles without third-party pixels. Include different X and Y indices, missing metadata, expired sessions, delayed and failed responses, cache validators, corrupt content and excessive image dimensions. Keep provider credentials and private surveys out of fixtures and evidence.

## Phase 1. CRS assignment, units and projection

| ID | Exercise | Pass result |
| --- | --- | --- |
| MAP-01 | Assign the matching datum and MGA zone to the independent control fixtures. Compare forward and inverse results with the recorded expectations. Exercise known zones from both datums and the XYZ tile latitude/zoom bounds. | Coordinate and tile results meet the fixture tolerances. The selected datum operation is visible in the test evidence. Equivalent forward and inverse errors cannot mask a bad absolute result. |
| MAP-02 | Open the metre and millimetre versions of the same survey, confirm the source units and enable a mocked map. Repeat with source units unset and with alternate valid unit selections. | Correctly declared equivalent archives place the same survey point at the same map position. Unconfirmed units block the map. Changing map units leaves the stored survey values and DXF units untouched. |
| MAP-03 | Supply an unsupported CRS, non-finite coordinates and out-of-domain coordinates. Reopen the archive when the previous file had an assigned CRS. | Invalid settings give an actionable message and issue zero tile requests. The new file uses only its own valid saved settings or stays unconfigured. No datum/zone is inferred from coordinate size. |

## Phase 2. Controls, persistence and preview

| ID | Exercise | Pass result |
| --- | --- | --- |
| MAP-04 | Apply the local-grid fixture first with one matched point, then with two. Exercise MGA coordinates directly and WGS 84 input using an independent reference. Check the unused third point and clear the alignment. | One pair translates; two pairs derive the expected uniform scale and rotation. Both control points and the independent check point land within tolerance. Clearing a local-grid alignment removes the overlay until it is registered again. |
| MAP-05 | Enter a partial second pair, duplicate control points, reversed or out-of-range latitude/longitude, and a non-finite or degenerate transform. | Validation explains the failure, preserves the last valid alignment and prevents requests based on rejected input. |
| MAP-06 | Save and reopen a registration. Try an absent, malformed, oversized, unsupported-version or stale-fingerprint sidecar. Simulate a failed replacement of a valid file. | Valid settings round-trip. Invalid sidecars leave the survey open and the map unconfigured. A failed save preserves the prior sidecar. The source archive is unchanged and the sidecar contains no credentials or imagery. |
| MAP-07 | Use synthetic marked tiles and survey geometry at multiple zooms. Pan, zoom, Fit, resize the window and drag the export divider at 100%, 125%, 150%, 175% and 200% display scaling. Exercise broad extents and repeat with the map off. | Survey points and tile markers stay aligned within one logical pixel for the synthetic layout fixture. Survey geometry stays above imagery, the preview fits the selected models, and required attribution fits without covering the export controls. Pointer movement alone leaves the export divider stable. |

## Phase 3. Provider requests, display and failure states

| ID | Exercise | Pass result |
| --- | --- | --- |
| MAP-08 | Open archives with Map Off, missing CRS or missing units. Then enable a provider and turn it off while a response is delayed. | Disabled and unconfigured states produce zero requests. An in-flight response after turning the map off cannot repaint the overlay. |
| MAP-09 | OpenStreetMap. Verify the app-specific User-Agent, HTTPS XYZ addressing using unequal X and Y, and visible attribution. Apply fresh, expired, conditional and missing cache headers. Rapidly pan and zoom. | Only visible tiles are requested. Expired entries revalidate correctly, HTTP cache rules and the fallback retention rule are followed, and attribution stays visible. Request and cache limits hold. |
| MAP-10 | Google. Simulate missing and rejected keys, satellite sessions, viewport metadata, expiry and renewal. Include overlapping global and high-resolution rectangles, neighboring rectangles that together cover the preview, and a gap in high-resolution coverage. | A session and the current viewport copyright are required before rendering. The app uses the finest coverage that spans the viewport. Google branding and current copyright remain visible. Tile files and session tokens are absent from disk. |
| MAP-11 | Esri. Simulate World Imagery and the labels overlay with distinct X and Y indices. Return imagery and labels in reverse order and simulate a missing entitlement for either service. | Each endpoint uses its documented request order and authentication. The labels composite over matching Esri imagery when both are ready. Applicable credits are visible. An entitlement failure is reported without switching provider or persisting tiles. |
| MAP-12 | Delay responses while switching archive, provider, CRS and viewport. Inject disconnects, HTTP errors, oversized payloads and corrupt images. Check normal diagnostics, reports and saved preferences. | Stale tiles and attribution cannot appear for the new state. Slow or bad responses leave the plan usable. Visible requests stay bounded and failures avoid rapid retries. Keys, tokens and authenticated URLs are absent from diagnostics, settings and reports. |
| MAP-13 | Open all three actual providers against a synthetic or public known-location survey. Use authorised provider credentials where required. Pan and zoom while recording attribution, alignment and errors without keys. | Each provider displays expected imagery, retains the required credits and aligns visually at the reference location. Capture the date, app version, provider, CRS, source units, account type where relevant and result. |

## Phase 4. Windows package and conversion parity

| ID | Exercise | Pass result |
| --- | --- | --- |
| MAP-14 | Export the same fixtures with the map off and with each provider selected. Repeat after a control-point alignment in Flatten 2D and Keep Z. Compare against matching CLI exports using an independent DXF reader. | Source archive hashes and canonical geometry, coordinates, elevations, units, layers and conversion diagnostics agree across runs. There are no basemap entities or credentials in the DXF or report. |
| MAP-15 | Smoke-test the extracted Windows ZIP offline from a standard-user account. Test startup, ordinary preview and both exports. Select an online map using a registered archive, then switch Map Off. | Offline conversion and help work. An unavailable basemap gives an actionable status and leaves survey preview and export usable. The existing panel-width regression and the desktop stress test pass. |

## CadCraft coverage used as a reference

Reviewed revision `39245810a9b2a0a1aee4adabba51ccf8a16b7eb7`. The 12 CadCraft map-core tests passed locally. The source review also identified the following suites. Their coverage guides the 12dAZ cases and does not count as acceptance evidence for the new feature.

| CadCraft suite | Useful coverage |
| --- | --- |
| `crates/map/src/lib.rs` | MGA zone 55 false origin, all offered MGA zones, registration invariants, tile bounds, Google and Esri request construction |
| `crates/engine/src/cmd/map.rs` | CRS assignment, control-point translation and scale/rotation, degenerate pairs, unchanged drawing and DXF export |
| `crates/io/src/map_sidecar.rs` | Sidecar round trip, missing/oversized sidecars, unchanged drawing |
| `crates/ui-egui/src/map.rs` and `map_tiles.rs` | No requests with Map Off or missing registration, request limits, attribution gating, cache freshness, satellite session and credential handling |

For each execution, record the case ID, fixture version, source commit, Windows version and display scale where applicable, expected and observed results, and a link to redacted logs or screenshots. Mark unrun cases explicitly.

## Run the automated tests

From the repository root on Windows, with dependencies cached:

```powershell
cargo test --manifest-path rust/Cargo.toml --locked --offline --features desktop
cargo test --manifest-path rust/Cargo.toml --locked --offline --no-default-features
cargo test --manifest-path rust/Cargo.toml --locked --offline --features desktop rust_19_repeats_load_export_and_cancel -- --ignored
```

The one-tile OpenStreetMap probe is opt-in. It requests a single tile at a synthetic public location. Run it with network access:

```powershell
cargo test --manifest-path rust/Cargo.toml --locked --offline --features desktop map_13_openstreetmap_downloads_one_public_tile -- --ignored
```

## v0.3.0 local preview evidence

Independent controls for both GDA datums and several MGA zones were computed with PROJ 9.5.1. The Rust tests compare the forward and inverse results separately. Additional tests cover map-only unit conversion, control-point registration, bounded sidecars, tile placement, provider responses, credentials gating and unchanged DXF output.

The native GUI displayed OpenStreetMap tiles behind the synthetic Zone 55 survey, with attribution visible. Google and Esri passed automated request and response tests. A live Google check on a local MGA56 survey confirmed that the preview retains requested zoom 18 when Google reports complete coverage up to zoom 20, even when the same response also contains a broad zoom-9 fallback. The road overlay and attribution were visually checked. Esri’s live imagery check remains separate.

The local preview passed 54 standard Rust tests, the separate desktop stress test and five Python-to-Rust CLI parity cases. The extracted Windows x64 ZIP passed the native graphics and software OpenGL smoke tests with a system-only PATH. The verified ZIP SHA-256 is `75e0c8b0bf56ab0c752675291011fb0232680e128bdd9368835276d9765c4c69`.
