#![cfg(feature = "desktop")]

use std::{fs, io::Write, path::Path};

use az12_converter::archive::Limits;
use az12_converter::map::{
    Coordinate, CrsId, LonLat, MapRegistration, Projection, SourceUnit, sidecar,
};
use az12_converter::service::{Job, Options, Report, run_job};

const BRISBANE: [(CrsId, &str, [[f64; 2]; 2], [[f64; 2]; 2]); 2] = [
    (
        CrsId::Gda94Bcsg02,
        "EPSG:3113",
        [[52_777.351, 158_753.314], [46_741.770, 145_876.982]],
        [
            [153.02810603389162, -27.469787445262934],
            [152.96700602580594, -27.585987440053923],
        ],
    ),
    (
        CrsId::Gda2020Bcsg2020,
        "EPSG:20047",
        [[152_777.351, 258_753.314], [146_741.770, 245_876.982]],
        [[153.0281, -27.4698], [152.967, -27.586]],
    ),
];

fn point([x, y]: [f64; 2]) -> Coordinate {
    Coordinate::new(x, y).unwrap()
}

fn scan(path: &Path, options: &Options, preview_limit: usize) -> Report {
    run_job(
        path,
        options,
        Job {
            output: None,
            report: None,
            limits: Limits::default(),
            cancel: None,
            progress: &mut |_, _| {},
            preview_limit,
        },
    )
    .unwrap()
}

#[test]
fn automatic_pairing_uses_unselected_models_and_breaks_at_ambiguous_edges() {
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("road.12da");
    fs::write(
        &archive,
        r#"
model "Left kerb" string super { breakline line data_2d { 350000 5800000 350100 5800000 } }
model "Right kerb" string super { breakline line data_2d { 350100 5800008 350000 5800008 } }
model "Duplicate kerb" string super { breakline line data_2d { 350000 5800000 350100 5800000 } }
model "Side kerb" string super { breakline line data_2d { 350040 5800014 350060 5800014 } }
model "Left kerb" string super { breakline line data_2d { 350010 5800002 350025 5800002 350025 5800005 } }
string super { breakline line data_2d { 350025 5800005 350010 5800005 350010 5800002 } }
model Utilities string super { breakline line data_2d { 350000 5800004 350100 5800004 } }
"#,
    )
    .unwrap();
    let report = scan(&archive, &Options::default(), 1);
    let evidence = report.map_evidence.unwrap();
    let corridors = evidence.road_sides.pair(SourceUnit::Metres).unwrap();
    assert_eq!(corridors.len(), 2);
    assert!(corridors.iter().all(|corridor| corridor.models.len() == 2));
    assert!(
        corridors
            .iter()
            .flat_map(|corridor| &corridor.stations)
            .all(|[[x, lo], [_, hi]]| {
                !(350040.0..350060.0).contains(x) && *lo == 5800000.0 && *hi == 5800008.0
            })
    );
}

#[test]
#[ignore = "requires local MGA54/MGA56 samples and approved pilot drawings"]
fn reviewed_archives_pair_the_known_pilot_sections_without_saved_grids() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = root.join("outputs/road-boundary-engine");
    for (archive, pilot) in [
        (
            "12635625-GHD-00-00-MDL-CI-0001-ROAD DESIGN 12D.12daz",
            "mga54-ghd/mga54-ghd-corridor-input.json",
        ),
        (
            "1.7 SHRP-SMEC-WGI-RBT-M3D-COMBINED.12daz",
            "mga56-shrp-corridor-input.json",
        ),
    ] {
        let report = scan(&root.join("sample").join(archive), &Options::default(), 1);
        let corridors = report
            .map_evidence
            .unwrap()
            .road_sides
            .pair(SourceUnit::Metres)
            .unwrap();
        let pilot: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join(pilot)).unwrap()).unwrap();
        for section in pilot["sections"].as_array().unwrap() {
            let axis = section["axis"].as_u64().unwrap() as usize;
            let expected: Vec<[[f64; 2]; 2]> =
                serde_json::from_value(section["stations"].clone()).unwrap();
            let comparable = expected
                .iter()
                .filter(|station| {
                    corridors
                        .iter()
                        .filter(|c| c.axis == axis)
                        .flat_map(|c| &c.stations)
                        .any(|found| {
                            (found[0][axis] - station[0][axis]).abs() <= 0.26
                                && (0..2).all(|side| {
                                    (found[side][1 - axis] - station[side][1 - axis]).abs() <= 0.5
                                })
                        })
                })
                .count();
            eprintln!(
                "{}: {} of {} prior stations",
                section["name"],
                comparable,
                expected.len()
            );
            assert!(
                comparable * 2 > expected.len(),
                "insufficient overlap for {}",
                section["name"]
            );
        }
    }
}

