use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

use az12_converter::archive::Limits;
use az12_converter::error::{ConversionError, Result};
use az12_converter::service::{Job, Options, Report, run_job};
use clap::{Args, Parser, Subcommand};

static CANCELLED: AtomicBool = AtomicBool::new(false);

#[derive(Parser)]
#[command(
    name = "12daz-converter",
    version,
    about = "Convert local 12daz/12da archives to DXF 2018.",
    args_override_self = true
)]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Inspect an archive and report model and geometry counts.
    Inspect(Common),
    /// Convert selected models to DXF 2018.
    Convert {
        #[command(flatten)]
        common: Common,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, value_parser = ["metres", "millimetres", "feet", "inches"], help = "Declare units without rescaling coordinates")]
        units: String,
    },
}

#[derive(Args)]
struct Common {
    input: PathBuf,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, help = "Explicit fallback encoding for files without a BOM")]
    encoding: Option<String>,
    #[arg(
        long = "model",
        help = "Select a model by name. Repeat to select several"
    )]
    models: Option<Vec<String>>,
    #[arg(long)]
    overwrite: bool,
    #[arg(long, default_value = "flatten-2d", value_parser = ["flatten-2d", "keep-z"])]
    geometry_mode: String,
    #[arg(long, default_value_t = 0.01, allow_negative_numbers = true)]
    plan_tolerance: f64,
    #[arg(long, default_value_t = 0.01, allow_negative_numbers = true)]
    elevation_tolerance: f64,
    #[arg(
        long,
        help = "In Keep Z, assume a linear grade on arcs between known endpoint elevations"
    )]
    linear_arc_elevations: bool,
    #[arg(long, default_value_t = 1024, value_parser = clap::value_parser!(u64).range(1..=(u64::MAX / (1024 * 1024))))]
    max_expanded_mib: u64,
}

fn run(action: Action) -> Result<Report> {
    let (common, output, units) = match action {
        Action::Inspect(common) => (common, None, None),
        Action::Convert {
            common,
            output,
            units,
        } => (common, Some(output), Some(units)),
    };
    let options = Options {
        units,
        models: common.models,
        overwrite: common.overwrite,
        encoding: common.encoding,
        geometry_mode: common.geometry_mode,
        plan_tolerance: common.plan_tolerance,
        elevation_tolerance: common.elevation_tolerance,
        linear_arc_elevations: common.linear_arc_elevations,
    };
    let mut progress = |_: &str, _: u64| {};
    let job = Job {
        output: output.as_deref(),
        report: common.report.as_deref(),
        cancel: Some(&CANCELLED),
        limits: Limits {
            expanded_bytes: common.max_expanded_mib * 1024 * 1024,
            ..Limits::default()
        },
        preview_limit: 0,
        progress: &mut progress,
    };
    run_job(&common.input, &options, job)
}

fn print_report(report: &Report) -> io::Result<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, report)?;
    writeln!(output)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Err(error) = ctrlc::set_handler(|| CANCELLED.store(true, Ordering::Relaxed)) {
        eprintln!("Error: Cannot register cancellation handler: {error}");
        return ExitCode::FAILURE;
    }
    match run(cli.action) {
        Ok(report) => {
            if let Err(error) = print_report(&report) {
                eprintln!("Error: Cannot write report to stdout: {error}");
                return ExitCode::FAILURE;
            }
            ExitCode::from(if report.diagnostics.total > 0 { 2 } else { 0 })
        }
        Err(ConversionError::Cancelled) => {
            eprintln!("Cancelled.");
            ExitCode::from(3)
        }
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}
