use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use az12_converter::archive::Limits;
use az12_converter::error::{ConversionError, Result};
use az12_converter::preview::PreviewRecord;
use az12_converter::service::{Job, Options, PREVIEW_LIMIT, Report, run_job};
use unicode_casefold::UnicodeCaseFold;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Inspect,
    Preview,
    Export,
}

struct Request {
    kind: Kind,
    revision: u64,
    ready: Instant,
}

struct Running {
    kind: Kind,
    revision: u64,
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<Option<(&'static str, u64)>>>,
    result: mpsc::Receiver<Result<Report>>,
}

pub struct State {
    pub input: Option<PathBuf>,
    pub output: String,
    pub selected: BTreeSet<String>,
    pub search: String,
    pub scan: Option<Report>,
    pub result: Option<Report>,
    pub preview: Vec<PreviewRecord>,
    pub preview_simplified: bool,
    pub preview_omitted: Vec<String>,
    pub fit_pending: bool,
    pub status: String,
    pub units: Option<&'static str>,
    pub encoding: Option<&'static str>,
    pub keep_z: bool,
    pub plan_tolerance: String,
    pub elevation_tolerance: String,
    pub linear_arc_elevations: bool,
    revision: u64,
    running: Option<Running>,
    pending: Option<Request>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            input: None,
            output: String::new(),
            selected: BTreeSet::new(),
            search: String::new(),
            scan: None,
            result: None,
            preview: vec![],
            preview_simplified: false,
            preview_omitted: vec![],
            fit_pending: true,
            status: "Open a local .12daz or .12da file to begin".into(),
            units: None,
            encoding: None,
            keep_z: false,
            plan_tolerance: "0.01".into(),
            elevation_tolerance: "0.01".into(),
            linear_arc_elevations: false,
            revision: 0,
            running: None,
            pending: None,
        }
    }
}

impl State {
    pub fn busy(&self) -> bool {
        self.running.is_some()
    }

    pub fn waiting(&self) -> bool {
        self.busy() || self.pending.is_some()
    }

    pub fn can_open(&self) -> bool {
        self.running
            .as_ref()
            .is_none_or(|job| job.kind == Kind::Preview)
    }

    pub fn can_select(&self) -> bool {
        self.scan.is_some() && self.can_open()
    }

