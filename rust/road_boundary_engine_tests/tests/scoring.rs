// Fixture arithmetic for independently measured lengths. Spatial matching supplies these lengths.
fn edge_scores(predicted: f64, reference: f64, matched: f64) -> (f64, f64, f64) {
    if matched == 0.0 {
        return (0.0, 0.0, 0.0);
    }
    (
        matched / predicted,
        matched / reference,
        2.0 * matched / (predicted + reference),
    )
}

#[derive(Clone, Copy)]
struct Coverage {
    matched: u64,
    evaluable: u64,
}

fn projection_match(edges: &[Coverage], centrelines: &[Coverage]) -> Option<bool> {
    // Integer fixture units express the strict 7/10 threshold without display rounding.
    let edge_length: u64 = edges.iter().map(|item| item.evaluable).sum();
    let centreline_length: u64 = centrelines.iter().map(|item| item.evaluable).sum();
    if edge_length == 0 || centreline_length == 0 {
        return None;
    }
    let matched: u64 = edges
        .iter()
        .chain(centrelines)
        .map(|item| item.matched)
        .sum();
    Some(10 * matched > 7 * (edge_length + centreline_length))
}

#[test]
fn rbe_t09_exactly_eighty_percent_qualifies() {
    assert_eq!(edge_scores(100.0, 100.0, 80.0), (0.8, 0.8, 0.8));
}

#[test]
fn rbe_t10_duplicate_length_reduces_precision() {
    assert_eq!(edge_scores(200.0, 100.0, 100.0), (0.5, 1.0, 2.0 / 3.0));
}

#[test]
fn rbe_t12_centreline_only_does_not_earn_boundary_recall() {
    assert_eq!(edge_scores(100.0, 200.0, 100.0), (1.0, 0.5, 2.0 / 3.0));
    assert_eq!(edge_scores(0.0, 100.0, 0.0), (0.0, 0.0, 0.0));
}

#[test]
fn rpm_t01_projection_requires_strictly_more_than_seventy_percent() {
    let edges = [Coverage {
        matched: 750,
        evaluable: 1_000,
    }];
    let exactly = [Coverage {
        matched: 650,
        evaluable: 1_000,
    }];
    let above = [Coverage {
        matched: 651,
        evaluable: 1_000,
    }];
    assert_eq!(projection_match(&edges, &exactly), Some(false));
    assert_eq!(projection_match(&edges, &above), Some(true));
}

#[test]
fn rpm_t03_pool_length_across_models() {
    let edges = [Coverage {
        matched: 100,
        evaluable: 100,
    }];
    let centrelines = [Coverage {
        matched: 240,
        evaluable: 400,
    }];
    assert_eq!(projection_match(&edges, &centrelines), Some(false));
}

#[test]
fn rpm_t04_missing_edges_leave_the_result_inconclusive() {
    let centrelines = [Coverage {
        matched: 100,
        evaluable: 100,
    }];
    assert_eq!(projection_match(&[], &centrelines), None);
    let unmatched_edges = [Coverage {
        matched: 0,
        evaluable: 100,
    }];
    assert_eq!(
        projection_match(&unmatched_edges, &centrelines),
        Some(false)
    );
}
