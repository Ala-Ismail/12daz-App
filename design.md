# Map backgrounds for 12dAZ Converter

This design brings CadCraft’s MGA assignment, control-point alignment, and three online basemaps into the 12dAZ plan preview. Reuse CadCraft’s portable map code under its MIT license and adapt the viewport and application state to the 12dAZ Rust desktop. The [converter design](12daz_design.md) describes the archive and DXF workflow.

## Desktop workflow

1. Open a `.12daz` or `.12da` file and select the models to preview.
2. Open **View → Background Map**. Keep the default **Off**, or assign a source coordinate reference system (CRS) and confirm the source coordinate units.
3. For a survey already on MGA, choose **GDA2020** or **GDA94** and the existing zone. The map uses the survey eastings and northings directly when the source is in metres. For other confirmed source units, convert the length to metres in map calculations only. The DXF drawing-units setting continues to declare units without rescaling geometry. It can suggest a map unit, but the map setup requires confirmation.
4. For a local grid, choose **Align to control points**. Pick or enter a plan point and its known map position as MGA easting/northing or WGS 84 latitude/longitude. One pair establishes a translation after unit conversion. An optional second pair derives a uniform scale and rotation. Show the scale and rotation for confirmation and allow the alignment to be cleared. A selected MGA zone by itself cannot place a local grid.
5. Choose **OpenStreetMap Streets**, **Google Satellite with Labels**, or **Esri World Imagery with Labels**. Show opacity, provider attribution and the current map status in the preview. Pan, zoom and Fit continue to use the existing survey viewport. Map imagery sits below the selected geometry.

