use std::path::{Path, PathBuf};

use az12_converter::geometry::Point;
use az12_converter::map::{
    Coordinate, CrsId, LonLat, MapMode, MapRegistration, Projection, SourceUnit, TileId, sidecar,
    tile_coordinate,
};
use eframe::egui;

use super::map_tiles::{TileRange, Tiles};

#[derive(Default)]
pub(super) struct Backdrop {
    source: Option<(PathBuf, String)>,
    registration: Option<MapRegistration>,
    projection: Option<Projection>,
    open: bool,
    pick: Option<usize>,
    draft: Draft,
    message: String,
    tiles: Tiles,
    opacity: f32,
    local_registration: bool,
    key_input: String,
    google_logo: Option<egui::TextureHandle>,
    #[cfg(windows)]
    remember_key: bool,
    pub(super) map_status: Option<String>,
}

#[derive(Default)]
struct Draft {
    crs: Option<CrsId>,
    unit: Option<SourceUnit>,
    local: bool,
    geographic_controls: bool,
    second: bool,
    controls: [Control; 2],
}

#[derive(Default)]
struct Control {
    drawing: [String; 2],
    map: [String; 2],
}

impl Backdrop {
    #[cfg(debug_assertions)]
    pub(super) fn start_capture_provider(
        &mut self,
        mode: MapMode,
        opacity: Option<f32>,
    ) -> Result<(), String> {
        if self.registration.is_none() {
            return Err("Map capture needs a valid saved registration.".into());
        }
        self.select_provider(mode, saved_key);
        if let Some(opacity) = opacity {
            self.opacity = opacity;
        }
        if matches!(mode, MapMode::Satellite | MapMode::EsriSatellite) && !self.tiles.has_key(mode)
        {
            return Err(if self.message.is_empty() {
                "No saved key is available for this provider.".into()
            } else {
                self.message.clone()
            });
        }
        Ok(())
    }

    #[cfg(debug_assertions)]
    pub(super) fn capture_complete(&self) -> bool {
        self.tiles.capture_complete()
    }

    pub(super) fn sync_source(&mut self, source: Option<(&Path, &str)>) {
        if self
            .source
            .as_ref()
            .map(|(path, hash)| (path.as_path(), hash.as_str()))
            == source
        {
            return;
        }
        let mut tiles = std::mem::take(&mut self.tiles);
        tiles.select(MapMode::Off);
        *self = Self {
            source: source.map(|(path, hash)| (path.to_owned(), hash.to_owned())),
            tiles,
            opacity: 0.75,
            ..Self::default()
        };
        if let Some((path, hash)) = &self.source {
            match sidecar::load(path, hash) {
                Ok(Some((registration, local))) => {
                    self.draft.crs = Some(registration.crs());
                    self.draft.unit = Some(registration.units());
                    self.draft.local = local;
                    self.activate(registration, local);
                }
                Ok(None) => {}
                Err(error) => self.message = format!("Could not load map alignment. {error}"),
            }
        }
    }

    pub(super) fn menu_item(&mut self, ui: &mut egui::Ui) {
        if ui
            .add_enabled(self.source.is_some(), egui::Button::new("Background Map"))
            .on_disabled_hover_text("Open a survey archive to configure its background map")
            .clicked()
        {
            ui.close();
            self.open = true;
            self.pick = None;
        }
    }

    pub(super) fn picking(&self) -> bool {
        self.pick.is_some()
    }

    pub(super) fn pick_point(&mut self, point: Point) {
        if let Some(index) = self.pick.take() {
            self.draft.controls[index].drawing = point.map(|value| format!("{value:.6}"));
            self.open = true;
            self.message.clear();
        }
    }

    pub(super) fn dialog(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Background Map")
            .open(&mut open)
            .default_width(410.0)
            .vscroll(true)
            .show(ctx, |ui| self.setup(ui));
        self.open = open && self.pick.is_none();
    }

