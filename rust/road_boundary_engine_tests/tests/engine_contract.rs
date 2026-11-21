use serde_json::{Value, json};
use std::{env, fs, process::Command};

const ENGINE_REQUIRED: &str = "set RBE_ENGINE_EXE to the standalone engine executable";

fn run(features: Value, input_crs: Option<&str>) -> Result<Value, String> {
    run_with_clip(features, input_crs, None)
}

fn run_with_clip(
    features: Value,
    input_crs: Option<&str>,
    clip: Option<[f64; 4]>,
) -> Result<Value, String> {
    let exe = env::var_os("RBE_ENGINE_EXE").expect(ENGINE_REQUIRED);
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.json");
    let output = dir.path().join("output.json");
    let mut fixture = json!({"output_crs": "EPSG:7855", "features": features});
    if let Some(crs) = input_crs {
        fixture["input_crs"] = json!(crs);
    }
    if let Some(clip) = clip {
        fixture["clip"] = json!(clip);
    }
    fs::write(&input, serde_json::to_vec(&fixture).unwrap()).unwrap();
    let result = Command::new(exe)
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).into_owned());
    }
    Ok(serde_json::from_slice(&fs::read(output).unwrap()).unwrap())
}

fn road_line() -> Value {
    json!({"id":"road", "tags":{"highway":"residential", "width":"8"},
        "geometry":{"type":"LineString", "coordinates":[[350000,5813000],[350100,5813000]]}})
}

fn xy(x: f64, y: f64) -> [f64; 2] {
    [350_000.0 + x, 5_813_000.0 + y]
}

fn rectangle(left: f64, bottom: f64, right: f64, top: f64) -> [[f64; 2]; 5] {
    [
        xy(left, bottom),
        xy(right, bottom),
        xy(right, top),
        xy(left, top),
        xy(left, bottom),
    ]
}

fn line(id: &str, tags: Value, coordinates: &[[f64; 2]]) -> Value {
    json!({"id":id, "tags":tags, "geometry":{"type":"LineString", "coordinates":coordinates}})
}

