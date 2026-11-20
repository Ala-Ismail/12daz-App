//! Shared conversion jobs, bounded diagnostics, and publication.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use serde::Serialize;
use tempfile::{Builder, TempPath};
use unicode_casefold::UnicodeCaseFold;

use crate::archive::{Limits, Metadata, input_hash, open_input};
use crate::dxf::{DxfWriter, validate_dxf};
use crate::error::{ConversionError, Result, checkpoint};
use crate::geometry::{
    GeometryError, GeometryOptions, Point, convert_geometry, preview_points, repr, skipped_location,
};
use crate::parser::{SourceEntity, iter_entities};
use crate::preview::{PreviewCollector, PreviewRecord, SkippedElement};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PREVIEW_LIMIT: usize = 30_000;
const SKIPPED_DETAIL_LIMIT: usize = 500;

#[derive(Clone, Debug, Serialize)]
pub struct Options {
    pub units: Option<String>,
    pub models: Option<Vec<String>>,
    pub overwrite: bool,
    pub encoding: Option<String>,
    pub geometry_mode: String,
    pub plan_tolerance: f64,
    pub elevation_tolerance: f64,
    pub linear_arc_elevations: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            units: None,
            models: None,
            overwrite: false,
            encoding: None,
            geometry_mode: "flatten-2d".into(),
            plan_tolerance: 0.01,
            elevation_tolerance: 0.01,
            linear_arc_elevations: false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Default, Serialize)]
pub struct Diagnostics {
    pub counts: BTreeMap<String, usize>,
    pub total: usize,
    pub details: Vec<Diagnostic>,
    pub details_truncated: bool,
    #[serde(skip)]
    detail_counts: HashMap<String, usize>,
}