List GDA2020 / MGA zones 46–56 (EPSG:7846–7856) and GDA94 / MGA zones 46–56 (EPSG:28346–28356), grouped by datum. Keep WGS 84 (EPSG:4326) as the interchange CRS and Web Mercator (EPSG:3857) for the tile grid. Do not infer datum, zone or units from coordinate magnitude. Reject non-finite points, duplicate control pairs and degenerate alignments. Check placement against an independently known point where available. [ICSM datum guidance](https://www.icsm.gov.au/sites/default/files/DatumMattersFactSheet3.pdf)

## Providers

Use a single active provider with its own authentication, zoom limits, attribution and cache rules. A failure leaves the survey visible and reports the cause below the preview. The user chooses any replacement provider explicitly.

| Provider | Requests and credentials | Display and caching |
| --- | --- | --- |
| OpenStreetMap Streets | Load visible PNG tiles from `https://tile.openstreetmap.org/{z}/{x}/{y}.png`. Identify the app using a 12dAZ User-Agent. No key is required. | Keep OpenStreetMap attribution visible. Honour HTTP freshness and conditional requests. Use the policy’s minimum cache duration where freshness headers cannot be read. Do not prefetch or offer bulk or offline downloads. [OSM tile policy](https://operations.osmfoundation.org/policies/tiles/) |
| Google Satellite with Labels | Use the Google Maps Platform Map Tiles API with a user-supplied key, an enabled Map Tiles API and billing. Create a satellite session with Google’s roadmap overlay. Request viewport metadata for available imagery zoom and copyright before painting that viewport. | Display the Google Maps branding and current viewport copyright without obstruction. Use memory-only caching within the applicable terms and response headers. Discard expired tiles and sessions. [Google 2D tiles](https://developers.google.com/maps/documentation/tile/2d-tiles-overview) · [Google tiles policy](https://developers.google.com/maps/documentation/tile/policies) |
| Esri World Imagery with Labels | Use the authenticated World Imagery service and the `arcgis/imagery/labels` Static Basemap Tiles overlay. A user-supplied ArcGIS Location Platform key needs access to both services. Keep imagery and label requests distinct. | Composite Esri labels over Esri imagery. Display Esri and the applicable imagery and label-source credits. Keep permitted decoded tiles in memory. [Esri service data](https://developers.arcgis.com/rest/basemap-styles/service-data/) · [Esri Static Basemap Tiles](https://developers.arcgis.com/documentation/mapping-and-location-services/mapping/basemaps/arcgis-styles-tiles/) |

Keep keys in memory by default. Offer an explicit **Remember key** option through the Windows credential store. When the user selects Google or Esri, load the saved key for that provider if the session has no active key. Show map attribution at the lower left with outlined text over the imagery. Never put provider credentials, session tokens or authenticated URLs in project files, reports, diagnostics or screenshots. Explain that enabling online maps sends the provider the visible tile coordinates and, where required, the viewport extent. Map Off and an unconfigured file make no map requests.

## Rust integration

- Put typed CRS, source-unit conversion, registration and XYZ math in a UI-independent Rust module. Use `proj4rs` behind an allowlist of supported CRS definitions. Cache the projection objects. Verify datum operations against independent fixtures. Use the GDA94-to-GDA2020 conformal transform (EPSG:8048) before approximating GDA2020 geographic coordinates as WGS 84 for display. Label the overlay as visual context because it omits site-specific distortion and epoch corrections. [Geoscience Australia transform guidance](https://www.ga.gov.au/scientific-topics/positioning-navigation/positioning-australia/geodesy/transform-convert)
- Use the existing plan preview’s screen-to-drawing transform to find the visible tile footprint and its drawing-to-screen transform to place tile meshes underneath survey records. Project tile positions through the inverse registration. Keep calculations in `f64` until screen placement. Check projected tile edges at broad zooms and subdivide a mesh if corner-only placement exceeds the display error tolerance. Preserve Fit extents and the existing preview geometry budget. [12dAZ preview canvas](rust/src/desktop/canvas.rs)
- Put network requests and tile decoding behind a provider interface. Use a bounded worker queue and validate response type, bytes and decoded dimensions. Request only visible tiles. After a pan or zoom, keep any requests still relevant to the viewport. Invalidate pending results when the archive, provider or registration changes. Keep disk-cache maintenance and network work off the GUI thread, and limit both in-flight requests and cached tiles.
- Keep source CRS, confirmed source units and optional controls per archive. Save only on explicit action in a versioned sidecar such as `survey.12daz.12daz-map.json`, bound to the source fingerprint. Write atomically. Warn on invalid, stale or unsupported sidecars and open the archive normally. Keep tiles, credentials and UI preferences out of the sidecar.
- Keep map state in the desktop. The archive reader, CLI conversion, DXF encoder, survey coordinates and conversion report are independent of the basemap. Rendering, selecting or aligning a map never rewrites source or output geometry.

## Implementation and acceptance

Use the [background map test requirements](docs/background_map_test_requirements.md) for fixtures, per-phase cases and the CadCraft coverage review.

| Phase | Pass criteria |
| --- | --- |
| 1. MGA assignment and math | Check known points from both datums and more than one MGA zone against independent reference coordinates, including GDA2020 / MGA zone 55. Test easting/northing axis order, metres versus millimetres, wrong or missing units and CRS, and invalid numeric input. |
| 2. Control points and preview | Test one-pair translation, two-pair scale and rotation, duplicate or invalid points, archive switching and sidecar round-trip. Use synthetic tiles to check alignment through pan, zoom and Fit at supported display scales. Run the desktop layout and panel-resize regression tests. |
| 3. Three provider adapters | Use mocked responses for all three providers to verify request axes, attribution, freshness, bounds, invalid credentials, switching providers and late responses. Verify no requests with Map Off or missing CRS. Then open each provider with an authorised account where required and check attribution and alignment at a known location. |
| 4. Package and regression | Compare reopened DXF geometry and conversion report counts and diagnostics across matching exports with the map off and with each provider active. Run offline Windows package smoke and native layout checks. Publish the map feature after all three providers pass, with user documentation on location disclosure, source units and alignment. |

The CadCraft review used revision `39245810a9b2a0a1aee4adabba51ccf8a16b7eb7`. Its MGA selector and tile renderer are in `crates/ui-egui/src/map.rs`, its control-point commands are in `crates/engine/src/cmd/map.rs`, and its projection code is in `crates/map/src/lib.rs`. The provider caches are in `crates/ui-egui/src/map_tiles.rs`.
