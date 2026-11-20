#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod canvas;
mod smoke;
mod state;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use az12_converter::service::VERSION;
use eframe::egui::{self, Color32, RichText, Vec2};

use canvas::View;
use state::{State, output_exists};

const TITLE: &str = "12dAZ Converter";
const COPYRIGHT: &str = "Copyright (c) 2026 Lambda Technologies Pty Ltd ABN 22 647 165 160";
const SUPPORT: &str = "https://paypal.me/12dazapp";
const GUIDE: &str = include_str!("../../resources/user_guide.txt");
const LOGO: &[u8] = include_bytes!("../../resources/logo.png");
const LICENSE: &str = include_str!("../../../LICENSE");
const UNITS: [&str; 4] = ["metres", "millimetres", "feet", "inches"];
const ENCODINGS: [&str; 4] = ["cp1252", "utf-8", "utf-16-le", "utf-16-be"];
const BLUE: Color32 = Color32::from_rgb(35, 102, 188);

#[derive(Clone, Copy)]
enum Dialog {
    About,
    HelpMenu,
    Guide,
    License,
    Report,
    Overwrite,
}

struct Desktop {
    state: State,
    view: View,
    dialog: Option<Dialog>,
    splash_until: Option<Instant>,
    splash_focused: bool,
    closing: bool,
    notices: String,
}

