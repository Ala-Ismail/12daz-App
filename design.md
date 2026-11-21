# 12dAZ Converter plan preview design

## 1. Background maps

This design brings CadCraft’s MGA assignment, control-point alignment, and three online basemaps into the 12dAZ plan preview. Reuse CadCraft’s portable map code under its MIT license and adapt the viewport and application state to the 12dAZ Rust desktop. The [converter design](12daz_design.md) describes the archive and DXF workflow.

### 1.1 Desktop workflow

1. Open a `.12daz` or `.12da` file and select the models to preview.
2. Open **View → Background Map**. Keep the default **Off**, or assign a source coordinate reference system (CRS) and confirm the source coordinate units.
3. For a survey already on MGA, choose **GDA2020** or **GDA94** and the existing zone. The map uses the survey eastings and northings directly when the source is in metres. For other confirmed source units, convert the length to metres in map calculations only. The DXF drawing-units setting continues to declare units without rescaling geometry. It can suggest a map unit, but the map setup requires confirmation.
4. For a local grid, choose **Align to control points**. Pick or enter a plan point and its known map position as MGA easting/northing or WGS 84 latitude/longitude. One pair establishes a translation after unit conversion. An optional second pair derives a uniform scale and rotation. Show the scale and rotation for confirmation and allow the alignment to be cleared. A selected MGA zone by itself cannot place a local grid.
5. Choose **OpenStreetMap Streets**, **Google Satellite with Labels**, or **Esri World Imagery with Labels**. Show opacity, provider attribution and the current map status in the preview. Pan, zoom and Fit continue to use the existing survey viewport. Map imagery sits below the selected geometry.

