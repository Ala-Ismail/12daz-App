# Skipped-elements preview test requirements

These cases verify the [skipped-elements preview design](../design.md#2-preview-of-skipped-elements). Use synthetic surveys with predictable coordinates and record the options used for each comparison.

## 1. Test fixtures

- **Mixed models.** Use a valid 3D line, a 2D line with unknown elevation, an attributed point string that exports with a warning, and an unsupported type in the same model. Use a separate model containing only an unsupported type.
- **Drawing coverage.** Use the 2D line for an outline in Keep Z, a rejected curve with a valid source vertex for a location marker, and an unsupported type with no known XY for a list-only entry.
- **Settings and load.** Use a varying-Z arc with the linear grade assumption off and on. Generate a dense survey and enough skipped elements to exceed the preview point limit and the skipped-details limit.

## 2. Conversion decisions

| ID | Exercise | Pass result |
| --- | --- | --- |
| SKIP-01 | Inspect and export the mixed fixture with the same selected models in Flatten 2D and Keep Z. Reopen both DXFs. | The skip counts, source identities and reasons agree. The unknown elevation causes a skip only in Keep Z. A partially exported or approximated element is excluded from the skip count. |
| SKIP-02 | Toggle the linear grade assumption for a varying-Z arc in Keep Z. Change valid tolerances and try an invalid tolerance. | The arc changes from skipped to approximated when the assumption is enabled. Analysis uses the current valid export settings. Invalid settings leave the skipped result unavailable and show the validation error. |
| SKIP-03 | Select only the unsupported model, select multiple models, then clear the selection. | The count and reasons follow the selected models. An unsupported-only model can be inspected. No stale details appear after clearing the selection. |

## 3. Display, limits and regression

| ID | Exercise | Pass result |
| --- | --- | --- |
| SKIP-04 | Open the display fixture, then show skipped elements and select entries from the list. Pan, zoom and Fit over a mocked basemap. | The overlay starts off. Outline and marker locations match the source XY. Unknown positions appear only in the list. The selected drawable entry highlights. The overlay and legend stay legible and registered to the preview. |
| SKIP-05 | Rapidly change settings and model selection during preview work, switch archive, and cancel a refresh. | Only the most recent successful analysis is shown for the current inputs. Pending, failed or cancelled work leaves the skipped result unavailable. |
| SKIP-06 | Exceed both the preview allowance and the skipped-details limit. | The shared display uses at most 30,000 points. Exportable geometry keeps priority, the total skipped count stays accurate, and the UI identifies omissions and truncation. |
| SKIP-07 | Export with the overlay off and on. Reopen the DXFs and compare canonical geometry and report fields. | Geometry, coordinates, layers, skip counts, diagnostics and report schema agree. The DXF and report contain no preview-only overlay data. |

## 4. Test commands

```powershell
cargo test --manifest-path rust/Cargo.toml --locked --test skipped_elements
cargo test --manifest-path rust/Cargo.toml --locked --features desktop
```

Record manual display evidence for SKIP-04 using the packaged Windows desktop. Use the checked-in synthetic fixtures or generated data for automated testing.
