use serde_json::{Value, json};
use std::{env, fs, process::Command};

fn edge(x0: f64, x1: f64, y: f64) -> Value {
    json!({"points":[[350_000.0+x0,5_813_000.0+y],[350_000.0+x1,5_813_000.0+y]],"provenance":"estimated_edge"})
}

fn run(edges: Vec<Value>, sides: Value, reference: Value) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let prediction_file = "road.json";
    let input = dir.path().join("benchmark.json");
    let output = dir.path().join("report.json");
    let dxf = dir.path().join("report.dxf");
    let manifest = json!({"crs":"EPSG:7855","tolerance_m":0.5,"reference_source":"Synthetic benchmark fixture","reference_date":"2026-11-21","sections":[{
        "id":"straight","road_type":"residential","window":[349990.0,5812990.0,350110.0,5813010.0],
        "prediction_file":prediction_file,"prediction_sides":sides,"reference":reference
    }]});
    fs::write(&input, manifest.to_string()).unwrap();
    fs::write(
        dir.path().join(prediction_file),
        json!({"output_crs":"EPSG:7855","edges":edges}).to_string(),
    )
    .unwrap();
    let result =
        Command::new(env::var_os("RBE_ENGINE_EXE").expect("provide standalone engine executable"))
            .arg("--benchmark")
            .arg(input)
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
    assert!(dxf.is_file());
    serde_json::from_slice(&fs::read(output).unwrap()).unwrap()
}

fn truth(side: &str, y: f64) -> Value {
    json!({"side":side,"points":edge(0.0,100.0,y)["points"]})
}

fn check(report: &Value, precision: f64, recall: f64, f1: f64) {
    for (field, expected) in [("precision", precision), ("recall", recall), ("f1", f1)] {
        assert!(
            (report["overall"][field].as_f64().unwrap() - expected).abs() < 0.00001,
            "{field}: {report}"
        );
    }
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t09_measured_eighty_percent_passes_the_threshold() {
    let result = run(
        vec![edge(0., 80., 0.25), edge(80., 100., 5.)],
        json!({"0":"left","1":"left"}),
        json!([truth("left", 0.)]),
    );
    check(&result, 0.8, 0.8, 0.8);
    assert_eq!(result["meets_numerical_threshold"], true);
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t10_duplicate_predicted_length_is_only_matched_once() {
    let result = run(
        vec![edge(0., 100., 0.1), edge(0., 100., 0.1)],
        json!({"0":"left","1":"left"}),
        json!([truth("left", 0.)]),
    );
    check(&result, 0.5, 1.0, 2.0 / 3.0);
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t11_outside_tolerance_and_opposite_side_are_misses() {
    let result = run(
        vec![edge(0., 100., 0.6), edge(0., 100., 4.)],
        json!({"0":"left","1":"left"}),
        json!([truth("left", 0.), truth("right", 4.)]),
    );
    check(&result, 0.0, 0.0, 0.0);
}

#[test]
#[ignore = "requires standalone engine executable"]
fn rbe_t12_an_empty_edge_prediction_still_counts_reference_length() {
    let result = run(Vec::new(), json!({}), json!([truth("left", 0.)]));
    check(&result, 0.0, 0.0, 0.0);
    assert!((result["overall"]["reference_m"].as_f64().unwrap() - 100.0).abs() < 0.001);
}
