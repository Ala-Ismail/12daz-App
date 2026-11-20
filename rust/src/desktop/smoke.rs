use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use az12_converter::dxf::validate_dxf;
use eframe::egui;

use crate::{COPYRIGHT, Desktop, GUIDE, LICENSE, TITLE, VERSION};

struct Smoke {
    desktop: Desktop,
    export_started: bool,
    outcome: Rc<RefCell<Option<Result<(), String>>>>,
    deadline: Instant,
}

impl Smoke {
    fn step(&mut self) -> Option<Result<(), String>> {
        if Instant::now() >= self.deadline {
            return Some(Err("Desktop workflow timed out.".into()));
        }
        self.desktop.state.tick();
        let state = &mut self.desktop.state;
        if state.waiting() {
            return None;
        }
        if !self.export_started {
            if state.scan.is_none() || state.selected.len() != 1 || state.preview.is_empty() {
                return Some(Err(format!("Scan or preview failed: {}", state.status)));
            }
            state.units = Some("metres");
            if let Err(error) = state.export(false) {
                return Some(Err(error.to_string()));
            }
            self.export_started = true;
            return None;
        }
        let Some(report) = &state.result else {
            return Some(Err(format!("Export failed: {}", state.status)));
        };
        let Some(output) = &report.output else {
            return Some(Err("The export report is missing its DXF path.".into()));
        };
        if let Err(error) = validate_dxf(output, 1, false, None) {
            return Some(Err(error.to_string()));
        }
        if !report
            .report_path
            .as_ref()
            .is_some_and(|path| path.is_file())
        {
            return Some(Err("The JSON conversion report is missing.".into()));
        }
        if let Err(error) = compare_cli(
            state.input.as_ref().expect("scan loaded an archive"),
            report,
        ) {
            return Some(Err(error.to_string()));
        }
        if self.desktop.splash_until.is_some()
            || !GUIDE.contains("Export DXF")
            || !LICENSE.contains(COPYRIGHT)
            || !self.desktop.notices.contains("acadrust")
        {
            return Some(Err(
                "Offline resources or startup mode failed verification.".into(),
            ));
        }
        Some(Ok(()))
    }
}

fn compare_cli(
    input: &std::path::Path,
    gui: &az12_converter::service::Report,
) -> Result<(), Box<dyn std::error::Error>> {
    let executable = std::env::current_exe()?
        .with_file_name(format!("az12-converter{}", std::env::consts::EXE_SUFFIX));
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("drawing.dxf");
    let mut command = std::process::Command::new(executable);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let invocation = command
        .arg("convert")
        .arg(input)
        .arg("--output")
        .arg(&output)
        .args(["--units", "metres", "--model", "Survey"])
        .output()?;
    if !invocation.status.success() {
        return Err(format!(
            "CLI smoke export failed: {}",
            String::from_utf8_lossy(&invocation.stderr)
        )
        .into());
    }
    let cli: serde_json::Value = serde_json::from_slice(&invocation.stdout)?;
    let desktop = serde_json::to_value(gui)?;
    for key in [
        "input_sha256",
        "options",
        "models",
        "source_counts",
        "bounds",
        "emitted_entities",
        "layer_mapping",
        "diagnostics",
        "published",
    ] {
        if cli[key] != desktop[key] {
            return Err(format!("CLI and GUI reports differ in {key}.").into());
        }
    }
    for path in [gui.output.as_deref().ok_or("GUI drawing missing")?, &output] {
        let drawing = acadrust::DxfReader::from_file(path)?.read()?;
        let mut entities = drawing.model_space_entities();
        let Some(acadrust::EntityType::LwPolyline(polyline)) = entities.next() else {
            return Err("Expected one planar polyline in the GUI and CLI smoke drawings.".into());
        };
        let vertices: Vec<_> = polyline
            .vertices
            .iter()
            .map(|vertex| (vertex.location.x, vertex.location.y, vertex.bulge))
            .collect();
        if vertices != [(100.0, 200.0, 0.0), (102.0, 206.0, 0.0)]
            || polyline.elevation != 0.0
            || entities.next().is_some()
        {
            return Err("GUI or CLI smoke drawing has unexpected geometry.".into());
        }
    }
    Ok(())
}

impl eframe::App for Smoke {
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if let Some(result) = self.step() {
            *self.outcome.borrow_mut() = Some(result);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.request_repaint_after(Duration::from_millis(25));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.desktop.draw_ui(ui);
    }
}