impl Desktop {
    fn new(ctx: &egui::Context, splash: bool) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(38, 41, 46);
        visuals.window_fill = Color32::from_rgb(43, 46, 52);
        visuals.selection.bg_fill = BLUE;
        visuals.widgets.active.bg_fill = BLUE;
        ctx.set_theme(egui::Theme::Dark);
        ctx.set_visuals_of(egui::Theme::Dark, visuals);
        Self {
            state: State::default(),
            view: View::default(),
            dialog: None,
            splash_until: splash.then(|| Instant::now() + Duration::from_millis(1800)),
            splash_focused: false,
            closing: false,
            notices: license_notices(),
        }
    }

    fn select_archive(&mut self) {
        if !self.state.can_open() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open 12d archive")
            .add_filter("12d archives", &["12daz", "12da"])
            .pick_file()
        {
            self.state.open(path);
        }
    }

    fn choose_output(&mut self) {
        let output = Path::new(&self.state.output);
        let mut picker = rfd::FileDialog::new()
            .set_title("Save DXF")
            .add_filter("DXF drawings", &["dxf"]);
        if let Some(name) = output.file_name().and_then(|value| value.to_str()) {
            picker = picker.set_file_name(name);
        }
        if let Some(parent) = output.parent() {
            picker = picker.set_directory(parent);
        }
        if let Some(mut path) = picker.save_file() {
            if path.extension().is_none() {
                path.set_extension("dxf");
            }
            self.state.output = path.to_string_lossy().into_owned();
            self.state.settings_changed();
        }
    }

    fn start_export(&mut self, overwrite: bool) {
        if !overwrite && output_exists(&self.state.output) {
            self.dialog = Some(Dialog::Overwrite);
            return;
        }
        if let Err(error) = self.state.export(overwrite) {
            self.state.status = format!("Failed · {error}");
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui
                    .add_enabled(self.state.can_open(), egui::Button::new("Open archive…"))
                    .clicked()
                {
                    ui.close();
                    self.select_archive();
                }
                if ui.button("Exit").clicked() {
                    ui.close();
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Help", |ui| {
                for (label, dialog) in [
                    ("User Guide", Dialog::Guide),
                    ("About 12dAZ Converter", Dialog::About),
                    ("License and Third-party Notices", Dialog::License),
                ] {
                    if ui.button(label).clicked() {
                        self.dialog = Some(dialog);
                        ui.close();
                    }
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.heading(TITLE);
            if ui
                .add_enabled(
                    self.state.can_open(),
                    egui::Button::new("Open archive…").fill(BLUE),
                )
                .clicked()
            {
                self.select_archive();
            }
            let filename = self
                .state
                .input
                .as_deref()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy());
            ui.label(
                filename
                    .as_deref()
                    .unwrap_or("Open a local .12daz or .12da file to begin"),
            );
        });
    }

    fn models(&mut self, ui: &mut egui::Ui) {
        ui.heading("Models");
        ui.add_enabled(
            self.state.scan.is_some(),
            egui::TextEdit::singleline(&mut self.state.search)
                .hint_text("Search models")
                .desired_width(f32::INFINITY),
        );
        ui.add_enabled_ui(self.state.can_select(), |ui| {
            ui.horizontal(|ui| {
                if ui.button("Select all").clicked() {
                    self.state.select_all(true);
                }
                if ui.button("Clear").clicked() {
                    self.state.select_all(false);
                }
            });
        });
        ui.separator();
        let mut toggled = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let enabled = self.state.can_select();
                for model in self.state.visible_models() {
                    let mut checked = self.state.selected.contains(&model.name);
                    let row = ui.add_enabled(
                        enabled && model.convertible > 0,
                        egui::Checkbox::new(
                            &mut checked,
                            format!("{}  ({})", model.name, model.count),
                        ),
                    );
                    if row.changed() {
                        toggled = Some(model.name.clone());
                    }
                }
            });
        if let Some(model) = toggled {
            self.state.toggle(&model);
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Export settings");
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.add_enabled_ui(!self.state.busy(), |ui| {
                    ui.label("Geometry");
                    let before = self.state.keep_z;
                    egui::ComboBox::from_id_salt("geometry")
                        .width(ui.available_width())
                        .selected_text(if before { "Keep Z" } else { "Flatten 2D" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.state.keep_z, false, "Flatten 2D");
                            ui.selectable_value(&mut self.state.keep_z, true, "Keep Z");
                        });
                    if before != self.state.keep_z {
                        self.state.mode_changed();
                    }
                    if self.state.keep_z {
                        ui.label("Plan tolerance");
                        if ui
                            .text_edit_singleline(&mut self.state.plan_tolerance)
                            .changed()
                        {
                            self.state.settings_changed();
                        }
                        ui.label("Elevation tolerance");
                        if ui
                            .text_edit_singleline(&mut self.state.elevation_tolerance)
                            .changed()
                        {
                            self.state.settings_changed();
                        }
                        if ui
                            .checkbox(
                                &mut self.state.linear_arc_elevations,
                                "Assume linear grade on arcs",
                            )
                            .changed()
                        {
                            self.state.settings_changed();
                        }
                    }
                    ui.add_space(8.0);
                    ui.label("Drawing units");
                    let before = self.state.units;
                    egui::ComboBox::from_id_salt("units")
                        .width(ui.available_width())
                        .selected_text(before.unwrap_or("Choose units…"))
                        .show_ui(ui, |ui| {
                            for unit in UNITS {
                                ui.selectable_value(&mut self.state.units, Some(unit), unit);
                            }
                        });
                    if before != self.state.units {
                        self.state.settings_changed();
                    }
                    ui.add_space(8.0);
                    ui.label("Output DXF");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.state.output)
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        self.state.settings_changed();
                    }
                    if ui.button("Browse…").clicked() {
                        self.choose_output();
                    }
                    ui.add_space(8.0);
                    ui.collapsing("Input options", |ui| {
                        ui.label("Fallback encoding");
                        let previous = self.state.encoding;
                        egui::ComboBox::from_id_salt("encoding")
                            .selected_text(previous.unwrap_or("Automatic (BOM / UTF-8)"))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.state.encoding,
                                    None,
                                    "Automatic (BOM / UTF-8)",
                                );
                                for encoding in ENCODINGS {
                                    ui.selectable_value(
                                        &mut self.state.encoding,
                                        Some(encoding),
                                        encoding,
                                    );
                                }
                            });
                        if previous != self.state.encoding
                            && let Some(path) = self.state.input.clone()
                        {
                            self.state.open(path);
                        }
                    });
                });
            });
        ui.add_space(8.0);
        if ui
            .add_enabled(
                self.state.can_export(),
                egui::Button::new(RichText::new("Export DXF").strong())
                    .fill(BLUE)
                    .min_size(Vec2::new(ui.available_width(), 32.0)),
            )
            .clicked()
        {
            self.start_export(false);
        }
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Plan preview");
            if ui.button("Fit").clicked() {
                self.state.fit_pending = true;
            }
            egui::ComboBox::from_id_salt("zoom")
                .selected_text(format!("{}%", self.view.zoom_percent()))
                .show_ui(ui, |ui| {
                    for percent in [25, 50, 75, 100, 125, 150, 200, 400] {
                        if ui
                            .selectable_label(
                                self.view.zoom_percent() == percent,
                                format!("{percent}%"),
                            )
                            .clicked()
                        {
                            self.view.set_zoom_percent(percent);
                        }
                    }
                });
            if let Some([x, y]) = self.view.cursor {
                ui.monospace(format!("X {x:.3}   Y {y:.3}"));
            }
        });
        if self.state.preview_simplified {
            let dropped = self.state.preview_omitted.len();
            if dropped > 0 {
                ui.label(format!("Simplified preview · {dropped} models omitted"));
            } else {
                ui.label("Simplified preview");
            }
        }
        let visible: Vec<_> = self
            .state
            .preview
            .iter()
            .filter(|record| self.state.selected.contains(&record.model))
            .collect();
        self.view.show(
            ui,
            &visible,
            std::mem::take(&mut self.state.fit_pending),
            self.state.units,
        );
    }

    fn status(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if self.state.waiting() {
                ui.spinner();
            }
            ui.label(&self.state.status);
            if self.state.scan.is_some() {
                ui.label(format!("{} models selected", self.state.selected.len()));
            }
            if self.state.waiting()
                && ui
                    .add_enabled(
                        !self.state.cancellation_requested(),
                        egui::Button::new("Cancel"),
                    )
                    .clicked()
            {
                self.state.cancel();
            }
            if let Some(report) = self.state.report() {
                if ui
                    .button(format!("Report · {} warnings", report.diagnostics.total))
                    .clicked()
                {
                    self.dialog = Some(Dialog::Report);
                }
                if let Some(path) = report.output.as_deref().and_then(Path::parent)
                    && ui.button("Open output folder").clicked()
                    && let Err(error) = open_folder(path)
                {
                    self.state.status = format!("Cannot open output folder · {error}");
                }
            }
        });
    }

    fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.dialog else { return };
        let mut close = false;
        let mut overwrite = false;
        let mut followup = None;
        let shown = egui::Modal::new(egui::Id::new("desktop-dialog")).show(ctx, |ui| {
            ui.set_max_width(560.0);
            match dialog {
                Dialog::HelpMenu => {
                    ui.heading("Help");
                    for (label, target) in [
                        ("User Guide", Dialog::Guide),
                        ("About 12dAZ Converter", Dialog::About),
                        ("License and Third-party Notices", Dialog::License),
                    ] {
                        if ui.button(label).clicked() {
                            followup = Some(target);
                        }
                    }
                }
                Dialog::About => {
                    ui.heading(TITLE);
                    ui.label(format!("Version {VERSION} · MIT License"));
                    ui.add(egui::Label::new(COPYRIGHT).selectable(true).wrap());
                    ui.add_space(12.0);
                    ui.hyperlink_to("Support on PayPal", SUPPORT);
                }
                Dialog::Guide => {
                    ui.heading("User Guide");
                    scroll_text(ui, GUIDE);
                }
                Dialog::License => {
                    ui.heading("License and Third-party Notices");
                    scroll_text(ui, &format!("{LICENSE}\n\n{}", self.notices));
                }
                Dialog::Report => {
                    ui.heading("Conversion report");
                    if let Some(report) = self.state.report()
                        && let Ok(json) = serde_json::to_string_pretty(report)
                    {
                        scroll_text(ui, &json);
                    }
                }
                Dialog::Overwrite => {
                    ui.heading("Replace existing output?");
                    ui.label("Replace the existing DXF and its conversion report?");
                    overwrite = ui.button("Replace").clicked();
                }
            }
            close = ui
                .button(if matches!(dialog, Dialog::Overwrite) {
                    "Cancel"
                } else {
                    "Close"
                })
                .clicked();
        });
        if close || overwrite || shown.should_close() {
            self.dialog = None;
        }
        if followup.is_some() {
            self.dialog = followup;
        }
        if overwrite {
            self.start_export(true);
        }
    }

    fn splash(&mut self, ctx: &egui::Context) {
        let Some(until) = self.splash_until else {
            return;
        };
        let now = Instant::now();
        if now >= until {
            self.splash_until = None;
            return;
        }
        ctx.request_repaint_after(until - now);
        let modal = egui::Modal::new(egui::Id::new("startup-splash")).show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.heading(TITLE);
            ui.label("Offline 12d archive to DXF converter");
            ui.add_space(12.0);
            ui.hyperlink_to("Support on PayPal", SUPPORT);
            let button = ui.button("Continue");
            if !self.splash_focused {
                button.request_focus();
                self.splash_focused = true;
            }
            button.clicked()
        });
        if modal.inner || modal.should_close() {
            self.splash_until = None;
        }
    }
}