    fn setup(&mut self, ui: &mut egui::Ui) {
        ui.label("The map is a visual reference for the plan preview.");
        let mut mode = self.tiles.mode();
        egui::ComboBox::from_label("Background")
            .selected_text(mode_label(mode))
            .show_ui(ui, |ui| {
                for value in [
                    MapMode::Off,
                    MapMode::Street,
                    MapMode::Satellite,
                    MapMode::EsriSatellite,
                ] {
                    ui.selectable_value(&mut mode, value, mode_label(value));
                }
            });
        if mode != self.tiles.mode() {
            self.select_provider(mode, saved_key);
        }
        if mode != MapMode::Off {
            ui.small("Online maps send the visible location to the selected provider.");
            ui.add(egui::Slider::new(&mut self.opacity, 0.1..=1.0).text("Map opacity"));
        }
        if matches!(mode, MapMode::Satellite | MapMode::EsriSatellite) {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.key_input)
                        .password(true)
                        .char_limit(512)
                        .hint_text("Provider API key"),
                );
                if ui.button("Set key").clicked() {
                    self.set_key(mode);
                }
            });
            #[cfg(windows)]
            {
                ui.checkbox(&mut self.remember_key, "Remember this key on this PC");
                ui.horizontal(|ui| {
                    if ui.button("Use saved key").clicked() {
                        match saved_key(mode) {
                            Ok(Some(key)) => self.apply_saved_key(mode, key),
                            Ok(None) => self.message = "No saved key is available in Windows Credential Manager.".into(),
                            Err(error) => self.message = error.into(),
                        }
                    }
                    if ui.button("Remove saved key").clicked() {
                        self.message = match credential(mode)
                            .and_then(|entry| match entry.delete_credential() {
                                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                                Err(_) => Err("Windows Credential Manager could not remove the key."),
                            })
                        {
                            Ok(()) => "Saved key removed. The current session key can be cleared with Set key.".into(),
                            Err(error) => error.into(),
                        };
                    }
                });
            }
        }
        ui.separator();
        egui::ComboBox::from_label("MGA datum and zone")
            .selected_text(
                self.draft
                    .crs
                    .map_or_else(|| "Choose CRS…".into(), crs_label),
            )
            .show_ui(ui, |ui| {
                for datum in ["GDA2020", "GDA94"] {
                    ui.label(egui::RichText::new(datum).strong());
                    for crs in CrsId::MGA {
                        if let Some((candidate, zone)) = crs.mga_details()
                            && candidate == datum
                        {
                            ui.selectable_value(
                                &mut self.draft.crs,
                                Some(crs),
                                format!("MGA zone {zone} ({})", crs.epsg()),
                            );
                        }
                    }
                }
            });
        egui::ComboBox::from_label("Source coordinate units")
            .selected_text(self.draft.unit.map_or("Choose units…", SourceUnit::label))
            .show_ui(ui, |ui| {
                for unit in SourceUnit::ALL {
                    ui.selectable_value(&mut self.draft.unit, Some(unit), unit.label());
                }
            });
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.draft.local, false, "Already on MGA");
            ui.radio_value(&mut self.draft.local, true, "Local grid");
        });
        if self.draft.local {
            ui.separator();
            ui.checkbox(
                &mut self.draft.geographic_controls,
                "Enter map points as WGS 84 latitude/longitude",
            );
            self.control(ui, 0);
            ui.checkbox(
                &mut self.draft.second,
                "Use a second point for scale and rotation",
            );
            if self.draft.second {
                self.control(ui, 1);
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                match self.draft.registration() {
                    Ok(registration) => self.activate(registration, self.draft.local),
                    Err(message) => self.message = message,
                }
            }
            if ui
                .add_enabled(
                    self.registration.is_some(),
                    egui::Button::new("Save alignment"),
                )
                .clicked()
            {
                self.save_alignment();
            }
            if ui
                .add_enabled(self.registration.is_some(), egui::Button::new("Clear"))
                .clicked()
            {
                self.registration = None;
                self.projection = None;
                self.tiles.select(MapMode::Off);
                self.message = "Map alignment cleared for this session.".into();
            }
        });
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
    }

    fn select_provider(
        &mut self,
        mode: MapMode,
        load: impl FnOnce(MapMode) -> Result<Option<String>, &'static str>,
    ) {
        self.tiles.select(mode);
        self.key_input.clear();
        self.message.clear();
        self.map_status = None;
        #[cfg(windows)]
        {
            self.remember_key = false;
        }
        if matches!(mode, MapMode::Satellite | MapMode::EsriSatellite) && !self.tiles.has_key(mode)
        {
            match load(mode) {
                Ok(Some(key)) => self.apply_saved_key(mode, key),
                Ok(None) => {}
                Err(error) => self.message = error.into(),
            }
        }
    }

    fn apply_saved_key(&mut self, mode: MapMode, key: String) {
        self.message = match self.tiles.configure_key(mode, key) {
            Ok(()) => "Saved provider key in use.".into(),
            Err(error) => error,
        };
    }

    fn set_key(&mut self, mode: MapMode) {
        let key = std::mem::take(&mut self.key_input);
        let empty = key.trim().is_empty();
        if let Err(error) = self.tiles.configure_key(mode, key.clone()) {
            self.message = error;
            return;
        }
        self.message = if empty {
            "Provider key cleared for this session."
        } else {
            "Provider key set for this session."
        }
        .into();
        #[cfg(windows)]
        if self.remember_key && !empty {
            self.message = match credential(mode).and_then(|entry| {
                entry.set_password(key.trim()).map_err(
                    |_| "The key is in use, but Windows Credential Manager could not save it.",
                )
            }) {
                Ok(()) => "Provider key saved in Windows Credential Manager.".into(),
                Err(error) => error.into(),
            };
        }
    }

    fn control(&mut self, ui: &mut egui::Ui, index: usize) {
        ui.group(|ui| {
            ui.label(format!("Control point {}", index + 1));
            let point = &mut self.draft.controls[index];
            ui.horizontal(|ui| {
                ui.label("Drawing");
                coordinate_edits(ui, &mut point.drawing, ["X", "Y"]);
                if ui.button("Pick").clicked() {
                    self.pick = Some(index);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Map");
                let labels = if self.draft.geographic_controls {
                    ["Latitude", "Longitude"]
                } else {
                    ["Easting", "Northing"]
                };
                coordinate_edits(ui, &mut point.map, labels);
            });
        });
    }

    fn activate(&mut self, registration: MapRegistration, local: bool) {
        match Projection::new(registration.crs()) {
            Ok(projection) => {
                self.registration = Some(registration);
                self.projection = Some(projection);
                self.local_registration = local;
                self.tiles.invalidate();
                self.message = format!(
                    "{} · {} · scale {:.8} · rotation {:.4}°",
                    registration.crs().epsg(),
                    registration.units().label(),
                    registration.scale(),
                    registration.rotation().to_degrees(),
                );
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    fn save_alignment(&mut self) {
        if let (Some((path, hash)), Some(registration)) = (&self.source, self.registration) {
            self.message = match sidecar::save(path, hash, registration, self.local_registration) {
                Ok(()) => "Map alignment saved beside the archive.".into(),
                Err(error) => format!("Could not save map alignment. {error}"),
            };
        }
    }

    pub(super) fn paint(
        &mut self,
        painter: &egui::Painter,
        rect: egui::Rect,
        to_drawing: impl Fn(egui::Pos2) -> Point,
        to_screen: impl Fn(Point) -> egui::Pos2,
    ) -> bool {
        self.map_status = None;
        if self.tiles.mode() == MapMode::Off {
            return false;
        }
        let (Some(registration), Some(projection)) = (self.registration, self.projection.as_ref())
        else {
            self.map_status =
                Some("Assign MGA and confirm source units to display the map.".into());
            return false;
        };
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.left_bottom(),
            rect.right_bottom(),
        ];
        let geographic = corners.map(|corner| {
            let [x, y] = to_drawing(corner);
            Coordinate::new(x, y)
                .and_then(|point| registration.to_project(point))
                .and_then(|point| projection.to_wgs84(point))
        });
        let [Ok(nw), Ok(ne), Ok(sw), Ok(se)] = geographic else {
            self.map_status = Some("Map is outside the configured coordinate area.".into());
            return false;
        };
        let geographic = [nw, ne, sw, se];
        let Some(range) = TileRange::covering(&geographic, self.tiles.maximum_zoom()) else {
            return false;
        };
        let range = self.tiles.update(painter.ctx(), range, &geographic);
        self.map_status = self.tiles.status().map(str::to_owned);
        let mut displayed = false;
        for tile in range.tiles() {
            let (base, labels) = self.tiles.textures(tile);
            if let Some(base) = base
                && let Some(mesh) = tile_mesh(tile, base.id(), self.opacity, |point| {
                    projection
                        .from_wgs84(point)
                        .and_then(|point| registration.to_drawing(point))
                        .ok()
                        .map(|point| to_screen([point.x(), point.y()]))
                })
            {
                if let Some(labels) = labels {
                    let mut overlay = mesh.clone();
                    overlay.texture_id = labels.id();
                    painter.add(egui::Shape::mesh(mesh));
                    painter.add(egui::Shape::mesh(overlay));
                } else {
                    painter.add(egui::Shape::mesh(mesh));
                }
                displayed = true;
            }
        }
        displayed
    }

    pub(super) fn paint_credits(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        rect: egui::Rect,
    ) -> Option<egui::Rect> {
        if let Some(credits) = self.tiles.credits() {
            let google = self.tiles.mode() == MapMode::Satellite;
            let panel_width = (rect.width() - 28.0).max(25.0);
            if google && panel_width >= 100.0 && self.google_logo.is_none() {
                let asset = include_bytes!("../../resources/google-maps-attribution.png");
                if let Ok(icon) = eframe::icon_data::from_png_bytes(asset) {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [icon.width as usize, icon.height as usize],
                        &icon.rgba,
                    );
                    self.google_logo = Some(painter.ctx().load_texture(
                        "Google Maps attribution",
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
            let logo = self
                .google_logo
                .as_ref()
                .filter(|_| google && panel_width >= 100.0);
            let credit = if google && logo.is_none() {
                format!("Google Maps · {credits}")
            } else {
                credits.to_owned()
            };
            let layout = painter.layout(
                credit,
                egui::FontId::proportional(11.0),
                egui::Color32::WHITE,
                panel_width,
            );
            let logo_size = logo.map_or(egui::Vec2::ZERO, |texture| {
                texture.size_vec2() * (17.0 / texture.size_vec2().y)
            });
            let header = if logo.is_some() { 23.0 } else { 0.0 };
            let width = layout.size().x.max(logo_size.x);
            let pos = egui::pos2(
                rect.left() + 12.0,
                rect.bottom() - 10.0 - layout.size().y - header,
            );
            let backing =
                egui::Rect::from_min_size(pos, egui::vec2(width, layout.size().y + header))
                    .expand(10.0);
            if let Some(texture) = logo {
                painter.image(
                    texture.id(),
                    egui::Rect::from_min_size(pos, logo_size),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            let text_pos = pos + egui::vec2(0.0, header);
            for offset in [
                egui::vec2(-1.0, 0.0),
                egui::vec2(1.0, 0.0),
                egui::vec2(0.0, -1.0),
                egui::vec2(0.0, 1.0),
            ] {
                painter.galley_with_override_text_color(
                    text_pos + offset,
                    layout.clone(),
                    egui::Color32::BLACK,
                );
            }
            painter.galley(text_pos, layout, egui::Color32::WHITE);
            let street = self.tiles.mode() == MapMode::Street;
            let hit = ui.interact(
                backing.intersect(rect),
                ui.id().with("map-attribution"),
                if street {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            hit.widget_info(|| {
                egui::WidgetInfo::labeled(
                    if street {
                        egui::WidgetType::Link
                    } else {
                        egui::WidgetType::Label
                    },
                    true,
                    if google {
                        format!("Google Maps · {credits}")
                    } else {
                        credits.to_owned()
                    },
                )
            });
            if street
                && hit
                    .on_hover_text("OpenStreetMap copyright and licence")
                    .clicked()
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(
                    "https://www.openstreetmap.org/copyright",
                ));
            }
            Some(backing.intersect(rect))
        } else {
            None
        }
    }
}

fn mode_label(mode: MapMode) -> &'static str {
    match mode {
        MapMode::Off => "Off",
        MapMode::Street => "OpenStreetMap Streets",
        MapMode::Satellite => "Google Satellite with Labels",
        MapMode::EsriSatellite => "Esri World Imagery with Labels",
    }
}

#[cfg(windows)]
fn credential(mode: MapMode) -> Result<keyring::Entry, &'static str> {
    let provider = match mode {
        MapMode::Satellite => "google-maps",
        MapMode::EsriSatellite => "esri",
        _ => return Err("This map provider needs no credentials."),
    };
    keyring::Entry::new("12dAZ Converter map backgrounds", provider)
        .map_err(|_| "Windows Credential Manager is unavailable.")
}

fn saved_key(mode: MapMode) -> Result<Option<String>, &'static str> {
    #[cfg(windows)]
    {
        match credential(mode)?.get_password() {
            Ok(key) if !key.trim().is_empty() => Ok(Some(key)),
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("Could not read the saved key from Windows Credential Manager."),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = mode;
        Ok(None)
    }
}

fn tile_mesh(
    tile: TileId,
    texture: egui::TextureId,
    opacity: f32,
    to_screen: impl Fn(LonLat) -> Option<egui::Pos2>,
) -> Option<egui::Mesh> {
    let project = |u, v| {
        tile_coordinate(tile, u, v)
            .ok()
            .and_then(&to_screen)
            .filter(|point| point.is_finite())
    };
    let tl = project(0.0, 0.0)?;
    let tr = project(1.0, 0.0)?;
    let br = project(1.0, 1.0)?;
    let bl = project(0.0, 1.0)?;
    let warp = project(0.5, 0.5)?
        .distance(tl.lerp(br, 0.5))
        .max(project(0.5, 0.0)?.distance(tl.lerp(tr, 0.5)))
        .max(project(0.5, 1.0)?.distance(bl.lerp(br, 0.5)))
        .max(project(0.0, 0.5)?.distance(tl.lerp(bl, 0.5)))
        .max(project(1.0, 0.5)?.distance(tr.lerp(br, 0.5)));
    let subdivisions = if warp > 1.0 {
        (warp.sqrt().ceil() as u32).clamp(2, 12)
    } else {
        1
    };
    let mut mesh = egui::Mesh::with_texture(texture);
    let color = egui::Color32::from_white_alpha((opacity.clamp(0.0, 1.0) * 255.0).round() as u8);
    let row_width = subdivisions + 1;
    for row in 0..=subdivisions {
        for column in 0..=subdivisions {
            let u = f64::from(column) / f64::from(subdivisions);
            let v = f64::from(row) / f64::from(subdivisions);
            mesh.vertices.push(egui::epaint::Vertex {
                pos: project(u, v)?,
                uv: egui::pos2(u as f32, v as f32),
                color,
            });
            if row < subdivisions && column < subdivisions {
                let a = row * row_width + column;
                mesh.indices.extend_from_slice(&[
                    a,
                    a + 1,
                    a + row_width + 1,
                    a,
                    a + row_width + 1,
                    a + row_width,
                ]);
            }
        }
    }
    Some(mesh)
}

impl Draft {
    fn registration(&self) -> Result<MapRegistration, String> {
        let crs = self.crs.ok_or("Choose the MGA datum and zone.")?;
        let unit = self.unit.ok_or("Confirm the source coordinate units.")?;
        if !self.local {
            return MapRegistration::assign(crs, unit).map_err(|error| error.to_string());
        }
        let projection = Projection::new(crs).map_err(|error| error.to_string())?;
        let control = |index: usize| {
            let point = &self.controls[index];
            let [x, y] = numbers(&point.drawing)?;
            let [a, b] = numbers(&point.map)?;
            let drawing = Coordinate::new(x, y).map_err(|error| error.to_string())?;
            let map = if self.geographic_controls {
                LonLat::new(b, a).and_then(|geographic| projection.from_wgs84(geographic))
            } else {
                Coordinate::new(a, b)
            }
            .map_err(|error| error.to_string())?;
            Ok::<_, String>((drawing, map))
        };
        let second = self.second.then(|| control(1)).transpose()?;
        MapRegistration::align(crs, unit, control(0)?, second).map_err(|error| error.to_string())
    }
}

fn numbers(values: &[String; 2]) -> Result<[f64; 2], String> {
    let number = |text: &str| {
        text.trim()
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
            .ok_or_else(|| String::from("Enter two finite coordinates for each control point."))
    };
    Ok([number(&values[0])?, number(&values[1])?])
}

fn coordinate_edits(ui: &mut egui::Ui, coordinates: &mut [String; 2], labels: [&str; 2]) {
    for (coordinate, label) in coordinates.iter_mut().zip(labels) {
        ui.add(
            egui::TextEdit::singleline(coordinate)
                .desired_width(100.0)
                .hint_text(label),
        );
    }
}

fn crs_label(crs: CrsId) -> String {
    crs.mga_details().map_or_else(
        || crs.epsg().to_owned(),
        |(datum, zone)| format!("{datum} / MGA zone {zone} ({})", crs.epsg()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_selection_loads_only_its_saved_key_and_preserves_session_overrides() {
        let mut backdrop = Backdrop::default();
        backdrop.select_provider(MapMode::Street, |_| panic!("Street maps do not use a key"));
        backdrop.select_provider(MapMode::Satellite, |mode| {
            assert_eq!(mode, MapMode::Satellite);
            Ok(Some("fixture-google".into()))
        });
        assert!(backdrop.tiles.has_key(MapMode::Satellite));
        assert!(!backdrop.tiles.has_key(MapMode::EsriSatellite));
        backdrop.select_provider(MapMode::EsriSatellite, |_| Ok(None));
        assert!(!backdrop.tiles.has_key(MapMode::EsriSatellite));
        backdrop.select_provider(MapMode::Satellite, |_| {
            panic!("An active key takes precedence")
        });
        backdrop.select_provider(MapMode::EsriSatellite, |mode| {
            assert_eq!(mode, MapMode::EsriSatellite);
            Ok(Some("fixture-esri".into()))
        });
        assert!(backdrop.tiles.has_key(MapMode::EsriSatellite));
    }

    #[test]
    fn esri_credits_wrap_at_lower_left_without_a_background() {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(240.0, 220.0));
        let mut backdrop = Backdrop::default();
        backdrop.tiles.select(MapMode::EsriSatellite);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            let credit = backdrop.paint_credits(ui, &painter, rect).unwrap();
            assert_eq!(credit.left(), rect.left() + 2.0);
            assert_eq!(credit.bottom(), rect.bottom());
            assert!(rect.contains_rect(credit));
        });
        output.textures_delta.clear();
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| matches!(shape.shape, egui::Shape::Text(_)))
        );
        assert!(
            output
                .shapes
                .iter()
                .all(|shape| !matches!(shape.shape, egui::Shape::Rect(_)))
        );
    }

    #[test]
    fn map_03_and_06_opening_another_archive_drops_the_previous_registration() {
        let folder = tempfile::tempdir().unwrap();
        let first = folder.path().join("first.12da");
        let second = folder.path().join("second.12da");
        let registration =
            MapRegistration::assign(CrsId::Gda2020MgaZone55, SourceUnit::Metres).unwrap();
        sidecar::save(&first, "first", registration, false).unwrap();
        let mut map = Backdrop::default();
        map.sync_source(Some((&first, "first")));
        assert_eq!(map.registration, Some(registration));
        map.sync_source(Some((&second, "second")));
        assert!(map.registration.is_none());
        assert!(map.projection.is_none());
        assert!(map.draft.registration().is_err());
        map.sync_source(Some((&first, "changed")));
        assert!(map.registration.is_none());
        assert!(map.message.contains("fingerprint"));
    }

    #[test]
    fn map_07_texture_vertices_follow_the_same_pan_and_zoom_as_survey_points() {
        let registration =
            MapRegistration::assign(CrsId::Gda2020MgaZone55, SourceUnit::Metres).unwrap();
        let projection = Projection::new(registration.crs()).unwrap();
        let location = LonLat::new(144.96, -37.81).unwrap();
        let tile =
            az12_converter::map::tile_at(location, az12_converter::map::Zoom::new(18).unwrap());
        let corner = tile_coordinate(tile, 0.0, 0.0).unwrap();
        let reference = registration
            .to_drawing(projection.from_wgs84(corner).unwrap())
            .unwrap();
        for (scale, pan) in [
            (1.0, egui::vec2(20.0, 30.0)),
            (2.5, egui::vec2(-100.0, 54.0)),
        ] {
            let screen = |point: Coordinate| {
                egui::pos2(
                    ((point.x() - 321000.0) * scale) as f32 + pan.x,
                    -((point.y() - 5812000.0) * scale) as f32 + pan.y,
                )
            };
            let mesh = tile_mesh(tile, egui::TextureId::User(1), 0.7, |location| {
                projection
                    .from_wgs84(location)
                    .and_then(|point| registration.to_drawing(point))
                    .ok()
                    .map(screen)
            })
            .unwrap();
            assert!(mesh.vertices[0].pos.distance(screen(reference)) < 1.0);
            assert_eq!(mesh.vertices[0].uv, egui::Pos2::ZERO);
            assert!(mesh.is_valid());
        }
    }

    #[test]
    fn map_08_unconfigured_preview_explains_why_map_is_unavailable() {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 400.0));
        let painter = ctx
            .layer_painter(egui::LayerId::background())
            .with_clip_rect(rect);
        let mut backdrop = Backdrop::default();
        backdrop.tiles.select(MapMode::Street);
        assert!(!backdrop.paint(
            &painter,
            rect,
            |_| panic!("map should stay disabled"),
            |_| panic!("map should stay disabled")
        ));
        assert!(backdrop.map_status.unwrap().contains("source units"));
    }
}