#[test]
fn corridor_comparison_keeps_uncovered_zones_and_uses_strict_threshold() {
    use az12_converter::map::{
        corridor_match::{self, Coverage, Status},
        detection::Candidate,
    };
    let folder = tempfile::tempdir().unwrap();
    let archive = folder.path().join("road.12da");
    assert_eq!(
        corridor_match::companion_path(&archive),
        folder.path().join("road.12da.osm-roads.geojson")
    );
    fs::write(
        &archive,
        r#"
model "West kerb" string super { breakline line data_2d { 350000 5800000 350100 5800000 } }
model "East kerb" string super { breakline line data_2d { 350000 5800008 350100 5800008 } }
"#,
    )
    .unwrap();
    let correct = Candidate {
        crs: CrsId::Gda2020MgaZone55,
        units: SourceUnit::Metres,
    };
    let unchecked = Candidate {
        crs: CrsId::Gda2020MgaZone56,
        ..correct
    };
    let projection = Projection::new(correct.crs).unwrap();
    let convert = |p| {
        let p = projection.to_wgs84(point(p)).unwrap();
        [p.longitude(), p.latitude()]
    };
    let [x, y] = convert([350050., 5800004.]);
    let path = folder.path().join("roads.geojson");
    let evaluate = |points: [[f64; 2]; 2]| {
        fs::write(&path, serde_json::json!({"type":"FeatureCollection", "source_date":"2026-11-21",
            "coverage": [x-0.005,y-0.005,x+0.005,y+0.005], "features":[{"properties":{"highway":"road","width":8},
                "geometry":{"type":"LineString","coordinates":points.map(convert)}}]}).to_string()).unwrap();
        let reference = corridor_match::load(&path).unwrap();
        let evidence = scan(&archive, &Options::default(), 1).map_evidence.unwrap();
        let sweep = corridor_match::Sweep::prepare(
            &[correct, unchecked],
            correct.units,
            &evidence.road_sides,
        )
        .unwrap();
        let missing = sweep.missing_boxes(std::slice::from_ref(&reference));
        assert_eq!(missing.len(), 1);
        assert!(x < missing[0][0]);
        let cancelled = sweep.assess(std::slice::from_ref(&reference), |step| step.index == 0);
        assert_eq!(cancelled.unwrap_err(), "Grid comparison cancelled.");
        let mut steps = Vec::new();
        let results = corridor_match::assess_with_progress(
            &[correct, unchecked],
            correct.units,
            &evidence.road_sides,
            &[reference],
            |step| {
                steps.push(step);
                true
            },
        )
        .unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(
            (steps[0].index, steps[0].total, steps[0].candidate),
            (0, 2, correct)
        );
        assert_eq!(
            (steps[1].index, steps[1].total, steps[1].candidate),
            (1, 2, unchecked)
        );
        results
    };
    let good = evaluate([[349995., 5800004.], [350105., 5800004.]]);
    let Status::Measured {
        coverage, by_model, ..
    } = &good[0].status
    else {
        panic!("{:?}", good[0]);
    };
    assert!(coverage.matches());
    assert_eq!(by_model.len(), 1);
    assert!(matches!(good[1].status, Status::Unchecked));
    let crossing = evaluate([[350050., 5799985.], [350050., 5800023.]]);
    let Status::Measured { coverage, .. } = &crossing[0].status else {
        panic!("{:?}", crossing[0]);
    };
    assert!(!coverage.matches());
    let exact = Coverage {
        design_centre_m: 100.,
        covered_centre_m: 65.,
        design_edge_m: 100.,
        covered_edge_m: 75.,
    };
    assert!(!exact.matches());
    assert!(
        Coverage {
            covered_centre_m: 65.01,
            ..exact
        }
        .matches()
    );
}