impl Desktop {
    fn draw_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.dialog.is_none() && self.splash_until.is_none() {
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
                self.select_archive();
            }
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::H)) {
                self.dialog = Some(Dialog::HelpMenu);
            }
        }
        egui::Panel::top("app-menu").show(ui, |ui| self.menu(ui));
        egui::Panel::bottom("app-status").show(ui, |ui| self.status(ui));
        egui::Panel::left("model-list")
            .resizable(true)
            .default_size(240.0)
            .min_size(175.0)
            .show(ui, |ui| self.models(ui));
        egui::Panel::right("export-settings")
            .resizable(true)
            .default_size(250.0)
            .min_size(205.0)
            .show(ui, |ui| self.settings(ui));
        egui::CentralPanel::default().show(ui, |ui| self.preview(ui));
        self.show_dialog(&ctx);
        self.splash(&ctx);
    }
}

impl eframe::App for Desktop {
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.state.tick();
        if self.state.waiting() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
        if ctx.input(|input| input.viewport().close_requested()) && self.state.waiting() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.closing = true;
            self.state.cancel();
        }
        if self.closing && !self.state.waiting() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.draw_ui(ui);
    }
}

fn scroll_text(ui: &mut egui::Ui, text: &str) {
    egui::ScrollArea::vertical()
        .max_height(350.0)
        .show(ui, |ui| {
            ui.add(egui::Label::new(text).selectable(true).wrap());
        });
}