List GDA2020 / MGA zones 46–56 (EPSG:7846–7856) and GDA94 / MGA zones 46–56 (EPSG:28346–28356), grouped by datum. Keep WGS 84 (EPSG:4326) as the interchange CRS and Web Mercator (EPSG:3857) for the tile grid. Do not infer datum, zone or units from coordinate magnitude. Reject non-finite points, duplicate control pairs and degenerate alignments. Check placement against an independently known point where available. [ICSM datum guidance](https://www.icsm.gov.au/sites/default/files/DatumMattersFactSheet3.pdf)

Extend the projection selector with a [Brisbane City Survey Grid](#16-brisbane-city-survey-grid) group for both published definitions.

The proposed [MGA detection](#15-automatic-mga-detection-and-assignment) adds an offline check and can fill in the registration when the file provides enough evidence.

### 1.2 Providers

Use a single active provider with its own authentication, zoom limits, attribution and cache rules. A failure leaves the survey visible and reports the cause below the preview. The user chooses any replacement provider explicitly.

| Provider | Requests and credentials | Display and caching |
| --- | --- | --- |
| OpenStreetMap Streets | Load visible PNG tiles from `https://tile.openstreetmap.org/{z}/{x}/{y}.png`. Identify the app using a 12dAZ User-Agent. No key is required. | Keep OpenStreetMap attribution visible. Honour HTTP freshness and conditional requests. Use the policy’s minimum cache duration where freshness headers cannot be read. Do not prefetch or offer bulk or offline downloads. [OSM tile policy](https://operations.osmfoundation.org/policies/tiles/) |
| Google Satellite with Labels | Use the Google Maps Platform Map Tiles API with a user-supplied key, an enabled Map Tiles API and billing. Create a satellite session with Google’s roadmap overlay. Request viewport metadata for available imagery zoom and copyright before painting that viewport. | Display the Google Maps branding and current viewport copyright without obstruction. Use memory-only caching within the applicable terms and response headers. Discard expired tiles and sessions. [Google 2D tiles](https://developers.google.com/maps/documentation/tile/2d-tiles-overview) · [Google tiles policy](https://developers.google.com/maps/documentation/tile/policies) |
| Esri World Imagery with Labels | Use the authenticated World Imagery service and the `arcgis/imagery/labels` Static Basemap Tiles overlay. A user-supplied ArcGIS Location Platform key needs access to both services. Keep imagery and label requests distinct. | Composite Esri labels over Esri imagery. Display Esri and the applicable imagery and label-source credits. Keep permitted decoded tiles in memory. [Esri service data](https://developers.arcgis.com/rest/basemap-styles/service-data/) · [Esri Static Basemap Tiles](https://developers.arcgis.com/documentation/mapping-and-location-services/mapping/basemaps/arcgis-styles-tiles/) |

Keep keys in memory by default. Offer an explicit **Remember key** option through the Windows credential store. When the user selects Google or Esri, load the saved key for that provider if the session has no active key. Show map attribution at the lower left with outlined text over the imagery. Never put provider credentials, session tokens or authenticated URLs in project files, reports, diagnostics or screenshots. Explain that enabling online maps sends the provider the visible tile coordinates and, where required, the viewport extent. Map Off and an unconfigured file make no map requests.

### 1.3 Rust integration

- Put typed CRS, source-unit conversion, registration and XYZ math in a UI-independent Rust module. Use `proj4rs` behind an allowlist of supported CRS definitions. Cache the projection objects. Verify datum operations against independent fixtures. Use the GDA94-to-GDA2020 conformal transform (EPSG:8048) before approximating GDA2020 geographic coordinates as WGS 84 for display. Label the overlay as visual context because it omits site-specific distortion and epoch corrections. [Geoscience Australia transform guidance](https://www.ga.gov.au/scientific-topics/positioning-navigation/positioning-australia/geodesy/transform-convert)
- Use the existing plan preview’s screen-to-drawing transform to find the visible tile footprint and its drawing-to-screen transform to place tile meshes underneath survey records. Project tile positions through the inverse registration. Keep calculations in `f64` until screen placement. Check projected tile edges at broad zooms and subdivide a mesh if corner-only placement exceeds the display error tolerance. Preserve Fit extents and the existing preview geometry budget. [12dAZ preview canvas](rust/src/desktop/canvas.rs)
- Put network requests and tile decoding behind a provider interface. Use a bounded worker queue and validate response type, bytes and decoded dimensions. Request only visible tiles. After a pan or zoom, keep any requests still relevant to the viewport. Invalidate pending results when the archive, provider or registration changes. Keep disk-cache maintenance and network work off the GUI thread, and limit both in-flight requests and cached tiles.
- Keep source CRS, confirmed source units and optional controls per archive. Save only on explicit action in a versioned sidecar such as `survey.12daz.12daz-map.json`, bound to the source fingerprint. Write atomically. Warn on invalid, stale or unsupported sidecars and open the archive normally. Keep tiles, credentials and UI preferences out of the sidecar.
- Keep map state in the desktop. The archive reader, CLI conversion, DXF encoder, survey coordinates and conversion report are independent of the basemap. Rendering, selecting or aligning a map never rewrites source or output geometry.

### 1.4 Implementation and acceptance

Use the [background map test requirements](docs/background_map_test_requirements.md) for fixtures, per-phase cases and the CadCraft coverage review.

| Phase | Pass criteria |
| --- | --- |
| 1. MGA assignment and math | Check known points from both datums and more than one MGA zone against independent reference coordinates, including GDA2020 / MGA zone 55. Test easting/northing axis order, metres versus millimetres, wrong or missing units and CRS, and invalid numeric input. |
| 2. Control points and preview | Test one-pair translation, two-pair scale and rotation, duplicate or invalid points, archive switching and sidecar round-trip. Use synthetic tiles to check alignment through pan, zoom and Fit at supported display scales. Run the desktop layout and panel-resize regression tests. |
| 3. Three provider adapters | Use mocked responses for all three providers to verify request axes, attribution, freshness, bounds, invalid credentials, switching providers and late responses. Verify no requests with Map Off or missing CRS. Then open each provider with an authorised account where required and check attribution and alignment at a known location. |
| 4. Package and regression | Compare reopened DXF geometry and conversion report counts and diagnostics across matching exports with the map off and with each provider active. Run offline Windows package smoke and native layout checks. Publish the map feature after all three providers pass, with user documentation on location disclosure, source units and alignment. |

The CadCraft review used revision `39245810a9b2a0a1aee4adabba51ccf8a16b7eb7`. Its MGA selector and tile renderer are in `crates/ui-egui/src/map.rs`, its control-point commands are in `crates/engine/src/cmd/map.rs`, and its projection code is in `crates/map/src/lib.rs`. The provider caches are in `crates/ui-egui/src/map_tiles.rs`.

### 1.5 Automatic MGA detection and assignment

Check each opened `.12daz` or `.12da` for survey coordinates that could be on the Map Grid of Australia. MGA zones share the same false easting and southern false northing. The same XY pair can fall within the valid range of more than one zone, and a local grid can use similar values. Coordinate ranges can screen candidates. Assigning the datum, zone and units requires independent evidence. [Geoscience Australia MGA2020](https://www.ga.gov.au/scientific-topics/positioning-navigation/positioning-australia/geodesy/datums-projections/grid2020) · [Geoscience Australia MGA94](https://www.ga.gov.au/mapspecs/topographic/v5/appendix_m.html)

#### 1.5.1 Background Map settings

1. Run the check locally in the background when a survey opens. Show its result under **View → Background Map**. Offer **Detect MGA** for the automatic road-corridor sweep and **Check known location** for optional coordinate screening.
2. Apply a valid saved alignment for the current archive first. Keep manual settings and control-point alignment in effect until the user explicitly replaces them. Clear the previous archive's suggestions when a new archive opens.
3. Show the result and the evidence used. Use an explicit **Apply candidate** action for suggestions. Allow the user to choose **Already on selected grid** or **Local grid** when the result is inconclusive.

| Result | Background Map behaviour |
| --- | --- |
| **Automatically assigned** | The archive declares a supported projected CRS and the source units are declared or user-confirmed. The archive's reliable XY is consistent with that declaration and there is no conflicting registration evidence. Apply a session registration and show the datum, grid or zone, units and metadata source. |
| **Projection candidates found** | The coordinates pass the range check, or the user supplied a known location that narrows the options. Present compatible grids or zones and unresolved datums or units for confirmation, even if only one candidate remains. Explain what evidence would resolve the ambiguity. |
| **Inconclusive or conflicting** | Show the reason, such as insufficient reliable coordinates, inconsistent model extents, conflicting declarations, or no candidate in the supported area. Offer manual CRS selection or local-grid alignment. |

Keep coordinate screening and assignment from verified declarations offline. The user can start the road-corridor sweep or a [temporary OpenStreetMap comparison](#154-temporary-openstreetmap-comparison) for a selected candidate. Persist the registration only when the user selects **Save alignment**. Detection and registration affect only map placement. DXF export and source coordinates are independent of the result.

#### 1.5.2 Coordinate screening and evidence

- Base the check on finite XY from recognised coordinate fields. Collect per-model extents and a bounded, deterministic sample across the archive as part of the existing background scan. Use source coordinates before export filtering or preview simplification. A model selection or change from **Flatten 2D** to **Keep Z** must leave the detected registration usable. A sparse file with clear metadata can be assigned if its available XY passes the check. A lone unreferenced point is inconclusive.
- Evaluate the supported GDA94 and GDA2020 MGA zones and the [Brisbane City Survey Grid definitions](#16-brisbane-city-survey-grid) using the projected CRS allowlist. Use explicitly declared or confirmed source units where available. With unknown units, test supported unit conversions to screen candidates and label units as unconfirmed. Keep X as easting and Y as northing. Offer an axis-order warning if reversing the axes would produce plausible coordinates. Leave axis correction to the user or source data.
- Inverse-project the candidate XY to geographic coordinates and compare the result to the supported CRS's published area of use. Use a documented tolerance at zone boundaries so a project using an adjacent-zone grid can still be reviewed. Check per-model extents as well as the sample so a large model cannot hide conflicting coordinates from a small model. Report which models disagree and require confirmation if the evidence conflicts.
- Treat only documented CRS and unit fields verified against representative 12d exports as archive declarations. Filenames, free-text model names, `archive_version`, DXF unit preferences and coordinate magnitudes may supply context or prompt a question. They cannot establish a CRS. A user-supplied project location can narrow zones offline, while independent survey metadata or control evidence is needed to resolve the datum. Reuse the existing control-point workflow for sites on a local grid.

#### 1.5.3 Rust integration and acceptance

Use the [MGA detection acceptance tests](docs/mga_detection_acceptance_tests.md) for fixtures, pass criteria and release evidence.

- Keep candidate evaluation in the Rust map module. Pass the bounded coordinate summary and provenance from the archive scan to the desktop's background map state. Key the results to the archive fingerprint and job revision. Discard stale results after an archive switch and preserve an alignment applied by the user while detection was running.
- Test a synthetic MGA55 survey with a supported declaration, the same survey with only XY, and equivalent metre and millimetre coordinates. Verify automatic assignment for the declared case and confirmation for the XY-only case. Verify that plausible coordinates in different zones or datums remain unresolved without independent evidence.
- Test a local grid that resembles MGA, a swapped-axis fixture, a declared survey near a zone boundary, mixed model extents, and conflicting or unsupported declarations. Verify that uncertain evidence leaves the registration unassigned and that a valid saved or manual alignment takes precedence.
- Verify that opening a new archive discards old detections, that detection sends zero tile requests, and that saving a confirmed alignment uses the existing fingerprint-bound sidecar. Compare reopened DXFs and conversion reports with detection enabled and disabled.

#### 1.5.4 Temporary OpenStreetMap comparison

1. Under **View → Background Map**, select a projection candidate with confirmed source units and choose **Compare on OpenStreetMap**. Before the first request, explain that OpenStreetMap receives the visible tile location. Start the comparison only after the user confirms.
2. Overlay the selected survey models on **OpenStreetMap Streets** using the candidate's datum and zone. Show the candidate name, attribution and adjustable opacity. Suggest comparing road shapes and junctions first, then checking any visible outlines that might correspond to survey boundaries. Show one candidate at a time and request tiles only for the visible viewport. Reuse the normal OpenStreetMap cache and attribution. [OpenStreetMap tile usage policy](https://operations.osmfoundation.org/policies/tiles/)
3. End the comparison after 30 seconds, when the user selects **End comparison**, or when the archive changes. On the same archive, restore the previous provider, registration, view and opacity. Drop late tile results from the comparison. When tiles fail to load, report that the comparison was unavailable and retain the existing recommendations. Applying a candidate remains an explicit action.

For an automatic recommendation, support an optional local OpenStreetMap vector extract with known coverage, date and attribution. The separate [visible road-shape check](#18-narrowing-projection-candidates-and-checking-road-shapes) uses pixels already displayed during a comparison. Approximate vector matches using the shape and orientation of road centrelines, bends and junctions that the user identifies, as well as their distances to counterpart features. Give independently mapped boundary outlines, such as a park edge or road reserve, secondary weight where a like-for-like feature is available. Missing outlines are neutral. Boundary overlap alone cannot recommend a zone, and outlines from the same road geometry do not count as independent evidence. Evaluate coordinates as placed by the candidate without fitting an offset or rotation. Require several separated matching features and a clear lead over the other evaluated zones. Report which road shapes and possible boundaries contributed to the recommendation. [OpenStreetMap data and licensing](https://www.openstreetmap.org/copyright)

Without suitable vector data, offer the visual comparison. If coverage is partial, features are sparse or multiple candidates fit, report **Inconclusive** for the automatic match and keep the original candidate list. A useful result may recommend a likely zone. Treat any matching outlines as approximate map features. Survey-grade or legal boundaries and the choice between GDA94 and GDA2020 require independent evidence. The recommendation always requires confirmation.

Accept a local WGS 84 GeoJSON FeatureCollection up to 8 MiB with explicit top-level `coverage: [west, south, east, north]` and `source_date` fields. Use OpenStreetMap highway tags for reference roads and mapped park, protected-area or road-reserve outlines for optional boundary evidence. Use the selected road and boundary models from the plan preview for the approximate comparison.

### 1.6 Brisbane City Survey Grid

Add **Brisbane City Survey Grid** as a group in **View → Background Map** and label both entries with their datum and EPSG code. Support older surveys in BCSG02 and newer surveys in BCSG2020.

| Projection | EPSG code | False easting | False northing |
| --- | --- | ---: | ---: |
| GDA94 / BCSG02 | [EPSG:3113](https://epsg.io/3113) | 50,000 m | 100,000 m |
| GDA2020 / BCSG2020 | [EPSG:20047](https://epsg.io/20047) | 150,000 m | 200,000 m |

Both definitions use Transverse Mercator on GRS80, with a latitude of origin of 28° S, a central meridian of 153° E and a scale factor of 0.99999. Use each definition's own offsets and datum. For background map placement, use the existing GDA94-to-GDA2020 conformal transform for BCSG02. Record the exact EPSG code and confirmed source units in the alignment sidecar.

Use **Already on selected grid** for surveys stored in either Brisbane grid. Retain **Local grid** for surveys that need control points to align to the selected coordinate system. Include both Brisbane definitions in offline candidate screening using their published area of use. A trusted matching declaration can support automatic assignment. Plausible XY alone yields a suggestion, and an OpenStreetMap shape match remains approximate. Map placement leaves source and DXF coordinates unchanged.

Use the [Brisbane projection acceptance cases](docs/mga_detection_acceptance_tests.md#7-brisbane-city-survey-grid) to verify the projection definitions, placement and assignment decisions.

### 1.7 Implementation steps for projection suggestions and Brisbane grids

Implement these phases in Rust in the order shown. Run each phase's gate before starting the next phase. Use the fixtures and case IDs in the [acceptance test document](docs/mga_detection_acceptance_tests.md).

#### 1.7.1 Establish reference fixtures

- Prepare the MGA, Brisbane, ambiguous, mixed-model and boundary fixtures as plain `.12da` files and wrapped `.12daz` archives. Verify the Brisbane forward and inverse reference coordinates independently of the app. Record the published areas of use and the boundary tolerance used in candidate screening.
- Review representative 12d exports for actual CRS and unit declarations. Record the recognised syntax and provenance for each supported format. Leave automatic assignment unavailable for declarations whose syntax has insufficient evidence. Keep coordinate-only suggestions available for those files.
- **Gate** Verify the reference data for MGA-DETECT-01 through 04, 09, and 24 through 26. Keep verified declarations, absent declarations and deliberately incorrect declarations in separate fixtures.

#### 1.7.2 Add manual Brisbane grid support

- Extend the typed CRS allowlist in [the Rust map module](rust/src/map.rs) with EPSG:3113 and EPSG:20047. Reuse the existing projection wrapper and datum operations. Apply the conformal GDA94-to-GDA2020 step to BCSG02 after inverse projection. Keep both false origins explicit in the CRS definitions.
- Add the Brisbane entries to the Background Map selector. Support direct assignment, source-unit conversion and local control-point alignment through [the existing registration model](rust/src/map/registration.rs). Verify that saved alignment files preserve the exact EPSG code and that existing MGA sidecars still load. Use a version change only if the on-disk format needs one.
- **Gate** Pass MGA-DETECT-24, 25, 27 and 28. Run the existing projection, registration and sidecar tests.

#### 1.7.3 Collect archive evidence

- Extend the desktop archive scan to collect supported declarations and their provenance, per-model XY extents, and a bounded deterministic sample. Read recognised source coordinates before preview simplification or export filtering. Keep the collector in the existing cancellable scan and keep its results out of conversion report serialization.
- Preserve separate evidence for absent, invalid, unsupported and conflicting declarations. Pass the summary to the desktop with the archive fingerprint and scan revision.
- **Gate** Verify the scan inputs for MGA-DETECT-01 through 04, 08, 10, 11 and 14. Check that reversing model order, changing model selection or changing export mode leaves the relevant evidence consistent.

#### 1.7.4 Evaluate candidates offline

- Evaluate each allowlisted MGA and Brisbane CRS using the shared projection code. Screen the sample and model extents in confirmed or candidate units against the published area of use. Apply the boundary tolerance and report possible swapped axes. Use a known project location, when supplied, to narrow the candidate list.
- Return the outcome, candidate CRS and units, and supporting or conflicting evidence. Offer automatic assignment only when a verified file declaration, confirmed units and the coordinate checks agree. Return candidates for plausible XY without a verified declaration. Keep a clear distinction between inconclusive evidence and a conflicting declaration.
- **Gate** Pass the Rust evaluator assertions for MGA-DETECT-01 through 11 and 26, including the same coordinates under the wrong Brisbane definition.

#### 1.7.5 Integrate suggestions into Background Map

- Show the result, evidence and candidate list in [Background Map settings](rust/src/desktop/backdrop.rs). Provide **Detect MGA** for the automatic sweep and **Check known location** for manual location screening. Allow the user to apply a candidate, enter a CRS manually or align a local grid. Keep any existing saved or manually applied registration until the user replaces it.
- Reject stale results after an archive switch or manual assignment. Keep automatic assignment session-only until **Save alignment** is selected. Detection runs with the basemap off, and a provider requests tiles only when explicitly enabled for the survey.
- **Gate** Pass desktop MGA-DETECT-01 through 17 and 26. Build the Windows candidate, run the package smoke test and reopen matched DXF exports to verify geometry, coordinates and conversion report parity.

#### 1.7.6 Add the optional OpenStreetMap comparison

- First add the user-confirmed, 30-second visual comparison. Reuse existing visible-viewport tile requests and restore the previous map state when the comparison ends. Keep the candidate registration temporary. Pass MGA-DETECT-18, 19 and the visual tile-failure checks in 22.
- Then add optional ranking from a local vector extract. Compare road shapes and junctions as primary evidence and like-for-like boundary outlines as supporting evidence. Leave ties, partial coverage and sparse matches inconclusive. Show the evidence and keep applying the recommendation an explicit action.
- **Gate** Pass MGA-DETECT-20 through 23 with local vectors and mocked providers. Repeat the package smoke test, archive-switch checks and DXF parity checks after integrating the comparison.

#### 1.7.7 Narrow candidates and check visible road shapes

- Add the known-zone and confirmed-unit filters. Verify the list, selected candidate and local vector comparison use the same filtered options.
- Add the raster check to the user-confirmed OpenStreetMap comparison. Run it against synthetic tiles before testing the desktop integration.
- **Gate** Pass MGA-DETECT-29 through 35. Verify zero extra tile requests from the raster check, then rerun the Windows package smoke test and DXF parity case.

### 1.8 Narrowing projection candidates and checking road shapes

#### 1.8.1 Filter by a known MGA zone and units

Under **Projection suggestions**, add an optional **Known MGA zone** filter. Leave it unset initially so both MGA and Brisbane grids remain eligible. Apply confirmed source units to the displayed list. With metres confirmed and zone 54 selected, the GHD road design sample should show the GDA94 and GDA2020 Zone 54 choices from its 18 metre candidates.

Keep the full offline detection result so the user can clear the filters. When filtering excludes the selected candidate, clear the selection and any comparison result based on that selection. Show an empty result explicitly when the chosen zone and units do not fit. Use the filtered candidates for the local vector comparison and require **Apply candidate** to activate a choice.

#### 1.8.2 Check roads in the visible OpenStreetMap image

1. Start the existing temporary OpenStreetMap comparison for a chosen candidate and use confirmed source units. Select a survey road centreline model, zoom to street level and choose **Check visible road match**. Tell the user when the displayed tiles or road detail are insufficient.
2. Use a small, bounded set of separated probes along the visible survey road lines, including bends and multiple headings. Project the probes to pixels in the displayed tiles using the candidate as entered. Look for narrow road-colour bands or paired road edges that continue in the along-road direction and contrast across the road. Ignore isolated dots and broad uniform areas. Keep the search radius tied to map scale and record the thresholds used. Road colours are rendering cues and can vary with the map style and zoom. [OpenStreetMap Carto road style](https://github.com/openstreetmap-carto/openstreetmap-carto/blob/master/style/roads.mss)
3. Report the number of aligned and evaluable probes, the viewport and zoom, and an approximate result. Return **Inconclusive** when the matching features are too sparse, too similar, or too obscured to distinguish a candidate. Compare scores only across candidates checked on the same survey road segment at comparable scale and coverage. Leave unchecked candidates visible and unscored.

The first implementation checks tile zoom 15 or closer and evaluates at most 72 separated probes. It requires at least six evaluable probes in two distinct directions. To recommend a grid relative to another, at least 70% of the probes must align and the lead must be at least 20 percentage points. The compared readings must share the same drawing viewport, survey model, zoom and evaluable probe IDs.

Use only pixels already loaded for the current, user-visible viewport. The check makes zero additional tile requests and uses temporary in-memory image data. Discard late results after a candidate, viewport, archive or source-unit change. Label completed scores with the candidate and viewport they used, and clear them when the archive or source units change. Follow the normal comparison timeout and restore the previous map state. [OpenStreetMap tile usage policy](https://operations.osmfoundation.org/policies/tiles/)

Treat the score as evidence of likely alignment. Visible boundary outlines can help the user inspect the result. Use the separate vector comparison for an automated like-for-like boundary check. A raster result leaves the datum and final registration decision with the user.

### 1.9 OpenStreetMap road boundaries beneath the survey

Add a proposed **Road boundary overlay** to the [automatic projection comparison](detection_enhancment.md#1-automatic-workflow). Display the OpenStreetMap road footprint beneath the selected 12dAZ objects so the survey kerbs and road edges remain visible. Use a translucent fill and a distinct edge style, with a legend that identifies mapped and estimated boundaries.

#### 1.9.1 Build the reference boundaries

- Prefer mapped road-surface polygons, such as applicable `area:highway` features. Use their outer edges as mapped references. Preserve islands, medians and separate carriageways where the reference identifies them. [OpenStreetMap road-area tagging](https://wiki.openstreetmap.org/wiki/Key:area:highway)
- Where the reference only has a centreline, use a valid mapped carriageway width to offset the line by half its width on each side. Build the offsets in a projected metre-based coordinate system. If the width is estimated from lanes or road class, retain its provenance and display the result as an estimate. If neither a useful width nor a credible estimate is available, show the centreline alone. [OpenStreetMap width tagging](https://wiki.openstreetmap.org/wiki/Key:width)
- Join touching footprints at same-level junctions where the data supports the connection. Keep grade-separated roads distinct. Exclude edges created only by clipping at the survey or viewport limit. Treat raster road edges as visual cues, since their apparent width depends on map style and zoom.

#### 1.9.2 Place and compare the overlay

- Transform the reference edges into the current candidate's drawing coordinates. Draw the basemap first, the reference footprint second, and the original 12dAZ linework on top. Keep all layers registered through pan, zoom and Fit.
- Compare like-for-like survey edges across separated road sections, preferably including bends and junctions. Record the overlap, lateral distance and heading agreement. Use paired kerbs or supported carriageway geometry where the source identifies them. Skip comparisons against unrelated lot boundaries and utilities.
- Use corresponding road shapes to help eliminate clearly wrong projections under the rules in the automatic comparison proposal. Treat estimated widths as supporting evidence only. Missing road edges leave the centreline check available and carry no penalty. The overlay is display-only and leaves source and DXF geometry unchanged.

#### 1.9.3 Acceptance examples

- Test a mapped polygon with an island, an 8 m centreline with mapped width, and a centreline with unknown width. Verify that the mapped outline preserves the island, the estimated edges lie 4 m either side of the centreline on a straight section, and the unknown-width feature shows a centreline only.
- Test the same synthetic survey under the correct projection and a deliberately displaced candidate. Verify that mapped and estimated edges have separate styles, the reference remains below the survey geometry through pan and zoom, and an estimated-width difference alone cannot eliminate a candidate.
- Test a same-level junction, a grade-separated crossing, and a viewport clip. Verify that only the connected junction joins and that no clip edge is scored as a road boundary.

#### 1.9.4 Develop and accept the standalone engine

Follow the [standalone engine implementation stages](docs/road_boundary_engine_requirements.md#6-implementation-steps) and pass the relevant tests before advancing to the next stage.

- Keep the road-boundary engine in this repository as a standalone Rust component that can run and be tested independently. Its input is OpenStreetMap road geometry and tags. Its output is mapped outlines, estimated edges with width provenance, or centreline-only features. Use a standalone runner to inspect the output and collect benchmark results.
- Develop against synthetic cases, then measure detection against independently surveyed or annotated road edges on a held-out set of road sections. Include bends, junctions, divided roads and sections with missing widths. Keep road sections used for tuning out of the acceptance set.
- Measure matched road-edge length within a documented lateral tolerance, in metres, on corresponding road sides. Report precision as the fraction of predicted edge length that matches the reference, and recall as the fraction of reference edge length that the engine detects. Use the harmonic mean of precision and recall (F1) as the detection score. Missed edges reduce recall and spurious edges reduce precision.
- **Integration decision** The maintainer accepted the MGA54 and MGA56 visual reviews and corridor pilots for integration. The independent held-out score was waived. The 80% road-edge F1 target remains documented for future quantitative validation.
- Include road centrelines in the standalone output. Preserve mapped centrelines and synthesise a centreline inside a usable road surface where one is missing. Label the centreline source separately from the road-edge source.

#### 1.9.5 Match projections across road models

The automatic corridor comparison identifies road geometry across the archive's models and deduplicates overlapping copies. Score the length of aligned survey road edges and centrelines as a percentage of comparable survey road length across those models. A candidate matches when both geometry classes provide adequate evidence and the pooled score is strictly greater than **70%**. Report the score by model and geometry class. Follow the [multi-model scoring and elimination rules](detection_enhancment.md#21-score-the-road-models-in-the-archive). Select **Detect MGA** to screen and cycle all compatible grids without selecting a grid or model. Use confirmed source units or assume metres. The worker loads local OpenStreetMap extracts first and makes one bounded Overpass road-vector request for missing candidate areas. Only geographic bounding boxes are sent. A local reference beside the archive with the suffix `.osm-roads.geojson` also runs offline on open. Show progress by candidate and up to three distinct ranked grids. Group datum variants by grid and retain the full measured and unchecked results for review. Applying a grid requires confirmation.

See the [standalone road-boundary engine requirements](docs/road_boundary_engine_requirements.md) for the input and output contract, detection rules, test cases and benchmark method.
Use the [road-boundary engine test requirements](docs/road_boundary_engine_test_requirements.md) for the DXF evidence and qualification cases.

## 2. Preview of skipped elements

The skipped-elements preview lets users see which elements in the selected models the converter would skip with the current export settings, before writing a DXF.

### 2.1 Desktop workflow

- Open a small, non-modal **Skipped elements** window from **View → Show skipped elements**. Show the overlay while the window is open and allow users to keep working in the plan preview. Update the count when the selected models or geometry settings change, including **Flatten 2D**, **Keep Z**, tolerances and geometry assumptions.
- Use a dashed outline for skipped elements with reliable plan geometry. Use a marker when the source provides only a reliable XY location, and list elements with no reliable XY information. Give the overlay a labelled legend that remains legible over an active background map.
- Show the model, element type, source line and skip reason in the skipped-elements list. Selecting a drawable entry highlights it in the preview. The count includes list-only elements and elements whose outlines were omitted to fit the preview budget.
- Count elements the converter skips entirely. Keep warnings about partial exports, approximations and lost attributes in the conversion report. An element can be skipped in **Keep Z** when its elevation is unknown and still be exportable in **Flatten 2D**.

### 2.2 Rust integration

- Reuse the conversion service's geometry decisions with the active export settings. Refresh the existing background preview job when they change and use its revision checks to discard stale results.
- At the point where the converter skips a source element, retain its identity, reason and any reliable plan geometry or position needed by the desktop overlay. Reuse the existing model name, source ID and source line. Keep an accurate skip count even when detailed entries are bounded, and show when details are truncated. A failed job should surface its existing error state.
- Derive the display geometry from valid source coordinates. A known vertex can anchor a marker when an unsupported curve cannot be drawn reliably. Avoid joining vertices with invented straight segments. Keep overlay geometry in drawing coordinates so it follows the existing pan, zoom and map registration.
- Keep preview drawing within the shared 30,000-point allowance. Reserve display capacity for exportable geometry and expose skipped entries whose geometry could not be drawn within the allowance. The overlay is preview-only. Enabling it leaves the DXF, CLI and conversion report format unchanged.

### 2.3 Acceptance criteria

Use the [skipped-elements preview test requirements](docs/skipped_elements_test_requirements.md) for fixtures, test cases and test commands.

- Use a mixed-model fixture to verify that counts and reasons match an actual export of the selected models with identical settings. Cover an element skipped only in **Keep Z**, a drawable skipped element, a marker-only element, and an element that can only appear in the list.
- Switch models and export settings while preview work is in progress. Verify that the count, list and overlay match the latest selection and settings.
- Verify that opening and closing the window shows and hides the overlay without resizing the plan. Verify that the overlay follows pan and zoom, remains identifiable over a background map, and respects the preview budget without changing the skip count.
- Export with the overlay on and off. Verify equivalent reopened DXF geometry and conversion report results.