impl Diagnostics {
    fn add(&mut self, code: &str, message: &str, source: Option<&SourceEntity>) {
        *self.counts.entry(code.into()).or_default() += 1;
        self.total += 1;
        let detail_count = self.detail_counts.entry(code.into()).or_default();
        if self.details.len() < 500 && *detail_count < 50 {
            *detail_count += 1;
            self.details.push(Diagnostic {
                code: code.into(),
                message: message.into(),
                source_id: source.map(|source| source.ordinal),
                model: source.map(|source| source.model.clone()),
                line: source.map(|source| source.line),
            });
        } else {
            self.details_truncated = true;
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct SourceCounts {
    pub selected: usize,
    pub exact: usize,
    pub approximated: usize,
    pub skipped: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct Model {
    pub name: String,
    pub count: usize,
    pub convertible: usize,
    pub types: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub job_id: String,
    pub application_version: &'static str,
    pub input: PathBuf,
    pub input_sha256: String,
    pub encoding: String,
    pub archive_version: Option<String>,
    pub options: Options,
    pub models: Vec<Model>,
    pub source_counts: SourceCounts,
    pub bounds: Option<[f64; 4]>,
    pub bounds_note: &'static str,
    #[serde(skip)]
    pub preview: Vec<PreviewRecord>,
    #[serde(skip)]
    pub skipped_elements: Vec<SkippedElement>,
    pub preview_simplified: bool,
    pub preview_omitted_models: Vec<String>,
    pub preview_note: &'static str,
    pub units: Option<String>,
    pub emitted_entities: Option<usize>,
    pub layer_mapping: BTreeMap<String, String>,
    pub diagnostics: Diagnostics,
    pub published: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<PathBuf>,
    pub elapsed_seconds: f64,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_path: Option<PathBuf>,
}

pub type Progress<'a> = dyn FnMut(&str, u64) + 'a;

pub struct Job<'a> {
    pub output: Option<&'a Path>,
    pub report: Option<&'a Path>,
    pub limits: Limits,
    pub cancel: Option<&'a AtomicBool>,
    pub progress: &'a mut Progress<'a>,
    pub preview_limit: usize,
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn resolved(path: &Path) -> io::Result<PathBuf> {
    let canonical = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let name = path.file_name().ok_or(error)?;
            resolved(parent(path))?.join(name)
        }
        Err(error) => return Err(error),
    };
    #[cfg(windows)]
    {
        // Strip Win32's extended prefix for paths shown to users and written in reports.
        use std::path::{Component, Prefix};
        let mut parts = canonical.components();
        if let Some(Component::Prefix(prefix)) = parts.next() {
            let prefix = match prefix.kind() {
                Prefix::VerbatimDisk(drive) => {
                    Some(PathBuf::from(format!("{}:\\", char::from(drive))))
                }
                Prefix::VerbatimUNC(server, share) => {
                    Some(PathBuf::from(r"\\").join(server).join(share))
                }
                _ => None,
            };
            if let Some(mut prefix) = prefix {
                for part in parts.filter(|part| *part != Component::RootDir) {
                    prefix.push(part);
                }
                return Ok(prefix);
            }
        }
    }
    Ok(canonical)
}

fn same_path(a: &Path, b: &Path) -> io::Result<bool> {
    let (a, b) = (resolved(a)?, resolved(b)?);
    if a == b {
        return Ok(true);
    }
    #[cfg(windows)]
    if a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
    {
        return Ok(true);
    }
    if a.exists() && b.exists() {
        return same_file::is_same_file(a, b);
    }
    Ok(false)
}

fn output_error(error: impl std::fmt::Display) -> ConversionError {
    ConversionError::message(format!("Cannot write output: {error}"))
}

fn check_destinations(input: &Path, output: &Path, report: &Path, overwrite: bool) -> Result<()> {
    if same_path(input, output).map_err(output_error)? {
        return Err(ConversionError::message(
            "Output must not replace the input file.",
        ));
    }
    if !output
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("dxf"))
    {
        return Err(ConversionError::message(
            "Output filename must end in .dxf.",
        ));
    }
    if !parent(output).is_dir() {
        return Err(ConversionError::message("Output directory does not exist."));
    }
    if same_path(input, report).map_err(output_error)?
        || same_path(output, report).map_err(output_error)?
    {
        return Err(ConversionError::message(
            "Report path must differ from input and DXF output.",
        ));
    }
    if !overwrite && (output.exists() || report.exists()) {
        return Err(ConversionError::message(
            "DXF or report already exists; choose another name or explicitly enable overwrite.",
        ));
    }
    Ok(())
}

fn publish(temp: TempPath, output: &Path, overwrite: bool) -> io::Result<()> {
    if overwrite {
        temp.persist(output)?;
    } else {
        temp.persist_noclobber(output)?;
    }
    Ok(())
}

fn validated_publish(
    temp: TempPath,
    output: &Path,
    overwrite: bool,
    cancel: Option<&AtomicBool>,
    write_and_validate: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    checkpoint(cancel)?;
    write_and_validate(&temp)?;
    checkpoint(cancel)?;
    publish(temp, output, overwrite).map_err(output_error)
}

fn atomic_json(path: &Path, report: &Report, overwrite: bool) -> io::Result<()> {
    let mut temp = Builder::new()
        .prefix(".12daz-")
        .suffix(".tmp")
        .tempfile_in(parent(path))?;
    serde_json::to_writer_pretty(temp.as_file_mut(), report)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    publish(temp.into_temp_path(), path, overwrite)
}

fn report_path(output: &Path) -> PathBuf {
    let mut path = output.as_os_str().to_owned();
    path.push(".conversion.json");
    path.into()
}

fn geometry_options(options: &Options) -> Result<GeometryOptions> {
    let keep_z = match options.geometry_mode.as_str() {
        "keep-z" => true,
        "flatten-2d" => false,
        _ => return Err(ConversionError::message("Unknown output geometry mode.")),
    };
    for (label, value) in [
        ("Plan", options.plan_tolerance),
        ("Elevation", options.elevation_tolerance),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(ConversionError::message(format!(
                "{label} tolerance must be a positive finite number in drawing units."
            )));
        }
    }
    Ok(GeometryOptions {
        keep_z,
        plan_tolerance: options.plan_tolerance,
        elevation_tolerance: options.elevation_tolerance,
        linear_arc_elevations: options.linear_arc_elevations,
    })
}

struct Accumulation<'a> {
    models: Vec<Model>,
    model_indices: HashMap<String, usize>,
    counts: SourceCounts,
    diagnostics: Diagnostics,
    bounds: Option<[f64; 4]>,
    preview: PreviewCollector<'a>,
    skipped_elements: Vec<SkippedElement>,
    skipped_point_budget: Option<usize>,
    writer: Option<DxfWriter>,
}

