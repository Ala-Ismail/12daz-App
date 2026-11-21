//! Bounded, user-initiated OpenStreetMap road-vector requests.

use std::{
    io::Read,
    sync::Mutex,
    time::{Duration, Instant},
};

use az12_converter::map::corridor_match::{self, Reference};

const ENDPOINT: &str = "https://overpass-api.de/api/interpreter";
const BYTES: u64 = 2 * 1024 * 1024;
static CACHE: Mutex<Cache> = Mutex::new(Cache {
    attempted: None,
    response: None,
});

struct Cache {
    attempted: Option<Instant>,
    response: Option<(Instant, String, Vec<Reference>)>,
}

pub(super) fn fetch(boxes: &[[f64; 4]]) -> Result<Vec<Reference>, String> {
    if boxes.is_empty() {
        return Ok(Vec::new());
    }
    let area: f64 = boxes.iter().map(|[w, s, e, n]| (e - w) * (n - s)).sum();
    if boxes.len() > 24 || area > 0.05 {
        return Err("The search area is too large for an automatic map request. Local extracts can cover larger areas.".into());
    }
    let query = query(boxes);
    let mut cache = CACHE
        .try_lock()
        .map_err(|_| "An OpenStreetMap road request is already in progress.")?;
    let now = Instant::now();
    if let Some((loaded, key, result)) = &cache.response
        && key == &query
        && now.duration_since(*loaded) < Duration::from_secs(3600)
    {
        return Ok(result.clone());
    }
    if cache
        .attempted
        .is_some_and(|previous| now.duration_since(previous) < Duration::from_secs(12))
    {
        return Err("Wait a few seconds before requesting more OpenStreetMap road data.".into());
    }
    cache.attempted = Some(now);
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(50)))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .new_agent();
    let response = agent
        .post(ENDPOINT)
        .header(
            "User-Agent",
            concat!(
                "12dAZ-Converter/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/Ala-Ismail/12daz-App)"
            ),
        )
        .send_form([("data", query.as_str())])
        .map_err(|_| "Could not reach OpenStreetMap road data.")?;
    let (headers, mut body) = response.into_parts();
    if !headers.status.is_success() {
        return Err(format!(
            "OpenStreetMap road service returned HTTP {}.",
            headers.status.as_u16()
        ));
    }
    let mut data = Vec::new();
    body.as_reader()
        .take(BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|_| "Could not read OpenStreetMap road data.")?;
    if data.len() as u64 > BYTES {
        return Err("OpenStreetMap road response exceeded the size limit.".into());
    }
    let references = corridor_match::overpass_references(&data, boxes)?;
    cache.response = Some((now, query, references.clone()));
    Ok(references)
}

fn query(boxes: &[[f64; 4]]) -> String {
    let clauses: String = boxes.iter().map(|[w,s,e,n]| format!("way[\"highway\"]({s:.6},{w:.6},{n:.6},{e:.6});way[\"area:highway\"]({s:.6},{w:.6},{n:.6},{e:.6});")).collect();
    format!("[out:json][timeout:15][maxsize:33554432];({clauses});out geom;")
}

#[cfg(test)]
mod tests {
    #[test]
    fn road_request_contains_only_bounds_and_fixed_tags() {
        let result = super::query(&[[150.0, -35.0, 150.01, -34.99]]);
        assert!(result.contains("way[\"highway\"](-35.000000,150.000000,-34.990000,150.010000)"));
        assert!(result.ends_with("out geom;"));
    }

    #[test]
    #[ignore = "requests road vectors for one small public area from OpenStreetMap"]
    fn fetches_a_small_public_road_reference() {
        let result = super::fetch(&[[145.294, -37.821, 145.300, -37.816]]).unwrap();
        assert_eq!(result.len(), 1);
        assert!(!result[0].date.is_empty());
    }
}