    pub fn cancellation_requested(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|job| job.cancel.load(Ordering::Relaxed))
    }

    pub fn can_export(&self) -> bool {
        let path = Path::new(&self.output);
        !self.waiting()
            && self.scan.is_some()
            && !self.selected.is_empty()
            && self.units.is_some()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dxf"))
            && path.parent().is_some_and(Path::is_dir)
    }

    pub fn report(&self) -> Option<&Report> {
        self.result.as_ref().or(self.scan.as_ref())
    }

    pub fn settings_changed(&mut self) {
        self.result = None;
        if !self.waiting() && self.scan.is_some() {
            self.status = "Ready to review selected models and export settings".into();
        }
    }

    pub fn mode_changed(&mut self) {
        if let Some(input) = &self.input {
            let current = Path::new(&self.output);
            if [default_output(input, false), default_output(input, true)]
                .contains(&current.to_owned())
            {
                self.output = default_output(input, self.keep_z)
                    .to_string_lossy()
                    .into_owned();
            }
        }
        self.settings_changed();
    }

    pub fn open(&mut self, path: PathBuf) {
        if !self.can_open() {
            return;
        }
        self.cancel();
        self.revision += 1;
        self.output = default_output(&path, self.keep_z)
            .to_string_lossy()
            .into_owned();
        self.input = Some(path);
        self.scan = None;
        self.result = None;
        self.selected.clear();
        self.preview.clear();
        self.preview_simplified = false;
        self.preview_omitted.clear();
        self.units = None;
        self.fit_pending = true;
        self.pending = Some(Request {
            kind: Kind::Inspect,
            revision: self.revision,
            ready: Instant::now(),
        });
        self.status = "Reading models…".into();
        self.tick();
    }

    pub fn visible_models(&self) -> impl Iterator<Item = &az12_converter::service::Model> {
        let query: String = self.search.case_fold().collect();
        self.scan
            .iter()
            .flat_map(|scan| &scan.models)
            .filter(move |model| model.name.case_fold().collect::<String>().contains(&query))
    }

    pub fn toggle(&mut self, model: &str) {
        if !self.can_select() {
            return;
        }
        if !self.selected.remove(model) {
            if !self.scan.iter().any(|scan| {
                scan.models
                    .iter()
                    .any(|item| item.name == model && item.convertible > 0)
            }) {
                return;
            }
            self.selected.insert(model.into());
        }
        self.selection_changed();
    }

    pub fn select_all(&mut self, all: bool) {
        if !self.can_select() {
            return;
        }
        let selected = if all {
            self.scan
                .iter()
                .flat_map(|scan| &scan.models)
                .filter(|model| model.convertible > 0)
                .map(|model| model.name.clone())
                .collect()
        } else {
            BTreeSet::new()
        };
        if self.selected != selected {
            self.selected = selected;
            self.selection_changed();
        }
    }

    fn selection_changed(&mut self) {
        self.settings_changed();
        self.fit_pending = true;
        self.cancel();
        self.revision += 1;
        if !self.selected.is_empty() {
            self.pending = Some(Request {
                kind: Kind::Preview,
                revision: self.revision,
                ready: Instant::now() + Duration::from_millis(300),
            });
            self.status = "Updating preview for selected models…".into();
        }
    }

    pub fn cancel(&mut self) {
        let pending = self.pending.take().is_some();
        if let Some(job) = &self.running {
            job.cancel.store(true, Ordering::Relaxed);
            self.status = "Cancellation requested · waiting for cleanup".into();
        } else if pending {
            self.status = "Cancelled.".into();
        }
    }

    pub fn export(&mut self, overwrite: bool) -> Result<()> {
        if !self.can_export() {
            return Err(ConversionError::message(
                "Select convertible models, drawing units and a DXF destination.",
            ));
        }
        let mut options = self.selection_options();
        options.units = self.units.map(str::to_owned);
        options.overwrite = overwrite;
        if self.keep_z {
            options.geometry_mode = "keep-z".into();
            options.plan_tolerance = tolerance(&self.plan_tolerance)?;
            options.elevation_tolerance = tolerance(&self.elevation_tolerance)?;
            options.linear_arc_elevations = self.linear_arc_elevations;
        }
        self.start(
            Kind::Export,
            self.revision,
            options,
            Some(PathBuf::from(&self.output)),
        );
        Ok(())
    }

    fn selection_options(&self) -> Options {
        Options {
            models: Some(self.selected.iter().cloned().collect()),
            encoding: self.encoding.map(str::to_owned),
            ..Options::default()
        }
    }

    fn start(&mut self, kind: Kind, revision: u64, options: Options, output: Option<PathBuf>) {
        let Some(input) = self.input.clone() else {
            return;
        };
        let (sender, result) = mpsc::sync_channel(1);
        let progress = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_progress = Arc::clone(&progress);
        let worker_cancel = Arc::clone(&cancel);
        let started = std::thread::Builder::new()
            .name("12daz-conversion".into())
            .spawn(move || {
                let mut on_progress = |stage: &str, amount| {
                    let stage = match stage {
                        "Reading archive" => "Reading archive",
                        "Inspecting" => "Inspecting",
                        "Converting" => "Converting",
                        _ => "Writing and validating DXF",
                    };
                    if let Ok(mut progress) = worker_progress.lock() {
                        *progress = Some((stage, amount));
                    }
                };
                let report = run_job(
                    &input,
                    &options,
                    Job {
                        output: output.as_deref(),
                        report: None,
                        limits: Limits::default(),
                        cancel: Some(&worker_cancel),
                        progress: &mut on_progress,
                        preview_limit: PREVIEW_LIMIT,
                    },
                );
                let _ = sender.send(report);
            });
        if let Err(error) = started {
            self.status = format!("Failed to start conversion worker · {error}");
            return;
        }
        self.running = Some(Running {
            kind,
            revision,
            cancel,
            progress,
            result,
        });
        self.status = match kind {
            Kind::Inspect => "Reading archive…",
            Kind::Preview => "Updating preview…",
            Kind::Export => "Exporting DXF…",
        }
        .into();
    }

    pub fn tick(&mut self) {
        if let Some(job) = &self.running {
            if job.revision == self.revision
                && !job.cancel.load(Ordering::Relaxed)
                && let Ok(mut progress) = job.progress.lock()
                && let Some((stage, size)) = progress.take()
            {
                self.status = format!("{stage} · {size}");
            }
            let result = match job.result.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(ConversionError::message(
                    "Conversion worker stopped unexpectedly.",
                ))),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                let kind = job.kind;
                let revision = job.revision;
                self.running = None;
                if revision == self.revision {
                    self.finish(kind, result);
                }
            }
        }
        if self.running.is_none()
            && self
                .pending
                .as_ref()
                .is_some_and(|request| request.ready <= Instant::now())
        {
            let request = self.pending.take().expect("this request is due");
            let options = if request.kind == Kind::Preview {
                self.selection_options()
            } else {
                Options {
                    encoding: self.encoding.map(str::to_owned),
                    ..Options::default()
                }
            };
            self.start(request.kind, request.revision, options, None);
        }
    }

    fn finish(&mut self, kind: Kind, result: Result<Report>) {
        match result {
            Ok(mut report) => match kind {
                Kind::Inspect => {
                    self.selected = report
                        .models
                        .iter()
                        .filter(|model| model.convertible > 0)
                        .map(|model| model.name.clone())
                        .collect();
                    self.accept_preview(&mut report);
                    self.scan = Some(report);
                    self.status = "Archive loaded".into();
                }
                Kind::Preview => {
                    self.accept_preview(&mut report);
                    self.status = "Selected-model preview updated".into();
                }
                Kind::Export => {
                    self.status = format!(
                        "DXF exported · {} entities · {} warnings",
                        report.emitted_entities.unwrap_or_default(),
                        report.diagnostics.total
                    );
                    self.result = Some(report);
                }
            },
            Err(ConversionError::Cancelled) => self.status = "Cancelled.".into(),
            Err(error) => self.status = format!("Failed · {error}"),
        }
    }

    fn accept_preview(&mut self, report: &mut Report) {
        self.preview = std::mem::take(&mut report.preview);
        self.preview_simplified = report.preview_simplified;
        self.preview_omitted
            .clone_from(&report.preview_omitted_models);
        self.fit_pending = true;
    }
}