impl Accumulation<'_> {
    fn model_index(&mut self, key: String, source: &SourceEntity) -> usize {
        let index = *self.model_indices.entry(key).or_insert_with(|| {
            let index = self.models.len();
            self.models.push(Model {
                name: source.model.clone(),
                ..Model::default()
            });
            index
        });
        let model = &mut self.models[index];
        model.count += 1;
        *model.types.entry(source.kind.clone()).or_default() += 1;
        index
    }

    fn process(
        &mut self,
        mut source: SourceEntity,
        index: usize,
        geometry: GeometryOptions,
        cancel: Option<&AtomicBool>,
    ) -> Result<()> {
        source.model.clone_from(&self.models[index].name);
        self.counts.selected += 1;
        let before = self.diagnostics.total;
        let mut warn =
            |code: &str, message: &str| self.diagnostics.add(code, message, Some(&source));
        let primitives = match convert_geometry(&source, geometry, &mut warn, cancel) {
            Ok(primitives) => primitives,
            Err(GeometryError::Unsupported(reason)) => {
                self.counts.skipped += 1;
                warn("UNSUPPORTED_OR_INVALID_GEOMETRY", &reason);
                if let Some(budget) = &mut self.skipped_point_budget
                    && self.skipped_elements.len() < SKIPPED_DETAIL_LIMIT
                {
                    let (paths, drawing_limited) =
                        skipped_paths(&source, geometry, *budget, cancel)?;
                    *budget -= paths.iter().map(Vec::len).sum::<usize>();
                    self.skipped_elements.push(SkippedElement {
                        source_id: source.ordinal,
                        model: source.model,
                        kind: source.kind,
                        line: source.line,
                        reason,
                        paths,
                        drawing_limited,
                    });
                }
                return Ok(());
            }
            Err(GeometryError::Input(error)) => return Err(error),
        };
        self.models[index].convertible += 1;
        for primitive in primitives {
            if let Some(writer) = &mut self.writer {
                writer.write(&primitive, &source.model, &mut warn)?;
            }
            let points = preview_points(&primitive).map_err(|error| match error {
                GeometryError::Unsupported(message) => ConversionError::message(message),
                GeometryError::Input(error) => error,
            })?;
            for &[x, y] in &points {
                let bounds = self.bounds.get_or_insert([x, y, x, y]);
                *bounds = [
                    bounds[0].min(x),
                    bounds[1].min(y),
                    bounds[2].max(x),
                    bounds[3].max(y),
                ];
            }
            self.preview.add(&source.model, &primitive, points)?;
        }
        if self.diagnostics.total > before {
            self.counts.approximated += 1;
        } else {
            self.counts.exact += 1;
        }
        Ok(())
    }
}

fn skipped_paths(
    source: &SourceEntity,
    geometry: GeometryOptions,
    budget: usize,
    cancel: Option<&AtomicBool>,
) -> Result<(Vec<Vec<Point>>, bool)> {
    if budget == 0 {
        return Ok((Vec::new(), true));
    }
    let mut limited = false;
    if geometry.keep_z {
        let plan_options = GeometryOptions {
            keep_z: false,
            ..geometry
        };
        match convert_geometry(source, plan_options, &mut |_, _| {}, cancel) {
            Ok(parts) => {
                let mut paths = Vec::new();
                let mut remaining = budget;
                for part in parts {
                    checkpoint(cancel)?;
                    if part.points.len() > remaining {
                        limited = true;
                        break;
                    }
                    let Ok(points) = preview_points(&part) else {
                        continue;
                    };
                    if !points
                        .iter()
                        .flatten()
                        .all(|coordinate| coordinate.is_finite())
                    {
                        continue;
                    }
                    if points.len() > remaining {
                        limited = true;
                        break;
                    }
                    remaining -= points.len();
                    if part.kind == crate::geometry::Kind::Point {
                        paths.extend(points.into_iter().map(|point| vec![point]));
                    } else if !points.is_empty() {
                        paths.push(points);
                    }
                }
                if !limited && !paths.is_empty() {
                    return Ok((paths, false));
                }
            }
            Err(GeometryError::Unsupported(_)) => {}
            Err(GeometryError::Input(_)) => checkpoint(cancel)?,
        }
    }
    Ok((
        skipped_location(source).map_or_else(Vec::new, |point| vec![vec![point]]),
        limited,
    ))
}