#[test]
fn grid_ranking_groups_datums_and_keeps_unscored_candidates_out_of_the_shortlist() {
    use az12_converter::map::{
        corridor_match::{Assessment, Coverage, Status, ranked_grids},
        detection::Candidate,
        road_match::Grid,
    };
    let measured = |crs, covered| Assessment {
        candidate: Candidate {
            crs,
            units: SourceUnit::Metres,
        },
        status: Status::Measured {
            coverage: Coverage {
                design_centre_m: 100.0,
                covered_centre_m: covered,
                design_edge_m: 100.0,
                covered_edge_m: covered,
            },
            by_model: Default::default(),
            date: "test".into(),
        },
    };
    let results = [
        measured(CrsId::Gda94MgaZone54, 91.0),
        measured(CrsId::Gda2020MgaZone56, 66.0),
        measured(CrsId::Gda2020MgaZone54, 99.0),
        measured(CrsId::Gda2020MgaZone53, 75.0),
        measured(CrsId::Gda2020MgaZone52, 0.0),
        Assessment {
            candidate: Candidate {
                crs: CrsId::Gda2020MgaZone55,
                units: SourceUnit::Metres,
            },
            status: Status::Unchecked,
        },
    ];
    let ranked = ranked_grids(&results);
    assert_eq!(
        ranked.iter().map(|result| result.grid).collect::<Vec<_>>(),
        [Grid::Mga(54), Grid::Mga(53), Grid::Mga(56)]
    );
    assert_eq!(
        ranked[0]
            .variants
            .iter()
            .map(|(result, _)| result.candidate.crs)
            .collect::<Vec<_>>(),
        [CrsId::Gda2020MgaZone54, CrsId::Gda94MgaZone54]
    );
}

