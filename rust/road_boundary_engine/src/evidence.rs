use std::path::Path;

use acadrust::entities::{EntityCommon, LwVertex};
use acadrust::{
    CadDocument, Color, DxfVersion, DxfWriter, EntityType, Layer, LwPolyline, Text, Vector2,
    Vector3,
};

use crate::{Output, Point, benchmark::Report};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn detection(output: &Output, path: &Path) -> Result<()> {
    let mut drawing = drawing(&[
        ("MAPPED-ROAD-OUTLINE", 3),
        ("ESTIMATED-ROAD-EDGE", 2),
        ("MAPPED-ROAD-CENTRELINE", 4),
        ("SYNTHESISED-ROAD-CENTRELINE", 6),
        ("CENTRELINE-ONLY", 8),
        ("MAP-SOURCE", 7),
    ])?;
    for road in output.edges.iter().chain(&output.centrelines) {
        let layer = match road.provenance {
            "mapped_outline" => "MAPPED-ROAD-OUTLINE",
            "estimated_edge" => "ESTIMATED-ROAD-EDGE",
            "mapped_centreline" if road.edge_status == Some("centreline_only") => "CENTRELINE-ONLY",
            "mapped_centreline" => "MAPPED-ROAD-CENTRELINE",
            "synthesised_centreline" => "SYNTHESISED-ROAD-CENTRELINE",
            other => return Err(format!("unknown line provenance {other}").into()),
        };
        polyline(&mut drawing, layer, road.points.iter().copied())?;
    }
    if let Some(reference) = &output.reference {
        let [x, y, _, _] = reference.coverage;
        let text = Text {
            common: EntityCommon {
                layer: "MAP-SOURCE".into(),
                ..EntityCommon::default()
            },
            value: format!(
                "{} | {} | {} | {}",
                output.output_crs, reference.source, reference.date, reference.attribution
            )
            .chars()
            .filter(|ch| !ch.is_control())
            .take(240)
            .collect(),
            insertion_point: Vector3::new(x, y - 5.0, 0.0),
            height: 3.0,
            ..Text::new()
        };
        drawing.add_entity(EntityType::Text(text))?;
    }
    DxfWriter::new(&drawing).write_to_file(path)?;
    Ok(())
}

pub fn benchmark(report: &Report, path: &Path) -> Result<()> {
    let mut drawing = drawing(&[
        ("REFERENCE-ROAD-EDGE", 4),
        ("MATCHED-PREDICTED-ROAD-EDGE", 3),
        ("UNMATCHED-PREDICTED-ROAD-EDGE", 1),
        ("MISSED-REFERENCE-ROAD-EDGE", 2),
    ])?;
    for (layer, sections) in &report.evidence {
        for section in sections {
            polyline(
                &mut drawing,
                layer,
                section.0.iter().map(|point| [point.x, point.y]),
            )?;
        }
    }
    DxfWriter::new(&drawing).write_to_file(path)?;
    Ok(())
}

pub fn corridor(report: &crate::corridor::Report, path: &Path) -> Result<()> {
    let mut drawing = drawing(&[
        ("FIT-MATCHED-CENTRELINE", 3),
        ("FIT-MISSED-CENTRELINE", 1),
        ("FIT-MATCHED-DETECTED-EDGE", 4),
        ("FIT-MISSED-DETECTED-EDGE", 6),
        ("FIT-COVERED-DESIGN-AXIS", 3),
        ("FIT-UNCOVERED-DESIGN-AXIS", 2),
        ("FIT-COVERED-DESIGN-KERB", 5),
        ("FIT-UNCOVERED-DESIGN-KERB", 2),
    ])?;
    for (layer, points) in &report.evidence {
        polyline(&mut drawing, layer, points.iter().copied())?;
    }
    DxfWriter::new(&drawing).write_to_file(path)?;
    Ok(())
}

fn drawing(layers: &[(&str, i16)]) -> Result<CadDocument> {
    let mut drawing = CadDocument::with_version(DxfVersion::AC1032);
    drawing.header.insertion_units = 6;
    for &(name, index) in layers {
        let mut layer = Layer::new(name);
        layer.color = Color::from_index(index);
        drawing.layers.add(layer)?;
    }
    Ok(drawing)
}

fn polyline(
    drawing: &mut CadDocument,
    layer: &str,
    points: impl Iterator<Item = Point>,
) -> Result<()> {
    let mut vertices: Vec<_> = points
        .map(|[x, y]| LwVertex::with_bulge(Vector2::new(x, y), 0.0))
        .collect();
    if vertices.len() < 2 {
        return Ok(());
    }
    let is_closed = vertices.len() >= 4
        && vertices.first().map(|v| v.location) == vertices.last().map(|v| v.location);
    if is_closed {
        vertices.pop();
    }
    let common = EntityCommon {
        layer: layer.to_owned(),
        ..EntityCommon::default()
    };
    drawing.add_entity(EntityType::LwPolyline(LwPolyline {
        common,
        vertices,
        is_closed,
        ..LwPolyline::new()
    }))?;
    Ok(())
}
