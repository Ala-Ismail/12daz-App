use std::{fs, path::Path};

use acadrust::{DxfReader, EntityType};
use az12_converter::{
    archive::Limits,
    service::{Job, Options, Report, run_job},
};

fn export(input: &Path, output: &Path, units: &str, geometry_mode: &str) -> Report {
    run_job(
        input,
        &Options {
            units: Some(units.into()),
            geometry_mode: geometry_mode.into(),
            ..Default::default()
        },
        Job {
            output: Some(output),
            report: None,
            limits: Limits::default(),
            cancel: None,
            progress: &mut |_, _| {},
            preview_limit: 0,
        },
    )
    .unwrap()
}

fn assert_survey(output: &Path, scale: f64, units: i16, keep_z: bool) {
    let drawing = DxfReader::from_file(output).unwrap().read().unwrap();
    assert_eq!(drawing.header.insertion_units, units);
    let mut entities = drawing.model_space_entities();
    let expected_xy = [
        (321000.0 * scale, 5812000.0 * scale),
        (321004.0 * scale, 5812003.0 * scale),
    ];
    match (entities.next(), keep_z) {
        (Some(EntityType::LwPolyline(line)), false) => {
            let xy: Vec<_> = line
                .vertices
                .iter()
                .map(|vertex| (vertex.location.x, vertex.location.y))
                .collect();
            assert_eq!(xy, expected_xy);
            assert_eq!(line.elevation, 0.0);
        }
        (Some(EntityType::Polyline3D(line)), true) => {
            let xyz: Vec<_> = line
                .vertices
                .iter()
                .map(|vertex| (vertex.position.x, vertex.position.y, vertex.position.z))
                .collect();
            assert_eq!(
                xyz,
                [
                    (expected_xy[0].0, expected_xy[0].1, 12.0 * scale),
                    (expected_xy[1].0, expected_xy[1].1, 16.0 * scale),
                ],
            );
        }
        _ => panic!("unexpected survey line type"),
    }
    assert!(entities.next().is_none());
}

#[test]
fn map_02_and_14_conversion_preserves_mga_coordinates_and_declares_units() {
    let folder = tempfile::tempdir().unwrap();
    for (source_units, scale) in [("metres", 1.0), ("millimetres", 1000.0)] {
        let input = folder.path().join(format!("{source_units}.12da"));
        let source = format!(
            "// archive_version \"15.1\"\nmodel Survey string super {{ breakline line data_3d {{ {} {} {} {} {} {} }} }}\n",
            321000.0 * scale,
            5812000.0 * scale,
            12.0 * scale,
            321004.0 * scale,
            5812003.0 * scale,
            16.0 * scale,
        );
        fs::write(&input, &source).unwrap();
        for (dxf_units, insertion_units) in [("metres", 6), ("millimetres", 4)] {
            for geometry_mode in ["flatten-2d", "keep-z"] {
                let output = folder
                    .path()
                    .join(format!("{source_units}-{dxf_units}-{geometry_mode}.dxf"));
                export(&input, &output, dxf_units, geometry_mode);
                assert_survey(&output, scale, insertion_units, geometry_mode == "keep-z");
            }
        }
        assert_eq!(fs::read_to_string(&input).unwrap(), source);
    }
}

#[cfg(feature = "desktop")]
#[test]
fn map_14_saving_a_control_point_registration_does_not_affect_conversion() {
    use az12_converter::map::{Coordinate, CrsId, MapRegistration, SourceUnit, sidecar};

    let folder = tempfile::tempdir().unwrap();
    let input = folder.path().join("survey.12da");
    let source = b"// archive_version \"15.1\"\nmodel Survey string super { breakline line data_3d { 321000 5812000 12 321004 5812003 16 } }";
    fs::write(&input, source).unwrap();
    let original = export(
        &input,
        &folder.path().join("before.dxf"),
        "metres",
        "keep-z",
    );
    let point = |x, y| Coordinate::new(x, y).unwrap();
    let registration = MapRegistration::align(
        CrsId::Gda2020MgaZone55,
        SourceUnit::Metres,
        (point(321000.0, 5812000.0), point(321100.0, 5812100.0)),
        None,
    )
    .unwrap();
    sidecar::save(&input, &original.input_sha256, registration, true).unwrap();
    let output = folder.path().join("after.dxf");
    let saved = export(&input, &output, "metres", "keep-z");
    assert_survey(&output, 1.0, 6, true);
    assert_eq!(original.input_sha256, saved.input_sha256);
    assert_eq!(
        serde_json::to_value(original.diagnostics).unwrap(),
        serde_json::to_value(saved.diagnostics).unwrap(),
    );
    assert_eq!(fs::read(input).unwrap(), source);
}