#[test]
fn overpass_road_reference_keeps_empty_areas_unchecked() {
    use az12_converter::map::corridor_match::overpass_references;
    let response = br#"{"osm3s":{"timestamp_osm_base":"2026-11-21T07:12:00Z"},"elements":[{"type":"way","tags":{"highway":"residential","width":"7"},"geometry":[{"lon":151.001,"lat":-34.998},{"lon":151.004,"lat":-34.998}]}]}"#;
    let boxes = [
        [151.0, -35.0, 151.01, -34.99],
        [145.0, -35.0, 145.01, -34.99],
    ];
    let references = overpass_references(response, &boxes).unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].date, "2026-11-21T07:12:00Z");
    assert!(overpass_references(br#"{"remark":"runtime timeout","elements":[]}"#, &boxes).is_err());
}

#[test]
#[ignore = "requires local sample archives and the corresponding OpenStreetMap extracts"]
fn automatic_corridor_comparison_checks_mga54_and_mga56_without_grid_selection() {
    use az12_converter::map::{
        corridor_match::{self, Status},
        detection::{self, Declaration, Detection},
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for (archive, reference, expected) in [
        (
            "12635625-GHD-00-00-MDL-CI-0001-ROAD DESIGN 12D.12daz",
            "mga54-ghd/mga54-ghd-engine-input.json",
            CrsId::Gda2020MgaZone54,
        ),
        (
            "1.7 SHRP-SMEC-WGI-RBT-M3D-COMBINED.12daz",
            "mga56-shrp-engine-input.json",
            CrsId::Gda2020MgaZone56,
        ),
    ] {
        let evidence = scan(&root.join("sample").join(archive), &Options::default(), 1)
            .map_evidence
            .unwrap();
        let Detection::Candidates(candidates) = detection::evaluate(
            &evidence,
            Declaration::Absent,
            Some(SourceUnit::Metres),
            None,
        ) else {
            panic!("candidate screening");
        };
        let reference =
            corridor_match::load(&root.join("outputs/road-boundary-engine").join(reference))
                .unwrap();
        let result = corridor_match::assess(
            &candidates,
            SourceUnit::Metres,
            &evidence.road_sides,
            &[reference],
        )
        .unwrap();
        let correct = result
            .iter()
            .find(|assessment| assessment.candidate.crs == expected)
            .unwrap();
        let Status::Measured { coverage, .. } = &correct.status else {
            panic!("{correct:?}");
        };
        eprintln!(
            "{}: {:.1}% pooled, {:.1}% centreline, {:.1}% edge",
            expected.epsg(),
            100.0 * coverage.pooled_fraction(),
            100.0 * coverage.centre_fraction(),
            100.0 * coverage.edge_fraction()
        );
        let expected_match = expected == CrsId::Gda2020MgaZone54;
        assert_eq!(coverage.matches(), expected_match);
        // The wider MGA56 comparison keeps the correct grid for review below the same cutoff.
        let best = result
            .iter()
            .filter_map(|assessment| match assessment.status {
                Status::Measured { coverage, .. } => {
                    Some((assessment.candidate.crs, coverage.pooled_fraction()))
                }
                _ => None,
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        assert_eq!(
            best.0.mga_details().map(|(_, zone)| zone),
            expected.mga_details().map(|(_, zone)| zone)
        );
        assert!(
            result
                .iter()
                .any(|assessment| matches!(assessment.status, Status::Unchecked))
        );
        assert!(result.iter().all(|assessment| !matches!(assessment.status, Status::Measured { coverage, .. } if coverage.matches()) || assessment.candidate.crs.mga_details().map(|(_, zone)| zone) == expected.mga_details().map(|(_, zone)| zone)));
    }
}

#[test]
fn mga_detect_29_30_known_zone_filters_units_and_both_datums() {
    use az12_converter::map::detection::Candidate;

    let original = [
        Candidate {
            crs: CrsId::Gda2020MgaZone54,
            units: SourceUnit::Metres,
        },
        Candidate {
            crs: CrsId::Gda94MgaZone54,
            units: SourceUnit::Metres,
        },
        Candidate {
            crs: CrsId::Gda2020MgaZone54,
            units: SourceUnit::Millimetres,
        },
        Candidate {
            crs: CrsId::Gda2020MgaZone55,
            units: SourceUnit::Metres,
        },
        Candidate {
            crs: CrsId::Gda2020Bcsg2020,
            units: SourceUnit::Metres,
        },
    ];
    let filter = |units, zone| {
        original
            .iter()
            .copied()
            .filter(|candidate| candidate.matches_filters(units, zone))
            .collect::<Vec<_>>()
    };
    assert_eq!(filter(Some(SourceUnit::Metres), Some(54)), original[..2]);
    assert!(filter(Some(SourceUnit::Feet), Some(54)).is_empty());
    assert_eq!(filter(None, Some(54)), original[..3]);
    assert_eq!(filter(None, None), original);
    assert!(filter(Some(SourceUnit::Metres), None).contains(&original[4]));
}

#[test]
fn mga_detect_08_11_14_scan_keeps_raw_extents_in_plain_and_zipped_input() {
    let folder = tempfile::tempdir().unwrap();
    let main = "model \"MGA55 GDA2020\" string super { data_2d { 350174.343 5812977.867 350294.343 5813057.867 } }\n";
    let local = "model Local string super { data_3d { 100 200 5 120 240 7 } }\n";
    let invalid = "model Invalid string super { data_2d { NaN 3 -999 4 } }\n";
    let header = "// archive_version \"15.1\"\n// EPSG:7855 metres\n";
    let plain = folder.path().join("MGA55.12da");
    fs::write(&plain, format!("{header}{main}{local}{invalid}")).unwrap();
    let zipped = folder.path().join("survey.12daz");
    let mut zip = zip::ZipWriter::new(fs::File::create(&zipped).unwrap());
    zip.start_file("survey.12da", zip::write::SimpleFileOptions::default())
        .unwrap();
    write!(&mut zip, "{header}{local}{invalid}{main}").unwrap();
    zip.finish().unwrap();

    let report = scan(&plain, &Options::default(), 100);
    assert!(
        serde_json::to_value(&report)
            .unwrap()
            .get("map_evidence")
            .is_none()
    );
    let evidence = report.map_evidence.unwrap();
    let keep_z = Options {
        geometry_mode: "keep-z".into(),
        ..Options::default()
    };
    assert_eq!(
        scan(&zipped, &keep_z, 1).map_evidence.as_ref(),
        Some(&evidence)
    );
    assert!(scan(&plain, &keep_z, 0).map_evidence.is_none());
    let selected = Options {
        models: Some(vec!["Local".into()]),
        ..Options::default()
    };
    assert!(scan(&plain, &selected, 100).map_evidence.is_none());
    assert_eq!(evidence.models.len(), 2);
    assert_eq!(
        evidence.models["Local"].samples,
        [point([100.0, 200.0]), point([120.0, 240.0])]
    );
    assert_eq!(
        evidence.models["MGA55 GDA2020"].extremes[1],
        point([350294.343, 5813057.867])
    );
}

#[test]
fn mga_detect_24_25_brisbane_grids_match_independent_references() {
    for (crs, epsg, projected, geographic) in BRISBANE {
        assert_eq!(crs.epsg(), epsg);
        let projection = Projection::new(crs).unwrap();
        for ([x, y], [lon, lat]) in projected.into_iter().zip(geographic) {
            let actual = projection.to_wgs84(point([x, y])).unwrap();
            assert!((actual.longitude() - lon).hypot(actual.latitude() - lat) < 4e-7);
            let actual = projection
                .from_wgs84(LonLat::new(lon, lat).unwrap())
                .unwrap();
            assert!((actual.x() - x).hypot(actual.y() - y) < 0.05);
        }
    }
    // The legacy origin expressed in GDA2020 via EPSG operation 8048.
    let legacy = Projection::new(CrsId::Gda94Bcsg02).unwrap();
    let origin = legacy.to_wgs84(point([50_000.0, 100_000.0])).unwrap();
    assert!(
        (origin.longitude() - 153.00000596974382).hypot(origin.latitude() + 27.999987446618036)
            < 4e-7
    );
    let current = Projection::new(CrsId::Gda2020Bcsg2020).unwrap();
    let origin = current.to_wgs84(point([150_000.0, 200_000.0])).unwrap();
    assert!((origin.longitude() - 153.0).hypot(origin.latitude() + 28.0) < 4e-7);
}

#[test]
fn mga_detect_27_28_brisbane_units_controls_and_sidecars() {
    let folder = tempfile::tempdir().unwrap();
    for (crs, epsg, [xy, _], _) in BRISBANE {
        let millimetres = point(xy.map(|value| value * 1_000.0));
        let registration = MapRegistration::assign(crs, SourceUnit::Millimetres).unwrap();
        let actual = registration.to_project(millimetres).unwrap();
        assert!((actual.x() - xy[0]).hypot(actual.y() - xy[1]) < 1e-8);

        let path = folder.path().join(format!("{}.12daz", &epsg[5..]));
        sidecar::save(&path, "survey-sha", registration, false).unwrap();
        assert_eq!(
            sidecar::load(&path, "survey-sha").unwrap(),
            Some((registration, false))
        );
        assert!(sidecar::load(&path, "other-survey").is_err());

        let local = point([100.0, 200.0]);
        let anchor = point(xy);
        let aligned =
            MapRegistration::align(crs, SourceUnit::Metres, (local, anchor), None).unwrap();
        assert_eq!(aligned.to_project(local).unwrap(), anchor);
        assert_eq!(aligned.to_drawing(anchor).unwrap(), local);
    }
    let existing = MapRegistration::assign(CrsId::Gda2020MgaZone56, SourceUnit::Metres).unwrap();
    let path = folder.path().join("mga56.12daz");
    sidecar::save(&path, "mga-sha", existing, false).unwrap();
    assert_eq!(
        sidecar::load(&path, "mga-sha").unwrap(),
        Some((existing, false))
    );
}

#[test]
fn mga_detect_17_scan_and_alignment_leave_export_unchanged() {
    use acadrust::{DxfReader, EntityType};
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("brisbane.12da");
    fs::write(&path, "// archive_version \"15.1\"\nmodel Survey string super { breakline line data_3d { 152777.351 258753.314 10 146741.770 245876.982 20 } }\n").unwrap();
    let scanned = scan(&path, &Options::default(), 30);
    assert!(scanned.map_evidence.is_some());
    let registration = MapRegistration::assign(CrsId::Gda2020Bcsg2020, SourceUnit::Metres).unwrap();
    for (mode, expected_z) in [("flatten-2d", [0.0, 0.0]), ("keep-z", [10.0, 20.0])] {
        let options = Options {
            geometry_mode: mode.into(),
            units: Some("metres".into()),
            ..Options::default()
        };
        let mut report_fields = Vec::new();
        for save in [false, true] {
            if save {
                sidecar::save(&path, &scanned.input_sha256, registration, false).unwrap();
            } else {
                let saved = path.with_file_name("brisbane.12da.12daz-map.json");
                if saved.exists() {
                    fs::remove_file(saved).unwrap();
                }
            }
            let output = folder.path().join(format!("{mode}-{save}.dxf"));
            let report = run_job(
                &path,
                &options,
                Job {
                    output: Some(&output),
                    report: None,
                    limits: Limits::default(),
                    cancel: None,
                    progress: &mut |_, _| {},
                    preview_limit: 0,
                },
            )
            .unwrap();
            assert!(report.map_evidence.is_none());
            let report = serde_json::to_value(report).unwrap();
            assert!(report.get("map_evidence").is_none());
            report_fields.push(
                [
                    "input_sha256",
                    "options",
                    "source_counts",
                    "bounds",
                    "diagnostics",
                    "emitted_entities",
                    "layer_mapping",
                ]
                .map(|key| report[key].clone()),
            );
            let drawing = DxfReader::from_file(&output).unwrap().read().unwrap();
            let mut entities = drawing.model_space_entities();
            let vertices: Vec<_> = match entities.next().unwrap() {
                EntityType::LwPolyline(line) => line
                    .vertices
                    .iter()
                    .map(|v| (v.location.x, v.location.y, line.elevation))
                    .collect(),
                EntityType::Polyline3D(line) => line
                    .vertices
                    .iter()
                    .map(|v| (v.position.x, v.position.y, v.position.z))
                    .collect(),
                _ => panic!("expected one survey line"),
            };
            assert_eq!(
                vertices,
                [
                    (152777.351, 258753.314, expected_z[0]),
                    (146741.770, 245876.982, expected_z[1])
                ]
            );
            assert_eq!(drawing.header.insertion_units, 6);
            assert!(entities.next().is_none());
        }
        assert_eq!(report_fields[0], report_fields[1]);
    }
}
