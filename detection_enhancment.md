# Narrowing projection suggestions with road matching

Brainstorming proposal. Assume the survey XY is in metres, automatically compare candidate projections with OpenStreetMap road shapes, and eliminate clear mismatches. The aim is to produce a shorter list even when the user does not know the MGA zone.

## 1. Automatic workflow

1. When a survey opens, run the check in the background using metres. Build the candidate list from the supported MGA and Brisbane grids that fit the coordinate ranges. Find likely road centreline and boundary shapes in the archive automatically.
2. Use available OpenStreetMap road reference data to compare candidates without displaying and clicking through each grid. Record the result for each candidate. If the survey has no reliable road geometry, finish with **Inconclusive**.
3. Remove clear mismatches from the shortlist and show how many were eliminated. Keep their reasons available in the comparison details. Automatically display an OpenStreetMap overlay for the leading plausible candidate when map data and meaningful evidence are available. Label the overlay with its assumed grid.
4. Show the shortlist and any unresolved datum choices. If the user later elects to apply or save an alignment, use the existing controls.

The comparison runs from archive open through to the shortlist and street-map view without requiring the user to select a grid, identify a road model, toggle the basemap, or start individual comparisons.

## 2. Find and compare road shapes

- Identify road centrelines and possible road edges from recognised source geometry across the archive, using model metadata as a hint and geometry as the check. Treat roughly parallel, continuous lines through bends and junctions as possible kerbs or corridor edges when the geometry supports that interpretation. Keep isolated boundaries, utilities and property lines out of the road match.
- Prefer OpenStreetMap vector centreline geometry for automatic comparison. Match visible road edges when both sides represent the same kind of feature, such as mapped kerbs or a road corridor. When the reference contains only a centreline, compare the survey centreline or the midpoint between a credible pair of survey edges.
- For an on-screen image check, look for narrow road-colour bands or paired edges that follow the survey's curves and junctions. The rendered width represents the map style and zoom. Use it as evidence of road shape, while judging survey width or legal boundaries from separate reference data.
- Project the survey as-is under each candidate and compare distance, heading, bend direction and junction geometry in metres. Require agreement across several separated road features. Penalise systematic offsets and poorly aligned shapes. Leave translation, rotation and scale fitting to the separate local-grid workflow.
- Use comparable road-surface boundaries and retain their provenance. Street-map display cues such as park edges can support review. Only road boundaries and centrelines enter the road match score below.

### 2.1 Score the road models in the archive

- Inspect every model automatically. Separate comparable road centrelines and road-surface edges from utilities, property boundaries and other unrelated geometry. Combine compatible road features split across models. Count overlapping copies of the same feature once and retain the contributing model names. Compare road edges to corresponding sides and centrelines to centrelines.
- For each candidate, record the matched and evaluable survey length in metres for each road model and each geometry class. Define evaluable length using the survey geometry, the comparison tolerance and verified reference coverage. Where reference coverage is reliable, surveyed road with no aligned counterpart counts as unmatched. Report any exclusions. Compare candidates using the same survey sections when ranking them head to head.
- Score the candidate as total matched comparable road length divided by total evaluable comparable road length, pooling edges and centrelines across road models. Require meaningful coverage and separated features across both classes. A **Match** requires **strictly more than 70%** overall. Exactly 70% falls below the threshold. Show the score and model breakdown so a match from part of the archive is visible.
- If either geometry class lacks enough comparable evidence, report **Inconclusive** with the available results. Keep a candidate below the threshold on the shortlist if evidence is insufficient to justify elimination. If more than one candidate passes, retain each and surface the ambiguity. Preserve road edge and centreline provenance, including when one was derived from the other.
- This threshold applies to candidate projection matching. The maintainer accepted the two corridor pilots for integration. The independent **80% held-out road-edge F1** target remains available for future validation.

## 3. Eliminate clear mismatches

| Outcome | Meaning |
| --- | --- |
| **Eliminated** | Reference coverage is sufficient, comparable roads are present, and several separated survey features strongly disagree in location or shape. Remove the candidate from the shortlist and retain the reason. |
| **Match** | Both road geometry classes have adequate evidence and the pooled match exceeds 70%. Multiple candidates can meet this rule. |
| **Retained** | The evidence leaves the candidate plausible, including cases below the threshold that do not justify elimination. |
| **Unchecked or inconclusive** | Reference coverage is missing or stale, the request failed, or the survey and reference lack comparable features. Keep the candidate available with that status. |

- Use road bends, junctions and multiple headings to justify an elimination. A straight road, regular street grid, missing boundary, or weak score alone is insufficient. Record the coverage, usable feature count and match count for every checked candidate.
- Show a shorter list even if there is no clear winner. If every checked candidate is eliminated, report **No credible match** and keep the original results available so the user can review the metres assumption or the possibility of a local grid.
- Street-map fit can distinguish candidate zones. Confirming GDA94 versus GDA2020 still requires survey metadata or control evidence.
- Keep recommendations separate from the active alignment. The comparison leaves source coordinates and DXF export unchanged.

## 4. Use OpenStreetMap in two ways

- **Automatic comparison** uses an available local OpenStreetMap vector dataset with documented coverage, date and attribution, or a configured service that permits automated spatial comparison. Coverage of plausible candidate locations is the dependency for eliminating mismatches without asking the user for a zone. [OpenStreetMap data and licensing](https://www.openstreetmap.org/copyright)
- **Automatic street-map display** shows the leading view once it is visible in the app. The public raster service serves that displayed viewport. Image recognition can reuse its loaded pixels as supplementary evidence. Offscreen candidate checks run against the automated reference source. If that source is unavailable, report which candidates remain unscored. [OpenStreetMap tile usage policy](https://operations.osmfoundation.org/policies/tiles/)

## 5. Evidence that the workflow helps

- Withhold the confirmed zone from known MGA54 and MGA56 tests. Open each archive and provide no further input. With adequate reference coverage, verify that the street map appears automatically, clearly mismatching grids are eliminated, and the expected zone remains on the shortlist.
- Verify that offsets, a similar-looking road in the wrong zone, unrelated property boundaries, and incomplete reference coverage do not produce a confident recommendation.
- Verify that missing data, stale maps and ties leave candidates available. Verify that every elimination has a reason and that completing the check leaves the current alignment intact.

The [existing background map design](design.md#18-narrowing-projection-candidates-and-checking-road-shapes) covers the current candidate filters and visible road-shape check.