fn has_segment_at(edges: &[Value], id: &str, [x, y]: [f64; 2]) -> bool {
    edges
        .iter()
        .filter(|edge| edge["source_ids"].as_array().unwrap().contains(&json!(id)))
        .any(|edge| {
            edge["points"]
                .as_array()
                .unwrap()
                .windows(2)
                .any(|segment| {
                    let [start, end] = [0, 1].map(|i| {
                        [
                            segment[i][0].as_f64().unwrap(),
                            segment[i][1].as_f64().unwrap(),
                        ]
                    });
                    let dx = end[0] - start[0];
                    let dy = end[1] - start[1];
                    let position = (((x - start[0]) * dx + (y - start[1]) * dy)
                        / (dx * dx + dy * dy))
                        .clamp(0.0, 1.0);
                    (x - start[0] - position * dx).hypot(y - start[1] - position * dy) < 0.05
                })
        })
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t04_joined_surface_preserves_grade_and_separate_carriageways() {
    let tags = json!({"highway":"road","width":"8"});
    let features = json!([
        road_line(),
        line("junction", tags.clone(), &[xy(50., 0.), xy(50., 20.)]),
        line(
            "bridge",
            json!({"highway":"road","width":"8","bridge":"yes","layer":"1"}),
            &[xy(20., -20.), xy(20., 20.)]
        ),
        line("parallel", tags, &[xy(0., -12.), xy(100., -12.)])
    ]);
    let result = run(features, Some("EPSG:7855")).unwrap();
    let edges = result["edges"].as_array().unwrap();
    assert!(!has_segment_at(edges, "road", xy(50., 4.)));
    assert!(has_segment_at(edges, "road", xy(20., 4.)));
    assert!(has_segment_at(edges, "bridge", xy(16., 0.)));
    assert!(has_segment_at(edges, "junction", xy(46., 10.)));
    assert!(has_segment_at(edges, "parallel", xy(50., -8.)));
    assert!(has_segment_at(edges, "road", xy(50., -4.)));
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t05_clipping_introduces_no_edges_and_invalid_values_are_reported() {
    let invalid_polygon = json!({"id":"crossed", "tags":{"area:highway":"road"},
        "geometry":{"type":"Polygon", "coordinates":[[xy(8.,8.),xy(18.,18.),xy(8.,18.),xy(18.,8.),xy(8.,8.)]]}});
    let features = json!([
        invalid_polygon,
        road_line(),
        line(
            "negative",
            json!({"highway":"road","width":"-5"}),
            &[xy(0., -30.), xy(100., -30.)]
        ),
        line(
            "malformed",
            json!({"highway":"road","width":"unknown"}),
            &[xy(0., -35.), xy(100., -35.)]
        )
    ]);
    let [west, south] = xy(15., -10.);
    let [east, north] = xy(35., 10.);
    let result = run_with_clip(
        features,
        Some("EPSG:7855"),
        Some([west, south, east, north]),
    )
    .unwrap();
    let edges = result["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    assert!(!has_segment_at(edges, "road", xy(15., 0.)));
    for edge in edges {
        let points = edge["points"].as_array().unwrap();
        let mut xs: Vec<_> = points
            .iter()
            .map(|point| point[0].as_f64().unwrap())
            .collect();
        xs.sort_by(f64::total_cmp);
        assert!((xs[0] - west).abs() < 0.05);
        assert!((xs.last().unwrap() - east).abs() < 0.05);
    }
    let diagnostics = result["diagnostics"].as_array().unwrap();
    for (id, code) in [
        ("crossed", "invalid_polygon"),
        ("negative", "invalid_width"),
        ("malformed", "invalid_width"),
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|entry| entry["feature_id"] == id && entry["code"] == code)
        );
    }
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t01_mapped_surface_preserves_island_and_supersedes_covered_offsets() {
    let surface = json!({"id":"surface", "tags":{"area:highway":"residential"},
        "geometry":{"type":"Polygon", "coordinates":[rectangle(10., -4., 90., 4.), rectangle(40., 1., 60., 2.)]}});
    let result = run(json!([surface, road_line()]), Some("EPSG:7855")).unwrap();
    let edges = result["edges"].as_array().unwrap();
    let outlines: Vec<_> = edges
        .iter()
        .filter(|edge| edge["provenance"] == "mapped_outline")
        .cloned()
        .collect();
    for point in [xy(50., 1.), xy(50., 2.), xy(40., 1.5), xy(60., 1.5)] {
        assert!(
            has_segment_at(&outlines, "surface", point),
            "missing island side near {point:?}"
        );
    }
    let estimates: Vec<_> = edges
        .iter()
        .filter(|edge| edge["provenance"] == "estimated_edge")
        .collect();
    assert_eq!(estimates.len(), 4);
    for edge in estimates {
        let points = edge["points"].as_array().unwrap();
        let x = |p: &Value| p[0].as_f64().unwrap() - 350_000.0;
        let span = (x(points.first().unwrap()) - x(points.last().unwrap())).abs();
        assert!((span - 10.0).abs() < 0.05);
    }
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t03_lanes_supply_width_and_unknown_width_keeps_centreline() {
    let inferred = json!({"id":"lanes", "tags":{"highway":"road","lanes":"2"},
        "geometry":{"type":"LineString","coordinates":[xy(0.,0.),xy(50.,0.),xy(75.,30.)]}});
    let unknown = json!({"id":"unknown", "tags":{"highway":"road"},
        "geometry":{"type":"LineString","coordinates":[xy(90.,30.),xy(100.,30.)]}});
    let result = run(json!([inferred, unknown]), Some("EPSG:7855")).unwrap();
    let edges = result["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    for edge in edges {
        assert_eq!(edge["width_source"], "lane_count");
        assert_eq!(edge["width_m"], 6.6);
        assert!(edge["points"].as_array().unwrap().len() >= 3);
    }
    assert_eq!(result["centrelines"].as_array().unwrap().len(), 2);
    assert!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["feature_id"] == "unknown" && item["code"] == "centreline_only")
    );
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t02_mapped_width_creates_two_sides_without_end_caps() {
    let result = run(json!([road_line()]), Some("EPSG:7855")).unwrap();
    assert_eq!(result["output_crs"], "EPSG:7855");
    let edges = result["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    let mut sides = Vec::new();
    for edge in edges {
        assert_eq!(edge["source_ids"], json!(["road"]));
        assert_eq!(edge["provenance"], "estimated_edge");
        assert_eq!(edge["width_source"], "mapped_width");
        let points = edge["points"].as_array().unwrap();
        let north = points[0][1].as_f64().unwrap();
        assert!(
            points
                .iter()
                .all(|p| (p[1].as_f64().unwrap() - north).abs() < 0.05)
        );
        let east_span =
            points.last().unwrap()[0].as_f64().unwrap() - points[0][0].as_f64().unwrap();
        assert!((east_span.abs() - 100.0).abs() < 0.05);
        sides.push(north);
    }
    sides.sort_by(f64::total_cmp);
    assert!((sides[0] - 5_812_996.0).abs() < 0.05);
    assert!((sides[1] - 5_813_004.0).abs() < 0.05);
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t06_absent_crs_fails_with_a_diagnostic() {
    let error = run(json!([road_line()]), None).unwrap_err();
    assert!(error.to_ascii_lowercase().contains("crs"), "{error}");
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t06_brisbane_projected_origin_agrees_with_geographic_origin() {
    let tags = json!({"highway":"road"});
    let grid = line(
        "brisbane",
        tags.clone(),
        &[[150_000.0, 200_000.0], [150_100.0, 200_000.0]],
    );
    let geographic = line("brisbane", tags, &[[153.0, -28.0], [153.001, -28.0]]);
    let grid = run(json!([grid]), Some("EPSG:20047")).unwrap();
    let geographic = run(json!([geographic]), Some("EPSG:4326")).unwrap();
    for axis in 0..2 {
        let point = |result: &Value| {
            result["centrelines"][0]["points"][0][axis]
                .as_f64()
                .unwrap()
        };
        assert!((point(&grid) - point(&geographic)).abs() < 0.05);
    }
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t06_equivalent_geographic_and_projected_geometry() {
    let mut line = road_line();
    line["geometry"]["coordinates"] = json!([[147.0, -0.001], [147.0, -0.002]]);
    let geographic = run(json!([line]), Some("EPSG:4326")).unwrap();
    let mut line = road_line();
    line["geometry"]["coordinates"] = json!([[500_000.0, 9_999_889.47], [500_000.0, 9_999_778.94]]);
    let projected = run(json!([line]), Some("EPSG:7855")).unwrap();
    let points = |value: &Value| {
        value["centrelines"][0]["points"]
            .as_array()
            .unwrap()
            .clone()
    };
    for (left, right) in points(&geographic).iter().zip(points(&projected).iter()) {
        for axis in 0..2 {
            assert!((left[axis].as_f64().unwrap() - right[axis].as_f64().unwrap()).abs() < 0.05);
        }
    }
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t05_malformed_feature_does_not_discard_valid_neighbour() {
    let malformed = json!({"id":"bad", "geometry":{"type":"LineString","coordinates":[[0,0]]}});
    let result = run(json!([malformed, road_line()]), Some("EPSG:7855")).unwrap();
    assert_eq!(result["diagnostics"][0]["feature_id"], "bad");
    assert_eq!(result["centrelines"][0]["source_ids"], json!(["road"]));
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t07_identical_input_produces_identical_road_geometry() {
    let input = json!([road_line()]);
    let first = run(input.clone(), Some("EPSG:7855")).unwrap();
    let second = run(input, Some("EPSG:7855")).unwrap();
    for key in ["edges", "centrelines"] {
        assert!(first[key].is_array());
        assert_eq!(first[key], second[key]);
    }
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t14_surface_synthesises_a_road_centreline() {
    let surface = json!({"id":"surface", "tags":{"area:highway":"residential"},
        "geometry":{"type":"Polygon", "coordinates":[[[350000,5812996],
        [350100,5812996],[350100,5813004],[350000,5813004],[350000,5812996]]]}});
    let result = run(json!([surface]), Some("EPSG:7855")).unwrap();
    let lines = result["centrelines"].as_array().unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["source_ids"], json!(["surface"]));
    assert_eq!(lines[0]["provenance"], "synthesised_centreline");
    let points = lines[0]["points"].as_array().unwrap();
    assert!(
        points
            .iter()
            .all(|point| (point[1].as_f64().unwrap() - 5_813_000.0).abs() < 0.05)
    );
    assert!(
        points
            .iter()
            .any(|point| (350_000.0..=350_020.0).contains(&point[0].as_f64().unwrap()))
    );
    assert!(
        points
            .iter()
            .any(|point| (350_080.0..=350_100.0).contains(&point[0].as_f64().unwrap()))
    );
}

fn surface(id: &str, ring: &[[f64; 2]]) -> Value {
    json!({"id":id,"tags":{"area:highway":"residential"},"geometry":{"type":"Polygon","coordinates":[ring]}})
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t15_synthesised_lines_follow_bend_and_keep_median_clear() {
    let bent = [
        xy(0., -4.),
        xy(64., -4.),
        xy(64., 60.),
        xy(56., 60.),
        xy(56., 4.),
        xy(0., 4.),
        xy(0., -4.),
    ];
    let branched = [
        xy(110., -4.),
        xy(210., -4.),
        xy(210., 4.),
        xy(164., 4.),
        xy(164., 55.),
        xy(156., 55.),
        xy(156., 4.),
        xy(110., 4.),
        xy(110., -4.),
    ];
    let features = json!([
        surface("bend", &bent),
        surface("parallel", &rectangle(0., -16., 64., -8.)),
        surface("branch", &branched)
    ]);
    let result = run(features, Some("EPSG:7855")).unwrap();
    let lines = result["centrelines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    assert!(has_segment_at(lines, "bend", xy(25., 0.)));
    assert!(has_segment_at(lines, "bend", xy(60., 40.)));
    assert!(has_segment_at(lines, "parallel", xy(30., -12.)));
    for line in lines {
        for point in line["points"].as_array().unwrap() {
            let x = point[0].as_f64().unwrap() - 350_000.0;
            let y = point[1].as_f64().unwrap() - 5_813_000.0;
            if line["source_ids"] == json!(["bend"]) {
                assert!(
                    (y.abs() <= 4.0 && (0.0..=64.0).contains(&x))
                        || ((56.0..=64.0).contains(&x) && (4.0..=60.0).contains(&y))
                );
            } else {
                assert!((-16.0..=-8.0).contains(&y));
            }
        }
    }
    assert!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["feature_id"] == "branch" && item["code"] == "ambiguous_centreline")
    );
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t16_keep_mapped_line_and_require_complete_edge_pair_for_synthesis() {
    let features = json!([
        surface("polygon", &rectangle(0., -4., 100., 4.)),
        road_line(),
        line(
            "incomplete",
            json!({"road_side":"left","corridor_id":"missing"}),
            &[xy(0., 45.), xy(100., 45.)]
        ),
        line(
            "left",
            json!({"road_side":"left","corridor_id":"pair"}),
            &[xy(0., 25.), xy(100., 25.)]
        ),
        line(
            "right",
            json!({"road_side":"right","corridor_id":"pair"}),
            &[xy(100., 17.), xy(0., 17.)]
        )
    ]);
    let result = run(features, Some("EPSG:7855")).unwrap();
    let lines = result["centrelines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["provenance"], "mapped_centreline");
    assert_eq!(lines[1]["source_ids"], json!(["left", "right"]));
    assert!(has_segment_at(lines, "left", xy(35., 21.)));
    assert!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["feature_id"] == "incomplete"
                && item["code"] == "incomplete_road_sides")
    );
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t08_dxf_reopens_with_scaled_units_and_separate_layers() {
    use acadrust::{DxfReader, EntityType};
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    let output = dir.path().join("out.json");
    let dxf = dir.path().join("out.dxf");
    let fixture = json!({"input_crs":"EPSG:7855","output_crs":"EPSG:7855","features":[
        road_line(),surface("no-line", &rectangle(0.,-28.,100.,-20.)),
        line("unknown",json!({"highway":"road"}),&[xy(0.,15.),xy(100.,15.)])
    ]});
    fs::write(&input, format!("\u{feff}{fixture}")).unwrap();
    let result = Command::new(env::var_os("RBE_ENGINE_EXE").expect(ENGINE_REQUIRED))
        .arg("--input")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .arg("--dxf")
        .arg(&dxf)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let drawing = DxfReader::from_file(&dxf).unwrap().read().unwrap();
    assert_eq!(drawing.header.insertion_units, 6);
    let mut layers = std::collections::BTreeMap::new();
    for entity in drawing.entities() {
        if let EntityType::LwPolyline(path) = entity {
            *layers.entry(path.common.layer.as_str()).or_insert(0) += 1;
            for vertex in &path.vertices {
                assert!((350_000.0..=350_100.0).contains(&vertex.location.x));
                assert!((5_812_972.0..=5_813_015.0).contains(&vertex.location.y));
            }
        }
    }
    for (layer, count) in [
        ("ESTIMATED-ROAD-EDGE", 2),
        ("MAPPED-ROAD-OUTLINE", 1),
        ("MAPPED-ROAD-CENTRELINE", 1),
        ("SYNTHESISED-ROAD-CENTRELINE", 1),
        ("CENTRELINE-ONLY", 1),
    ] {
        assert_eq!(layers.get(layer), Some(&count));
    }
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t16_synthesis_only_fills_the_gap_between_mapped_sections() {
    let features = json!([
        surface("polygon", &rectangle(0., -4., 100., 4.)),
        line(
            "west",
            json!({"highway":"road"}),
            &[xy(0., 0.), xy(30., 0.)]
        ),
        line(
            "east",
            json!({"highway":"road"}),
            &[xy(70., 0.), xy(100., 0.)]
        )
    ]);
    let result = run(features, Some("EPSG:7855")).unwrap();
    let lines = result["centrelines"].as_array().unwrap();
    assert!(has_segment_at(lines, "polygon", xy(50., 0.)));
    assert!(!has_segment_at(lines, "polygon", xy(20., 0.)));
    assert!(!has_segment_at(lines, "polygon", xy(80., 0.)));
}