pub fn run_job(path: &Path, options: &Options, job: Job<'_>) -> Result<Report> {
    let inspect_skipped = job.output.is_none() && job.preview_limit > 0;
    let geometry = geometry_options(options)?;
    let start = Instant::now();
    let default_report = job.output.filter(|_| job.report.is_none()).map(report_path);
    let report_dest = job.report.or(default_report.as_deref());
    if let Some(output) = job.output {
        check_destinations(
            path,
            output,
            report_dest.expect("every DXF has a report destination"),
            options.overwrite,
        )?;
    } else if let Some(report) = report_dest
        && same_path(path, report).map_err(output_error)?
    {
        return Err(ConversionError::message(
            "Report must not replace the input file.",
        ));
    }
    let writer = job
        .output
        .map(|_| DxfWriter::new(options.units.as_deref()))
        .transpose()?;
    let digest = input_hash(path, job.cancel)?;
    let mut accumulation = Accumulation {
        models: Vec::new(),
        model_indices: HashMap::new(),
        counts: SourceCounts::default(),
        diagnostics: Diagnostics::default(),
        bounds: None,
        preview: PreviewCollector::new(job.preview_limit, job.cancel),
        skipped_elements: Vec::new(),
        skipped_point_budget: inspect_skipped.then_some(job.preview_limit / 5),
        writer,
    };
    let stage = if job.output.is_some() {
        "Converting"
    } else {
        "Inspecting"
    };
    let progress = RefCell::new(job.progress);
    let selected = options.models.as_ref().map(|names| {
        names
            .iter()
            .map(|name| name.case_fold().collect::<String>())
            .collect::<std::collections::HashSet<_>>()
    });
    let metadata: Metadata = open_input(
        path,
        job.limits,
        options.encoding.as_deref(),
        job.cancel,
        &mut |stage, amount| progress.borrow_mut()(stage, amount),
        |mut archive| {
            let mut entities = iter_entities(&mut archive, job.limits, job.cancel);
            while let Some(source) = entities.next_entity(&mut |key, _, line| {
                accumulation.diagnostics.add(
                    "UNKNOWN_COMMAND",
                    &format!(
                        "Unrecognized top-level command {} at line {line}.",
                        repr(key)
                    ),
                    None,
                );
            })? {
                let key = source.model.case_fold().collect::<String>();
                let index = accumulation.model_index(key.clone(), &source);
                if selected.as_ref().is_some_and(|names| !names.contains(&key)) {
                    continue;
                }
                let skipped = accumulation.counts.skipped;
                accumulation.process(source, index, geometry, job.cancel)?;
                if accumulation.counts.skipped == skipped {
                    progress.borrow_mut()(stage, accumulation.counts.selected as u64);
                }
            }
            Ok(archive.metadata)
        },
    )?;
    let Accumulation {
        models,
        counts,
        mut diagnostics,
        bounds,
        preview,
        mut skipped_elements,
        writer,
        ..
    } = accumulation;
    if metadata.extra_members > 0 {
        diagnostics.add(
            "EXTRA_ARCHIVE_MEMBERS",
            &format!(
                "Ignored {} non-payload archive entries.",
                metadata.extra_members
            ),
            None,
        );
    }
    if counts.selected == 0 {
        return Err(ConversionError::message(
            "Input contains no objects in the selected models.",
        ));
    }
    match metadata.archive_version.as_deref() {
        None => diagnostics.add("ARCHIVE_VERSION_UNVERIFIED", "No archive_version comment; compatibility is determined by recognized syntax only.", None),
        Some(version) if !version.starts_with("15.") => diagnostics.add("ARCHIVE_VERSION_UNVERIFIED", "Archive version is outside the V15 grammar baseline; recognized syntax only is converted.", None),
        _ => {}
    }
    if input_hash(path, job.cancel)? != digest {
        return Err(ConversionError::message(
            "Input changed during the job; output was not published.",
        ));
    }
    let preview = preview.finish()?;
    let displayed = preview
        .records
        .iter()
        .map(|record| record.points.len())
        .sum::<usize>();
    let mut remaining = job.preview_limit.saturating_sub(displayed);
    for element in &mut skipped_elements {
        let points = element.paths.iter().map(Vec::len).sum::<usize>();
        if points > remaining {
            element.paths.clear();
            element.drawing_limited = true;
        } else {
            remaining -= points;
        }
    }
    let mut result = Report {
        schema_version: 1,
        job_id: uuid::Uuid::new_v4().to_string(),
        application_version: VERSION,
        input: resolved(path).map_err(output_error)?,
        input_sha256: digest,
        encoding: metadata.encoding,
        archive_version: metadata.archive_version,
        options: options.clone(),
        models,
        source_counts: counts,
        bounds,
        bounds_note: "Preview bounds; curved extents sampled for display.",
        preview: preview.records,
        skipped_elements,
        preview_simplified: preview.limited,
        preview_omitted_models: preview.omitted_models,
        preview_note: "Preview prioritizes path shape and spatial coverage within the shared display allowance. DXF export uses independent geometry.",
        units: options.units.clone(),
        emitted_entities: writer.as_ref().map(DxfWriter::count),
        layer_mapping: writer
            .as_ref()
            .map(|writer| writer.layers().clone())
            .unwrap_or_default(),
        diagnostics,
        published: false,
        output: None,
        elapsed_seconds: 0.0,
        status: "complete",
        report_path: None,
    };
    if let (Some(output), Some(writer)) = (job.output, writer) {
        if writer.count() == 0 {
            return Err(ConversionError::message(
                "Selection contains no convertible geometry; no DXF was published.",
            ));
        }
        let temp = Builder::new()
            .prefix(".12daz-")
            .suffix(".dxf")
            .tempfile_in(parent(output))
            .map_err(output_error)?
            .into_temp_path();
        progress.borrow_mut()("Writing and validating DXF", writer.count() as u64);
        validated_publish(temp, output, options.overwrite, job.cancel, |temp| {
            writer.save(temp)?;
            validate_dxf(temp, writer.count(), geometry.keep_z, job.cancel)
        })?;
        result.published = true;
        result.output = Some(resolved(output).map_err(output_error)?);
    }
    result.elapsed_seconds = (start.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;
    if result.diagnostics.total > 0 {
        result.status = "complete_with_warnings";
    }
    if let Some(report) = report_dest {
        match atomic_json(report, &result, options.overwrite) {
            Ok(()) => result.report_path = Some(resolved(report).map_err(output_error)?),
            Err(error) if !result.published => {
                return Err(ConversionError::message(format!(
                    "Cannot save inspect report: {error}"
                )));
            }
            Err(error) => {
                result.diagnostics.add(
                    "REPORT_SAVE_FAILED",
                    &format!("DXF was published, but its report could not be saved: {error}"),
                    None,
                );
                result.status = "complete_with_warnings";
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::sync::{Barrier, atomic::Ordering};

    use super::*;

    const HEADER: &str = "// archive_version \"15.01.08.60\"\n";
    const ROAD: &str = "model Road string super { breakline line data_3d { 0 0 12 10 0 16 } }";

    fn job<'a>(
        output: Option<&'a Path>,
        report: Option<&'a Path>,
        cancel: Option<&'a AtomicBool>,
        progress: &'a mut Progress<'a>,
    ) -> Job<'a> {
        Job {
            output,
            report,
            limits: Limits::default(),
            cancel,
            progress,
            preview_limit: PREVIEW_LIMIT,
        }
    }

    fn options() -> Options {
        Options {
            units: Some("metres".into()),
            ..Options::default()
        }
    }

    #[test]
    fn rust_09_cancellation_and_source_mutation_preserve_destinations() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("test.12da");
        let output = folder.path().join("existing.dxf");
        let original = format!("{HEADER}{ROAD}");
        fs::write(&input, &original).unwrap();
        fs::write(&output, b"original drawing").unwrap();
        let options = Options {
            overwrite: true,
            ..options()
        };
        let cancel = AtomicBool::new(false);
        let result = run_job(
            &input,
            &options,
            job(Some(&output), None, Some(&cancel), &mut |stage, _| {
                if stage == "Writing and validating DXF" {
                    cancel.store(true, Ordering::Relaxed);
                }
            }),
        );
        assert!(matches!(result, Err(ConversionError::Cancelled)));
        assert_eq!(fs::read_to_string(&input).unwrap(), original);
        assert_eq!(fs::read(&output).unwrap(), b"original drawing");

        let result = run_job(
            &input,
            &options,
            job(Some(&output), None, None, &mut |stage, _| {
                if stage == "Converting" {
                    fs::write(&input, format!("{original}\n// changed")).unwrap();
                }
            }),
        );
        assert!(result.unwrap_err().to_string().contains("Input changed"));
        assert_eq!(fs::read(&output).unwrap(), b"original drawing");
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 2);

        let alias = folder.path().join("input-alias.dxf");
        fs::hard_link(&input, &alias).unwrap();
        assert!(
            run_job(
                &input,
                &options,
                job(Some(&alias), None, None, &mut |_, _| {})
            )
            .unwrap_err()
            .to_string()
            .contains("must not replace")
        );
    }

    #[test]
    fn rust_09_competing_jobs_publish_only_once() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("test.12da");
        fs::write(&input, format!("{HEADER}{ROAD}")).unwrap();
        let output = folder.path().join("same.dxf");
        let barrier = Barrier::new(2);
        let run = || {
            run_job(
                &input,
                &options(),
                job(Some(&output), None, None, &mut |stage, _| {
                    if stage == "Writing and validating DXF" {
                        barrier.wait();
                    }
                }),
            )
        };
        let (first, second) = std::thread::scope(|scope| {
            let first = scope.spawn(run);
            let second = scope.spawn(run);
            (first.join().unwrap(), second.join().unwrap())
        });
        assert_ne!(first.is_ok(), second.is_ok());
        validate_dxf(&output, 1, false, None).unwrap();
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 3);
    }

    #[test]
    fn rust_09_distinguishes_report_failure_from_published_dxf() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("test.12da");
        fs::write(&input, format!("{HEADER}{ROAD}")).unwrap();
        let output = folder.path().join("drawing.dxf");
        let missing = folder.path().join("missing").join("report.json");
        let result = run_job(
            &input,
            &options(),
            job(None, Some(&missing), None, &mut |_, _| {}),
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Cannot save inspect report")
        );
        let result = run_job(
            &input,
            &options(),
            job(Some(&output), Some(&missing), None, &mut |_, _| {}),
        )
        .unwrap();
        assert!(result.published);
        assert_eq!(result.status, "complete_with_warnings");
        assert_eq!(result.diagnostics.counts["REPORT_SAVE_FAILED"], 1);
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 2);
    }

    #[test]
    fn rust_09_write_and_validation_failures_preserve_output_and_clean_temp() {
        let folder = tempfile::tempdir().unwrap();
        let output = folder.path().join("existing.dxf");
        fs::write(&output, b"existing drawing").unwrap();
        for corrupt in [false, true] {
            let temp = Builder::new()
                .tempfile_in(folder.path())
                .unwrap()
                .into_temp_path();
            let result = validated_publish(temp, &output, true, None, |path| {
                if corrupt {
                    fs::write(path, b"invalid drawing").map_err(output_error)?;
                    return validate_dxf(path, 1, false, None);
                }
                Err(output_error(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected write failure",
                )))
            });
            assert!(result.is_err());
            assert_eq!(fs::read(&output).unwrap(), b"existing drawing");
            assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn rust_10_counts_objects_and_reports_bounded_diagnostics() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("test.12da");
        let future = "\nstring future {}".repeat(70);
        fs::write(&input, format!("{HEADER}{ROAD}\nstring super {{ breakline point attributes {{}} data_2d {{ 0 0 1 2 }} }}{future}")).unwrap();
        let output = folder.path().join("drawing.dxf");
        let result = run_job(
            &input,
            &options(),
            job(Some(&output), None, None, &mut |_, _| {}),
        )
        .unwrap();
        let counts = result.source_counts;
        assert_eq!(
            [
                counts.selected,
                counts.exact,
                counts.approximated,
                counts.skipped
            ],
            [72, 1, 1, 70]
        );
        validate_dxf(&output, 3, false, None).unwrap();
        assert_eq!(result.emitted_entities, Some(3));
        assert_eq!(
            result.diagnostics.counts["UNSUPPORTED_OR_INVALID_GEOMETRY"],
            70
        );
        assert!(result.diagnostics.details_truncated);
        assert_eq!(result.diagnostics.details.len(), 51);
        assert_eq!(result.diagnostics.total, 71);
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(result.report_path.unwrap()).unwrap()).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["input_sha256"], input_hash(&input, None).unwrap());
        assert!(json.get("preview").is_none());
    }
}
