use std::{fs, path::Path};

use az12_converter::{
    archive::Limits,
    dxf::validate_dxf,
    service::{Job, Options, PREVIEW_LIMIT, Report, run_job},
};

const SURVEY: &str = concat!(
    "// archive_version \"15.1\"\n",
    "model Roads string super { breakline line data_3d { 0 0 10 10 0 12 } }\n",
    "model Roads string super { breakline line data_2d { 0 20 10 20 } }\n",
    "model Roads string future {}\n",
    "model Roads string super { breakline point attributes {} data_3d { 0 30 10 1 30 11 } }\n",
    "model Utilities string future {}\n",
);

fn run(input: &Path, options: &Options, output: Option<&Path>) -> Report {
    run_job(
        input,
        options,
        Job {
            output,
            report: None,
            limits: Limits::default(),
            cancel: None,
            progress: &mut |_, _| {},
            preview_limit: if output.is_some() { 0 } else { PREVIEW_LIMIT },
        },
    )
    .unwrap()
}

#[test]
fn skip_01_selection_and_geometry_mode_match_the_export() {
    let folder = tempfile::tempdir().unwrap();
    let input = folder.path().join("mixed.12da");
    fs::write(&input, SURVEY).unwrap();
    for (mode, skipped_lines, entities) in [("flatten-2d", &[4][..], 4), ("keep-z", &[3, 4][..], 3)]
    {
        let options = Options {
            units: Some("metres".into()),
            models: Some(vec!["Roads".into()]),
            geometry_mode: mode.into(),
            ..Default::default()
        };
        let preview = run(&input, &options, None);
        let output = folder.path().join(format!("{mode}.dxf"));
        let export = run(&input, &options, Some(&output));
        for report in [&preview, &export] {
            assert_eq!(report.source_counts.selected, 4);
            assert_eq!(report.source_counts.skipped, skipped_lines.len());
            assert_eq!(report.source_counts.approximated, 1);
            let skipped: Vec<_> = report
                .diagnostics
                .details
                .iter()
                .filter(|diagnostic| diagnostic.code == "UNSUPPORTED_OR_INVALID_GEOMETRY")
                .map(|diagnostic| {
                    assert_eq!(diagnostic.model.as_deref(), Some("Roads"));
                    let line = diagnostic.line.unwrap();
                    assert_eq!(diagnostic.source_id, Some(line - 1));
                    let reason = if line == 3 {
                        "Keep Z requires a known source elevation."
                    } else {
                        "Unsupported source type: future"
                    };
                    assert_eq!(diagnostic.message, reason);
                    line
                })
                .collect();
            assert_eq!(skipped, skipped_lines);
        }
        assert_eq!(export.emitted_entities, Some(entities));
        validate_dxf(&output, entities, mode == "keep-z", None).unwrap();
        assert_eq!(
            preview
                .skipped_elements
                .iter()
                .map(|item| item.line)
                .collect::<Vec<_>>(),
            skipped_lines,
        );
        let serialized = serde_json::to_value(&preview).unwrap();
        assert!(serialized.get("skipped_elements").is_none());
        assert!(export.skipped_elements.is_empty());
    }
}

#[test]
fn skip_04_retains_only_reliable_drawable_geometry() {
    let folder = tempfile::tempdir().unwrap();
    let input = folder.path().join("display.12da");
    let source = concat!(
        "// archive_version \"15.1\"\n",
        "model Survey string super { breakline line data_2d { 0 20 10 20 } }\n",
        "model Survey string super { breakline line data_3d { 5 30 10 10 30 11 } geometry_data { spiral {} } }\n",
        "model Survey string future { data_2d { 50 90 } }\n",
        "model Survey string super { breakline line data_2d { nan 20 10 20 } }\n",
    );
    fs::write(&input, source).unwrap();
    let options = Options {
        geometry_mode: "keep-z".into(),
        ..Default::default()
    };
    let report = run(&input, &options, None);
    let items = report.skipped_elements;
    assert_eq!(items.len(), 4);
    assert_eq!(items[0].paths, [vec![[0.0, 20.0], [10.0, 20.0]]]);
    assert_eq!(items[1].paths, [vec![[5.0, 30.0]]]);
    assert!(items[1].reason.contains("Unsupported spiral curve"));
    assert!(items[2].paths.is_empty());
    assert!(items[3].paths.is_empty());
    assert!(items.iter().all(|item| !item.drawing_limited));
}

#[test]
fn skip_06_point_and_detail_limits_preserve_the_total() {
    let folder = tempfile::tempdir().unwrap();
    let input = folder.path().join("dense.12da");
    let line = "model Survey string super { breakline line data_2d { 0 0 5 10 10 0 } }\n";
    let rejected = "model Survey string circle { xcentre 12 ycentre 23 radius 0 }\n";
    fs::write(
        &input,
        format!(
            "// archive_version \"15.1\"\n{}{}",
            line.repeat(30),
            rejected.repeat(700)
        ),
    )
    .unwrap();
    let report = run_job(
        &input,
        &Options::default(),
        Job {
            output: None,
            report: None,
            limits: Limits::default(),
            cancel: None,
            progress: &mut |_, _| {},
            preview_limit: 30,
        },
    )
    .unwrap();
    assert_eq!(report.source_counts.skipped, 700);
    assert_eq!(report.skipped_elements.len(), 500);
    let supported: usize = report.preview.iter().map(|item| item.points.len()).sum();
    let overlay: usize = report
        .skipped_elements
        .iter()
        .flat_map(|item| &item.paths)
        .map(Vec::len)
        .sum();
    assert!(supported > 0 && supported + overlay <= 30);
    assert!(
        report
            .skipped_elements
            .iter()
            .any(|item| item.drawing_limited)
    );
}

#[test]
fn skip_02_linear_arc_assumption_changes_skip_to_approximation() {
    let folder = tempfile::tempdir().unwrap();
    let input = folder.path().join("arc.12da");
    fs::write(
        &input,
        concat!(
            "// archive_version \"15.1\"\n",
            "model Roads string super { breakline line data_3d { 0 0 12 10 0 16 } radius_data { -10 } }\n",
        ),
    )
    .unwrap();
    for assume_grade in [false, true] {
        let report = run(
            &input,
            &Options {
                geometry_mode: "keep-z".into(),
                linear_arc_elevations: assume_grade,
                ..Default::default()
            },
            None,
        );
        assert_eq!(report.source_counts.skipped, usize::from(!assume_grade));
        assert_eq!(report.source_counts.approximated, usize::from(assume_grade));
        if assume_grade {
            assert_eq!(report.diagnostics.counts["ELEVATED_ARC_SAMPLED"], 1);
            assert!(!report.preview.is_empty());
        } else {
            assert_eq!(
                report.diagnostics.counts["UNSUPPORTED_OR_INVALID_GEOMETRY"],
                1
            );
            assert!(report.preview.is_empty());
        }
    }
}