pub fn run(report: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let folder = tempfile::tempdir()?;
    let input = folder.path().join("測量 smoke test.12da");
    fs::write(
        &input,
        "// archive_version \"15.01.08.60\"\nmodel Survey string 3d { breakline line data { 100 200 4 102 206 5 } }",
    )?;
    let outcome = Rc::new(RefCell::new(None));
    let app_outcome = Rc::clone(&outcome);
    let app_result = eframe::run_native(
        TITLE,
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            viewport: egui::ViewportBuilder::default().with_visible(false),
            ..eframe::NativeOptions::default()
        },
        Box::new(move |creation| {
            let mut desktop = Desktop::new(&creation.egui_ctx, false);
            desktop.state.open(input);
            Ok(Box::new(Smoke {
                desktop,
                export_started: false,
                outcome: app_outcome,
                deadline: Instant::now() + Duration::from_secs(30),
            }))
        }),
    );
    let result = match app_result {
        Ok(()) => outcome
            .borrow_mut()
            .take()
            .unwrap_or_else(|| Err("Desktop exited before the smoke test finished.".into())),
        Err(error) => Err(error.to_string()),
    };
    let evidence = serde_json::json!({"passed": result.is_ok(), "application_version": VERSION, "error": result.as_ref().err()});
    fs::write(report, serde_json::to_vec_pretty(&evidence)?)?;
    result.map_err(Into::into)
}

#[cfg(debug_assertions)]
pub fn capture(
    input: PathBuf,
    output: PathBuf,
    scale: f32,
    mode: az12_converter::map::MapMode,
    opacity: Option<f32>,
) -> Result<(), Box<dyn std::error::Error>> {
    use eframe::icon_data::IconDataExt;

    struct Capture {
        desktop: Desktop,
        frames: usize,
        mode: az12_converter::map::MapMode,
        opacity: Option<f32>,
        provider_started: bool,
        capture_sent: bool,
        result: Rc<RefCell<Option<Result<(), String>>>>,
        output: PathBuf,
    }
    impl eframe::App for Capture {
        fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
            self.desktop.state.tick();
            if let Some(image) = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            }) {
                let icon = egui::IconData {
                    rgba: image
                        .pixels
                        .iter()
                        .flat_map(|pixel| pixel.to_array())
                        .collect(),
                    width: image.size[0] as u32,
                    height: image.size[1] as u32,
                };
                let result = icon.to_png_bytes().and_then(|bytes| {
                    fs::write(&self.output, bytes).map_err(|error| error.to_string())
                });
                *self.result.borrow_mut() = Some(result);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
            self.desktop.draw_ui(ui);
            if !self.desktop.state.waiting() {
                self.desktop.state.units = Some("metres");
                self.frames += 1;
                let ready = if self.mode != az12_converter::map::MapMode::Off {
                    if !self.provider_started {
                        if let Err(error) = self
                            .desktop
                            .backdrop
                            .start_capture_provider(self.mode, self.opacity)
                        {
                            *self.result.borrow_mut() = Some(Err(error));
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            return;
                        }
                        self.provider_started = true;
                    }
                    self.desktop.backdrop.capture_complete()
                } else {
                    true
                };
                if self.frames >= 500 && !ready {
                    *self.result.borrow_mut() = Some(Err(self
                        .desktop
                        .backdrop
                        .map_status
                        .clone()
                        .unwrap_or_else(|| "The map did not finish loading.".into())));
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                } else if !self.capture_sent && self.frames >= 4 && ready {
                    self.capture_sent = true;
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Screenshot(
                            egui::UserData::default(),
                        ));
                }
            }
            ui.ctx().request_repaint_after(Duration::from_millis(40));
        }
    }

    let result = Rc::new(RefCell::new(None));
    let capture_result = Rc::clone(&result);
    eframe::run_native(
        TITLE,
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([760.0 * scale, 485.0 * scale]),
            ..eframe::NativeOptions::default()
        },
        Box::new(move |creation| {
            let native = creation.egui_ctx.native_pixels_per_point().unwrap_or(1.0);
            creation.egui_ctx.set_zoom_factor(scale / native);
            let mut desktop = Desktop::new(&creation.egui_ctx, false);
            desktop.state.open(input);
            Ok(Box::new(Capture {
                desktop,
                frames: 0,
                mode,
                opacity,
                provider_started: false,
                capture_sent: false,
                result: capture_result,
                output,
            }))
        }),
    )?;
    result
        .borrow_mut()
        .take()
        .ok_or("The desktop did not return a capture.")?
        .map_err(Into::into)
}