pub fn default_output(input: &Path, keep_z: bool) -> PathBuf {
    let mut name = input.file_stem().unwrap_or_default().to_owned();
    name.push(if keep_z { "_3D.dxf" } else { "_2D.dxf" });
    input.with_file_name(name)
}

pub fn output_exists(output: &str) -> bool {
    Path::new(output).exists() || PathBuf::from(format!("{output}.conversion.json")).exists()
}

fn tolerance(value: &str) -> Result<f64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| {
            ConversionError::message(
                "Enter positive finite plan and elevation tolerances in drawing units.",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const HEADER: &str = "// archive_version \"15.01.08.60\"\n";
    const ROAD: &str = "model Survey string super { breakline line data_3d { 500000 6200000 12 500001.25 6200002.5 16 } }\n";
    const RIVER: &str = "model River string super { breakline line data_3d { 1 2 12 3 4 14 } }\n";

    fn settle(state: &mut State) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while state.waiting() {
            assert!(
                Instant::now() < deadline,
                "worker did not finish: {}",
                state.status
            );
            std::thread::sleep(Duration::from_millis(10));
            state.tick();
        }
    }

    #[test]
    fn rust_13_load_cancellation_and_invalid_input() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("測量 town.12da");
        fs::write(&input, format!("{HEADER}{}", ROAD.repeat(40_000))).unwrap();
        let mut state = State::default();
        state.open(input);
        assert!(state.busy());
        assert!(!state.can_open());
        assert!(state.export(false).is_err());
        state.cancel();
        settle(&mut state);
        assert_eq!(state.status, "Cancelled.");
        assert!(state.scan.is_none());
        state.open(folder.path().join("missing.12da"));
        settle(&mut state);
        assert!(state.status.starts_with("Failed"));
        assert!(state.scan.is_none() && !state.can_export());
    }

    #[test]
    fn rust_14_rapid_selection_and_reload_preserve_only_latest_preview() {
        let folder = tempfile::tempdir().unwrap();
        let first = folder.path().join("first.12da");
        let second = folder.path().join("second.12da");
        fs::write(&first, format!("{HEADER}{ROAD}{RIVER}")).unwrap();
        fs::write(&second, format!("{HEADER}{ROAD}")).unwrap();
        let mut state = State::default();
        state.open(first);
        settle(&mut state);
        assert_eq!(state.selected.len(), 2);
        state.search = "river".into();
        assert_eq!(
            state
                .visible_models()
                .map(|model| &model.name)
                .collect::<Vec<_>>(),
            ["River"]
        );
        assert!(state.selected.contains("Survey"));
        state.toggle("River");
        state.select_all(true);
        state.toggle("Survey");
        settle(&mut state);
        assert_eq!(state.preview.len(), 1);
        assert_eq!(state.preview[0].model, "River");
        assert_eq!(state.scan.as_ref().unwrap().models.len(), 2);
        state.units = Some("metres");
        state.select_all(true);
        state.pending.as_mut().unwrap().ready = Instant::now();
        state.tick();
        assert!(state.busy());
        state.open(second);
        settle(&mut state);
        assert_eq!(state.selected, BTreeSet::from(["Survey".into()]));
        assert_eq!(state.preview[0].model, "Survey");
        assert!(state.units.is_none());
    }

    #[test]
    fn rust_15_exports_through_shared_service_and_preserves_declined_overwrite() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("input.12da");
        let output = folder.path().join("input_3D.dxf");
        fs::write(&input, format!("{HEADER}{ROAD}")).unwrap();
        let mut state = State::default();
        state.open(input);
        settle(&mut state);
        assert!(!state.can_export());
        state.keep_z = true;
        state.mode_changed();
        state.units = Some("metres");
        assert_eq!(Path::new(&state.output), output);
        assert!(state.can_export());
        state.plan_tolerance = "invalid".into();
        assert!(state.export(false).is_err());
        state.plan_tolerance = "0.01".into();
        state.export(false).unwrap();
        assert!(!state.can_export() && state.export(false).is_err());
        settle(&mut state);
        let report = state.result.as_ref().unwrap();
        assert_eq!(report.options.geometry_mode, "keep-z");
        assert_eq!(report.emitted_entities, Some(1));
        assert!(report.report_path.as_ref().unwrap().is_file());
        az12_converter::dxf::validate_dxf(&output, 1, true, None).unwrap();
        let bytes = fs::read(&output).unwrap();
        assert!(output_exists(&state.output));
        state.export(false).unwrap();
        settle(&mut state);
        assert!(state.status.contains("already exists"));
        assert_eq!(fs::read(&output).unwrap(), bytes);
    }

    #[test]
    #[ignore = "Windows release qualification stress run"]
    fn rust_19_repeats_load_export_and_cancel_in_one_desktop_process() {
        let folder = tempfile::tempdir().unwrap();
        let dense = folder.path().join("dense survey.12da");
        let compact = folder.path().join("export survey.12da");
        let line = "model \"Dense\" string super { breakline line data_2d { 0 0 1 2 3 4 } }\n";
        fs::write(&dense, format!("{HEADER}{}", line.repeat(10_100))).unwrap();
        fs::write(&compact, format!("{HEADER}{ROAD}")).unwrap();
        let mut state = State::default();
        let mut largest_cancel = Duration::ZERO;
        #[cfg(windows)]
        let mut private_bytes = Vec::new();
        for iteration in 0..10 {
            state.open(dense.clone());
            settle(&mut state);
            assert!(state.preview_simplified);
            assert!(
                state
                    .preview
                    .iter()
                    .map(|record| record.points.len())
                    .sum::<usize>()
                    <= PREVIEW_LIMIT
            );
            state.open(compact.clone());
            settle(&mut state);
            state.units = Some("metres");
            state.export(iteration > 0).unwrap();
            settle(&mut state);
            assert!(state.result.as_ref().is_some_and(|result| result.published));
            state.open(dense.clone());
            let start = Instant::now();
            state.cancel();
            settle(&mut state);
            let elapsed = start.elapsed();
            largest_cancel = largest_cancel.max(elapsed);
            assert!(
                elapsed < Duration::from_secs(2),
                "cancel cleanup in cycle {iteration} took {elapsed:?}"
            );
            assert_eq!(state.status, "Cancelled.");
            assert!(!state.waiting());
            assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 4);
            #[cfg(windows)]
            private_bytes.push(current_process_private_bytes());
        }
        println!(
            "10 completed cycles; longest cancellation through cleanup was {largest_cancel:?}"
        );
        #[cfg(windows)]
        println!("Private bytes after cleanup per cycle: {private_bytes:?}");
    }

    #[cfg(windows)]
    fn current_process_private_bytes() -> usize {
        use windows_sys::Win32::System::ProcessStatus::{
            K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        counters.cb = size;
        // The extended structure begins with PROCESS_MEMORY_COUNTERS, and the API receives its size.
        let result = unsafe {
            K32GetProcessMemoryInfo(GetCurrentProcess(), (&raw mut counters).cast(), size)
        };
        assert_ne!(result, 0, "Windows process memory query failed");
        counters.PrivateUsage
    }
}