fn license_notices() -> String {
    std::env::current_exe().ok().and_then(|path| path.parent().map(|parent| parent.join("THIRD_PARTY_NOTICES.txt")))
        .and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_else(|| {
            "Third-party notices\n\nThe Rust desktop uses acadrust (MPL-2.0), egui/eframe and the other dependencies listed in Cargo.lock. The portable distribution includes their full notices.".into()
        })
}

fn open_folder(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    let program = "explorer.exe";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program).arg(path).spawn()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = args.next();
    if input.as_deref() == Some(std::ffi::OsStr::new("--smoke-test")) {
        let report = args.next().ok_or("Supply the smoke-test report path.")?;
        return smoke::run(PathBuf::from(report));
    }
    #[cfg(debug_assertions)]
    if input.as_deref() == Some(std::ffi::OsStr::new("--capture-layout")) {
        let fixture = args.next().ok_or("Supply the capture fixture path.")?;
        let output = args.next().ok_or("Supply the capture PNG path.")?;
        let scale = args
            .next()
            .and_then(|value| value.to_string_lossy().parse::<f32>().ok())
            .ok_or("Supply a display scale.")?;
        return smoke::capture(PathBuf::from(fixture), PathBuf::from(output), scale);
    }
    let input = input.map(PathBuf::from);
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 760.0])
        .with_min_inner_size([760.0, 485.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(LOGO) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport,
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        TITLE,
        options,
        Box::new(move |creation| {
            let mut app = Desktop::new(&creation.egui_ctx, true);
            if let Some(input) = input {
                app.state.open(input);
            }
            Ok(Box::new(app))
        }),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        desktop: &mut Desktop,
        scale: f32,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(760.0, 485.0));
        let mut input = egui::RawInput {
            screen_rect: Some(bounds),
            events,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(scale);
        let mut output = ctx.run_ui(input, |ui| desktop.draw_ui(ui));
        output.textures_delta.clear();
        output
    }

    fn pressed(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn rust_16_controls_fit_minimum_window_at_three_display_scales() {
        for scale in [1.0, 1.5, 2.0] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut desktop = Desktop::new(&ctx, false);
            let output = frame(&ctx, &mut desktop, scale, vec![]);
            assert_eq!(output.pixels_per_point, scale);
            let nodes = output.platform_output.accesskit_update.unwrap().nodes;
            for label in ["Open archive…", "Fit", "Export DXF"] {
                let control = nodes
                    .iter()
                    .find(|(_, node)| node.label() == Some(label))
                    .unwrap_or_else(|| panic!("missing {label} at {scale}"));
                let bounds = control.1.bounds().unwrap();
                assert!(
                    bounds.x0 >= 0.0
                        && bounds.x1 <= 760.0
                        && bounds.y0 >= 0.0
                        && bounds.y1 <= 485.0,
                    "{label}: {bounds:?} at {scale}"
                );
            }
        }
    }

    #[test]
    fn rust_16_keyboard_help_splash_and_offline_notices() {
        let ctx = egui::Context::default();
        let mut desktop = Desktop::new(&ctx, true);
        assert!(desktop.splash_until.is_some());
        frame(&ctx, &mut desktop, 1.0, vec![]);
        frame(
            &ctx,
            &mut desktop,
            1.0,
            vec![pressed(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert!(desktop.splash_until.is_none());
        frame(
            &ctx,
            &mut desktop,
            1.0,
            vec![pressed(egui::Key::H, egui::Modifiers::ALT)],
        );
        assert!(matches!(desktop.dialog, Some(Dialog::HelpMenu)));
        desktop.dialog = Some(Dialog::About);
        frame(&ctx, &mut desktop, 1.0, vec![]);
        frame(
            &ctx,
            &mut desktop,
            1.0,
            vec![pressed(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert!(desktop.dialog.is_none());
        assert!(LICENSE.contains(COPYRIGHT) && GUIDE.contains("Export DXF"));
        assert!(desktop.notices.contains("acadrust"));
        assert_eq!(SUPPORT, "https://paypal.me/12dazapp");
    }
}
