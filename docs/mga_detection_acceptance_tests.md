# Automatic MGA detection acceptance tests

These cases cover the proposed [Background Map detection workflow](../design.md#15-automatic-mga-detection-and-assignment). A case passes only when the displayed result, active map registration and observable side effects match the expected result. Run automated cases offline with instrumented providers.

## 1. Fixtures and reference values

| Fixture | Source data |
| --- | --- |
| MGA55 metres | Two points in GDA2020 / MGA zone 55 (EPSG:7855), with XY `(350174.343, 5812977.867)` and `(350294.343, 5813057.867)`. The first point matches approximately `37.81831708° S, 145.29775943° E`. Create versions with and without trusted CRS and unit declarations. |
| MGA55 millimetres | The same two XY points multiplied by 1,000. Use variants with explicit millimetres and with units absent. |
| Ambiguous XY | Two points at `(500000, 7000000)` and `(500120, 7000080)` in confirmed metres with no independent CRS evidence. Reuse the same coordinates in a fixture whose source is known to use a local grid. |
| Mixed models | A large model with the MGA55 metre coordinates and a small model with local XY `(100, 200)` and `(120, 240)`. Repeat with model order reversed. |
| Zone boundary | Two geographic points at latitude 37° S on opposite sides of longitude 150° E. Project both into zone 55 using an independent reference implementation. Place the eastern point inside the agreed boundary tolerance, then create a second fixture outside it. Record both offsets and the acceptance tolerance. |

Use deterministic synthetic fixture geometry for coordinate screening. Verify declared CRS and unit fields against representative 12d exports before treating them as trusted. Use approved exports or a faithful synthetic copy of their documented fields for parser acceptance. Write the same payload as a plain `.12da` and inside a valid `.12daz`. Record the exact accepted metadata syntax and reference projection results in the test evidence.

Phase 1 reference check used PROJ 9.5.1 to verify the MGA55 control, both Brisbane false origins and the Brisbane checkpoints. The zone 55 boundary fixtures use longitudes 149.999°, 150.012° and 150.020° E at 37° S. The offline screen allows an additional 0.005° beyond the published area of use. The PROJ registry has no area-of-use records for legacy GDA94 zones 46 and 47, so they remain manual choices.

The V15 archive specification and repository fixtures did not establish a trustworthy CRS or source-unit declaration emitted in a 12d archive. Exercise the verified-declaration cases with typed evidence at the evaluator boundary. In the desktop, enter the known projection and units explicitly. Recorded or approved exports with a verified declaration are required before enabling automatic assignment from archive text. [12d Model V15 archive specification](https://downloads.12dmodel.com/v15/12d_Model_15_C1p/12d_A_File_Format.pdf)

## 2. Automatic assignment and units

| ID | Steps | Pass result |
| --- | --- | --- |
| MGA-DETECT-01 | Open the declared MGA55 metre fixture as `.12da` and `.12daz`. Open **View → Background Map** after the scan. | Both formats show **Automatically assigned** with GDA2020, zone 55, metres and the actual declaration source. The session registration puts the first point within 0.05 m of the independently computed reference. |
| MGA-DETECT-02 | Open the fixture with an explicit CRS but no source units. Set DXF drawing units to metres. Then confirm the map source units as metres and choose **Detect MGA again**. | The DXF unit preference leaves the map units unconfirmed. Detection assigns the declared CRS only after map source units are confirmed and the range check passes. |
| MGA-DETECT-03 | Open the declared millimetre fixture. Repeat with the unit declaration removed, then confirm millimetres and rerun. | Explicit millimetres give the same geographic positions as the metre fixture. The file with missing units stays unassigned until confirmation. Source XY values remain in millimetres. |
| MGA-DETECT-04 | Repeat with a synthetic survey independently projected into GDA94 / MGA zone 56 (EPSG:28356), with supported CRS and unit declarations. | Detection assigns GDA94 and zone 56. Its geographic positions match the reference using the same datum transformation policy as manual registration within the recorded numerical tolerance. |

## 3. Ambiguity and conflicting evidence

| ID | Steps | Pass result |
| --- | --- | --- |
| MGA-DETECT-05 | Open the ambiguous XY fixture. Confirm metres. Add a known project location lying near the zone 55 interpretation and rerun. | The first result lists multiple compatible zones and leaves the datum unresolved. The known location narrows the candidates without confirming a datum or assigning a session registration. |
| MGA-DETECT-06 | Open the local-grid version of the ambiguous XY fixture. Set **Local grid** and apply a valid control-point alignment. Rerun detection. | The coordinate range alone never causes automatic assignment. The active control-point alignment survives the rerun. |
| MGA-DETECT-07 | Swap X and Y in the MGA55 metre fixture and leave the declaration in place. | Detection reports a range conflict and an axis-order hint where reversing the inputs would fit. It leaves the registration unassigned and preserves the input coordinate order. |
| MGA-DETECT-08 | Open both versions of the mixed-model fixture and select only the large model in the preview. | Both model orders produce the same conflict. The result identifies the conflicting model even when it is hidden or deselected. |
| MGA-DETECT-09 | Open each of the zone-boundary fixtures with an explicit zone 55 declaration and metre units. | The fixture inside the documented tolerance retains its declared zone. The fixture outside the tolerance flags the range conflict and leaves the registration unassigned. Detection never switches it to the adjacent zone on its own. |
| MGA-DETECT-10 | Supply conflicting supported CRS declarations, then try an unsupported CRS. In a separate XY-only file, put `MGA55 GDA2020` in the filename and model name. | Conflicts or unsupported declarations remain unassigned and show an explanation. Free text and `archive_version` cannot produce an automatic assignment. |
| MGA-DETECT-11 | Open a one-point version of the metre fixture with and without its trusted declaration. Repeat with no finite recognised XY. | The declared point can be assigned after validation. The unreferenced point and the file without usable XY are inconclusive. No detection failure prevents the archive from opening when the normal parser accepts it. |

## 4. Registration ownership and background jobs

| ID | Steps | Pass result |
| --- | --- | --- |
| MGA-DETECT-12 | Reopen an archive with a valid fingerprint-bound sidecar that differs from the automatic suggestion. Repeat with a stale fingerprint. | The valid sidecar wins and stays intact. A stale sidecar is reported and ignored. Any resulting automatic assignment follows the same evidence requirements as a file without a sidecar. |
| MGA-DETECT-13 | Delay a detection result for file A, open file B, then deliver A's result. Separately, apply a manual CRS while detection is pending. | File B shows only its own evidence and registration. A late result leaves a manual assignment in effect. |
| MGA-DETECT-14 | Open the declared metre fixture. Change selected models, toggle Flatten 2D and Keep Z, and exercise preview simplification on a dense copy of the fixture. | The same archive receives the same detection result. Geometry used only for display or export selection cannot alter the assignment. |

## 5. Network, persistence and export

| ID | Steps | Pass result |
| --- | --- | --- |
| MGA-DETECT-15 | Open the declared, ambiguous and conflicting fixtures with all providers instrumented. Repeat after using a provider on a different archive. Open the map settings but leave the provider Off. | Automatic assignment, candidate screening and failed detection all make zero tile, viewport or geocoding requests. An explicitly enabled provider can request tiles only after a usable registration exists for the current survey. |
| MGA-DETECT-16 | Open the declared fixture and let detection assign it. Inspect the source and adjacent files before and after selecting **Save alignment**. Reopen the survey. | Automatic assignment creates no sidecar. Explicit saving writes a fingerprint-bound sidecar with the applied CRS and source units. Reopening restores it. The source hash stays the same. |
| MGA-DETECT-17 | For both export modes, export the same selected geometry with detection enabled and with detection bypassed in the test harness. Also compare the matching CLI export. | Reopened DXFs have identical canonical geometry, coordinates, elevations, layers and unit declarations. Conversion diagnostics and substantive report fields agree. Detection adds no map-only fields to the conversion report. |

## 6. OpenStreetMap comparison

Use mock tiles and local synthetic reference vectors. Include a candidate with matching road bends and junctions, a candidate with a different road pattern, and cases with tied road shapes, sparse geometry or incomplete coverage. Provide comparable and misleading boundary outlines as separate features.

| ID | Steps | Pass result |
| --- | --- | --- |
| MGA-DETECT-18 | Select a candidate with confirmed units. Open the comparison notice, decline, then confirm on a second attempt. Pan the viewport and switch candidates manually. | Declining sends zero requests. Confirmation shows the candidate and visible OpenStreetMap attribution. Requests cover user-visible viewports only. Other candidates generate zero tile requests until viewed. |
| MGA-DETECT-19 | Start with the map Off, then repeat with another provider and a valid saved alignment. End the comparison manually, advance a fake clock past 30 seconds, and repeat while switching archives with requests in flight. | Ending or timing out on the same archive restores the prior provider, view, opacity and alignment. Switching archives invalidates the old comparison. Late tile responses are discarded and comparison writes no sidecar. |
| MGA-DETECT-20 | Supply matching local reference road shapes for one zone and a clearly different road pattern for the other candidates. Run with both GDA94 and GDA2020 possibilities present. | The matching zone is recommended with its evidence. The datum remains unresolved without independent evidence, and the recommendation does not apply a registration. |
| MGA-DETECT-21 | Repeat with equivalent road shapes, sparse features, incomplete coverage and a survey with no comparable road geometry. Repeat with no extract. | The automatic result is inconclusive with a reason. The existing candidate list is retained, and the visual preview remains available without vectors. |
| MGA-DETECT-22 | Simulate tile errors and an offline connection. Compare before and after enabling a valid local reference extract. | The UI reports tile failure without selecting another provider or penalising a candidate. The local comparison produces the same result offline and sends no requests. |
| MGA-DETECT-23 | Compare a strong road match with an independently mapped boundary match. Remove the boundary, then add a near-identical boundary to the wrong candidate. Repeat with only a boundary match and no road evidence. | A comparable boundary adds supporting evidence. Removing it causes no penalty. An unrelated outline cannot override a poor road match or generate a recommendation by itself. The UI labels boundary evidence as approximate. |

## 7. Brisbane City Survey Grid

Use two synthetic checkpoints for each Brisbane grid. The listed longitudes and latitudes are in the native datum of the row. Reference XY values were generated with PROJ 9.5.1 through pyproj using the EPSG definitions, with longitude and latitude passed in that order.

| EPSG code | Native longitude and latitude | Easting and northing (m) |
| --- | --- | --- |
| 3113 | 153.028100° E, 27.469800° S | 52,777.351 E, 158,753.314 N |
| 3113 | 152.967000° E, 27.586000° S | 46,741.770 E, 145,876.982 N |
| 20047 | 153.028100° E, 27.469800° S | 152,777.351 E, 258,753.314 N |
| 20047 | 152.967000° E, 27.586000° S | 146,741.770 E, 245,876.982 N |

| ID | Steps | Pass result |
| --- | --- | --- |
| MGA-DETECT-24 | Select each Brisbane CRS in **View → Background Map**. Test forward and inverse projection at both independent checkpoints and at the 153° E, 28° S origin in its native datum. | Both entries are shown with their correct names and EPSG codes. Absolute results agree within 0.05 m. The origin gives `(50000, 100000)` for EPSG:3113 and `(150000, 200000)` for EPSG:20047. |
| MGA-DETECT-25 | Apply each grid to its own fixture and verify the geographic position used by the basemap against an independent reference. For BCSG02, use EPSG operation 8048 to obtain the conformal GDA2020 positions. | BCSG02 applies the datum transformation after inverse projection. Its first checkpoint reaches `153.02810603° E, 27.46978745° S` in GDA2020 within 0.05 m. BCSG2020 uses its native GDA2020 position. |
| MGA-DETECT-26 | Open each Brisbane fixture with trusted matching CRS and units. Repeat with declaration removed and with the wrong Brisbane EPSG code on the same coordinates. | A matching declaration can assign the exact grid. The XY-only case requires confirmation. The wrong-definition fixture fails the area-of-use screen and does not automatically assign a replacement. |
| MGA-DETECT-27 | Repeat manual assignment with a millimetre copy of each fixture, then select **Save alignment** and reopen. Compare against the metre result and export both copies. | Equivalent units place the same points on the map. The sidecar restores the correct EPSG code and source units. DXF coordinates and unit declarations follow the input values and export settings. |
| MGA-DETECT-28 | Choose a Brisbane CRS as the target while aligning a local-grid fixture with a known control point. Open an MGA56 file after the Brisbane file, then repeat in reverse order. | The local control maps to the expected Brisbane coordinate. MGA56 and the two Brisbane grids remain distinct candidates, and switching archives leaves each registration tied to its own file. |

## 8. Candidate filters and visible road-shape checks

Make synthetic raster fixtures with a bent road and a junction using representative street-map road fills and contrasting edges. Use a street-level zoom, separated candidate views with the same survey footprint, and a wrong-grid version where the survey is displaced beyond the configured match radius. Include a broad uniform light area, labels, unrelated boundaries and a missing tile. Use mock providers and a fake clock.

| ID | Steps | Pass result |
| --- | --- | --- |
| MGA-DETECT-29 | Run detection on the maintainer-confirmed MGA54 road design archive. Confirm metres and choose **Known MGA zone → 54**. Clear the zone filter. | The 18 metre candidates narrow to the two Zone 54 datum variants. Clearing the zone filter restores the original metre list. Filtering sends no requests and leaves registration for explicit confirmation. |
| MGA-DETECT-30 | Start with MGA, Brisbane and more than one candidate unit. Select a zone and confirm a unit that excludes the current candidate, then choose an incompatible combination. Run the local vector comparison with a compatible filter. | The selector clears an excluded candidate and shows an explicit empty state for an incompatible filter. The vector check receives exactly the filtered candidates. Clearing the zone restores eligible Brisbane options. |
| MGA-DETECT-31 | Select the synthetic road model. Run **Check visible road match** on the aligned image, then on the wrong-grid image with the same road section and zoom. | The aligned case matches probes on separated road segments and bends. The displaced case scores lower with the same evaluable probes. The result shows the aligned and evaluable counts. |
| MGA-DETECT-32 | Run the raster check with light background patches, labels, an unrelated boundary, a single straight line and zoom below street detail. | The check reports **Inconclusive** when there is insufficient distinctive road evidence. Uniform fills and isolated marks do not count as matches. An apparent boundary by itself does not produce a road match. |
| MGA-DETECT-33 | Start the visual comparison, record normal viewport requests and click the raster check twice. Repeat with a missing or failed tile and include a survey segment outside the painted viewport. | Recognition adds zero tile requests. It ignores offscreen geometry and excludes unavailable pixels from scored probes. Too little loaded detail produces **Inconclusive** rather than a poor candidate score. |
| MGA-DETECT-34 | Check two candidates on the same visible road section and scale. Repeat with tied scores and with different zoom or coverage. Leave a third candidate unchecked. | Only comparable completed scores can support a relative recommendation. Ties or incomparable views remain inconclusive. The third candidate stays visible without a score. The result leaves the datum and registration for confirmation. |
| MGA-DETECT-35 | Delay the image result while changing candidates, viewport, units and archives. End the comparison manually and repeat with the 30-second timeout. Repeat the DXF parity case after a completed check. | Late results cannot appear for the new context. The previous map state is restored when appropriate. The check writes no alignment, creates no persistent image data and leaves DXF and conversion report results unchanged. |

## 9. Acceptance evidence

Capture the fixture identifier, CRS and units, actual outcome, active registration, provider request count and any sidecar changes. Record independently calculated coordinates for the absolute-position checks. Run candidate evaluation and race cases in Rust with deterministic fixtures and controlled worker completion. Record a packaged Windows check of the three Background Map outcomes and the manual **Apply** and **Save alignment** actions before release.

The local implementation passed the Rust desktop and core suites and the Windows portable package smoke test. OpenStreetMap comparison used controlled map state and synthetic reference vectors. Offline scans of maintainer-confirmed MGA54 and MGA56 surveys included the expected zones among 18 and 16 metre-based candidates, respectively. Each scan offered both GDA94 and GDA2020 for the expected zone and left the choice for confirmation. Neither archive header contained a verified datum declaration.

The offline GHD sample check for MGA-DETECT-29 narrowed 18 metre-based candidates to both Zone 54 datum variants. The synthetic raster fixture matched 6 of 6 aligned probes and 0 of 6 displaced probes. Mock tile checks verified that reading a mask starts no requests and ignores unpainted and invalidated tiles.

## 10. Automatic grid sweep

| Case | Exercise | Expected result |
| --- | --- | --- |
| MGA-DETECT-36 | Open an archive with no zone, model or unit selected and click Detect MGA. | The app uses metres, checks compatible candidate projections and shows progress. Applying a result is a separate action. |
| MGA-DETECT-37 | Supply local coverage for one grid and mock road vectors for another. Include an empty area or a provider failure. | Local coverage is used first. One bounded request contains only uncovered candidate area bounds. Empty or unavailable coverage remains unchecked. |
| MGA-DETECT-38 | Compare at least three measured zones with both datum variants in one zone, plus an unchecked and a zero-score candidate. | Up to three distinct positive-score grids appear in score order. Expanding a grid shows its measured datum variants. Full results retain the other candidates. |
| MGA-DETECT-39 | Cancel a sweep or change the archive or units during comparison. | Late results are discarded and the active projection stays in place. |
