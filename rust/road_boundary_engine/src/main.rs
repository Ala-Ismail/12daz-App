use road_boundary_engine::{benchmark, corridor, detect, evidence, load_json};
use serde::Serialize;
use std::{
    env, fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const USAGE: &str = "usage: road-boundary-engine (--input FILE | --benchmark FILE | --corridor-pilot FILE) --output FILE [--dxf FILE]";

fn run() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--help" {
        println!("{USAGE}");
        return Ok(());
    }
    let [input_flag, input, output_flag, output, rest @ ..] = args.as_slice() else {
        return Err(USAGE.into());
    };
    if output_flag != "--output" {
        return Err(USAGE.into());
    }
    let dxf = match rest {
        [] => None,
        [flag, path] if flag == "--dxf" => Some(PathBuf::from(path)),
        _ => return Err(USAGE.into()),
    };
    let input = PathBuf::from(input);
    let output = PathBuf::from(output);
    if input_flag == "--input" {
        let result = detect(load_json(&input)?)?;
        write_outputs(&result, &output, dxf.as_deref(), |path| {
            evidence::detection(&result, path)
        })?;
    } else if input_flag == "--benchmark" {
        let report = benchmark::score(load_json(&input)?, parent(&input))?;
        write_outputs(&report, &output, dxf.as_deref(), |path| {
            evidence::benchmark(&report, path)
        })?;
    } else if input_flag == "--corridor-pilot" {
        let report = corridor::score(load_json(&input)?)?;
        write_outputs(&report, &output, dxf.as_deref(), |path| {
            evidence::corridor(&report, path)
        })?;
    } else {
        return Err(USAGE.into());
    }
    Ok(())
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

fn write_outputs(
    value: &impl Serialize,
    output: &Path,
    dxf: Option<&Path>,
    write_dxf: impl Fn(&Path) -> Result<()>,
) -> Result<()> {
    let mut temporary = tempfile::NamedTempFile::new_in(parent(output))?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        serde_json::to_writer_pretty(&mut writer, value)?;
        writer.flush()?;
    }
    let drawing = dxf
        .map(|target| -> Result<_> {
            let path = tempfile::NamedTempFile::new_in(parent(target))?.into_temp_path();
            write_dxf(&path)?;
            Ok((path, target))
        })
        .transpose()?;
    temporary.persist_noclobber(output)?;
    if let Some((drawing, target)) = drawing
        && let Err(error) = drawing.persist_noclobber(target)
    {
        fs::remove_file(output)?;
        return Err(error.into());
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
