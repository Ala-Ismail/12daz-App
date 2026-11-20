//! DXF 2018 export and validation of the finished temporary drawing.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use acadrust::entities::{
    EntityCommon, LwVertex, Polyline3D, TextHorizontalAlignment, TextVerticalAlignment,
};
use acadrust::{
    CadDocument, Circle, Color, DxfReader, DxfVersion, EntityType, Layer, LwPolyline, Point, Text,
    Vector2, Vector3,
};
use unicode_casefold::UnicodeCaseFold;

use crate::error::{ConversionError, Result, checkpoint};
use crate::geometry::{Kind, Primitive, Warning, repr};

fn write_error(error: impl std::fmt::Display) -> ConversionError {
    ConversionError::message(format!("Cannot write DXF: {error}"))
}

const INVALID_GEOMETRY: &str = "Output geometry has invalid coordinates or nonzero Z.";

pub struct DxfWriter {
    document: CadDocument,
    layers: BTreeMap<String, String>,
    used: HashSet<String>,
    count: usize,
}

impl DxfWriter {
    pub fn new(unit: Option<&str>) -> Result<Self> {
        let code = match unit {
            Some("metres") => 6,
            Some("millimetres") => 4,
            Some("feet") => 2,
            Some("inches") => 1,
            _ => {
                return Err(ConversionError::message(
                    "Choose units: metres, millimetres, feet or inches. Unit declaration does not rescale coordinates.",
                ));
            }
        };
        let mut document = CadDocument::with_version(DxfVersion::AC1032);
        document.header.insertion_units = code;
        document.header.point_display_mode = 3;
        document.header.point_display_size = -2.0;
        Ok(Self {
            document,
            layers: BTreeMap::new(),
            used: HashSet::from(["0".into(), "defpoints".into()]),
            count: 0,
        })
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn layers(&self) -> &BTreeMap<String, String> {
        &self.layers
    }

    fn layer(&mut self, model: &str) -> Result<String> {
        if let Some(name) = self.layers.get(model) {
            return Ok(name.clone());
        }
        let sanitized: String = model
            .chars()
            .map(|c| {
                if c < ' ' || "<>/\\\":;?*|=".contains(c) {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        let mut base: String = sanitized.trim().chars().take(240).collect();
        if base.is_empty() {
            base = "Model".into();
        }
        let mut chosen = base.clone();
        let mut suffix = 2;
        while self.used.contains(&chosen.case_fold().collect::<String>()) {
            chosen = format!("{base}_{suffix}");
            suffix += 1;
        }
        self.document
            .layers
            .add(Layer::new(&chosen))
            .map_err(write_error)?;
        self.used.insert(chosen.case_fold().collect());
        self.layers.insert(model.into(), chosen.clone());
        Ok(chosen)
    }

    pub fn write(
        &mut self,
        primitive: &Primitive,
        model: &str,
        warn: &mut Warning<'_>,
    ) -> Result<()> {
        let layer = self.layer(model)?;
        let colour = match primitive.colour.to_lowercase().as_str() {
            "red" => 1,
            "yellow" => 2,
            "green" => 3,
            "cyan" => 4,
            "blue" => 5,
            "magenta" => 6,
            "black" | "white" => 7,
            "grey" | "gray" => 8,
            "no_colour" => 256,
            other => match other.parse::<i16>() {
                Ok(value @ 1..=255) if other.bytes().all(|value| value.is_ascii_digit()) => value,
                _ => {
                    warn(
                        "COLOUR_SUBSTITUTION",
                        &format!(
                            "Unresolved colour {}; using white/black CAD index 7.",
                            repr(&primitive.colour)
                        ),
                    );
                    7
                }
            },
        };
        if !matches!(primitive.style.to_lowercase().as_str(), "1" | "continuous") {
            warn(
                "LINESTYLE_SUBSTITUTION",
                &format!(
                    "Unresolved style {}; using CONTINUOUS.",
                    repr(&primitive.style)
                ),
            );
        }
        let common = EntityCommon {
            layer,
            color: Color::from_index(colour),
            ..EntityCommon::new()
        };
        let z = primitive.elevations.first().copied().unwrap_or_default();
        let [x, y] = *primitive
            .points
            .first()
            .ok_or_else(|| ConversionError::message("Empty output primitive."))?;
        let position = Vector3::new(x, y, z);
        let entity = match primitive.kind {
            Kind::Point => EntityType::Point(Point {
                common,
                location: position,
                ..Point::new()
            }),
            Kind::Circle => EntityType::Circle(Circle {
                common,
                center: position,
                radius: primitive.radius,
                ..Circle::new()
            }),
            Kind::Polyline if primitive.elevations.iter().any(|level| *level != z) => {
                if primitive.points.len() != primitive.elevations.len()
                    || primitive.bulges.iter().any(|&bulge| bulge != 0.0)
                {
                    return Err(ConversionError::message(
                        "Invalid varying-elevation output polyline.",
                    ));
                }
                let points = primitive
                    .points
                    .iter()
                    .zip(&primitive.elevations)
                    .map(|(&[x, y], &z)| Vector3::new(x, y, z))
                    .collect();
                let mut polyline = Polyline3D::from_points(points);
                polyline.common = common;
                if primitive.closed {
                    polyline = polyline.with_closed();
                }
                EntityType::Polyline3D(polyline)
            }
            Kind::Polyline => {
                let vertices = primitive
                    .points
                    .iter()
                    .enumerate()
                    .map(|(i, &[x, y])| {
                        LwVertex::with_bulge(
                            Vector2::new(x, y),
                            primitive.bulges.get(i).copied().unwrap_or_default(),
                        )
                    })
                    .collect();
                EntityType::LwPolyline(LwPolyline {
                    common,
                    vertices,
                    is_closed: primitive.closed,
                    elevation: z,
                    ..LwPolyline::new()
                })
            }
            Kind::Text => {
                let (vertical, horizontal) = primitive.justify.split_once('-').unwrap_or_default();
                let vertical_alignment = match vertical {
                    "top" => TextVerticalAlignment::Top,
                    "middle" => TextVerticalAlignment::Middle,
                    _ => TextVerticalAlignment::Bottom,
                };
                let horizontal_alignment = match horizontal {
                    "centre" => TextHorizontalAlignment::Center,
                    "right" => TextHorizontalAlignment::Right,
                    _ => TextHorizontalAlignment::Left,
                };
                EntityType::Text(Text {
                    common,
                    value: primitive.text.clone(),
                    insertion_point: position,
                    alignment_point: Some(position),
                    height: primitive.height,
                    width_factor: primitive.width,
                    rotation: primitive.rotation.to_radians(),
                    oblique_angle: primitive.slant.to_radians(),
                    vertical_alignment,
                    horizontal_alignment,
                    ..Text::new()
                })
            }
        };
        self.document.add_entity(entity).map_err(write_error)?;
        self.count += 1;
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        acadrust::DxfWriter::new(&self.document)
            .write_to_file(path)
            .map_err(write_error)
    }
}

fn planar(z: f64, keep_z: bool) -> bool {
    z.is_finite() && (keep_z || z == 0.0)
}

fn xyz(point: Vector3, keep_z: bool) -> bool {
    point.x.is_finite() && point.y.is_finite() && planar(point.z, keep_z)
}

pub fn validate_dxf(
    path: &Path,
    count: usize,
    keep_z: bool,
    cancel: Option<&AtomicBool>,
) -> Result<()> {
    checkpoint(cancel)?;
    let audit_error = || ConversionError::message("Output DXF failed audit or required repair.");
    let reopened = DxfReader::from_file(path)
        .map_err(|_| audit_error())?
        .read_with_stats()
        .map_err(|_| audit_error())?;
    if !reopened.stats.stream_completed
        || reopened.stats.recovered()
        || reopened.stats.skipped_source_records != 0
    {
        return Err(audit_error());
    }
    let document = reopened.document;
    if document.version != DxfVersion::AC1032 {
        return Err(ConversionError::message("Unexpected DXF version."));
    }
    let mut seen = 0;
    for entity in document.model_space_entities() {
        checkpoint(cancel)?;
        seen += 1;
        let (normal, thickness, valid) = match entity {
            EntityType::Point(point) => {
                (point.normal, point.thickness, xyz(point.location, keep_z))
            }
            EntityType::Circle(circle) => (
                circle.normal,
                circle.thickness,
                xyz(circle.center, keep_z) && circle.radius > 0.0 && circle.radius.is_finite(),
            ),
            EntityType::Text(text) => (
                text.normal,
                text.thickness,
                xyz(text.insertion_point, keep_z)
                    && text.alignment_point.is_none_or(|point| xyz(point, keep_z))
                    && text.height > 0.0
                    && text.height.is_finite(),
            ),
            EntityType::LwPolyline(path) => {
                if !planar(path.elevation, keep_z) {
                    return Err(ConversionError::message("Invalid output elevation."));
                }
                let valid = path.vertices.iter().all(|vertex| {
                    vertex.location.x.is_finite()
                        && vertex.location.y.is_finite()
                        && vertex.bulge.is_finite()
                });
                (path.normal, path.thickness, valid)
            }
            EntityType::Polyline3D(path) if keep_z => {
                let valid = path.elevation.is_finite()
                    && path
                        .vertices
                        .iter()
                        .all(|vertex| xyz(vertex.position, true));
                (path.normal, 0.0, valid)
            }
            EntityType::Polyline3D(_) => {
                return Err(ConversionError::message("Unexpected 3D polyline."));
            }
            _ => return Err(ConversionError::message("Unexpected output entity.")),
        };
        if thickness != 0.0 {
            return Err(ConversionError::message("Invalid output thickness."));
        }
        if !valid {
            return Err(ConversionError::message(INVALID_GEOMETRY));
        }
        if normal != Vector3::UNIT_Z {
            return Err(ConversionError::message("Nonstandard output extrusion."));
        }
    }
    if seen != count {
        return Err(ConversionError::message(
            "DXF entity count changed during validation.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_08_validates_written_entities_and_deterministic_layers() {
        let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let mut writer = DxfWriter::new(Some("metres")).unwrap();
        let mut warnings = Vec::new();
        let mut warn = |code: &str, _message: &str| warnings.push(code.to_owned());
        let point = Primitive::new(Kind::Point, vec![[1.0, 2.0]], "unresolved", "broken");
        writer.write(&point, "Road?", &mut warn).unwrap();
        let circle = Primitive {
            radius: 3.0,
            ..Primitive::new(Kind::Circle, vec![[2.0, 3.0]], "green", "1")
        };
        writer.write(&circle, "road*", &mut warn).unwrap();
        assert_eq!(warnings, ["COLOUR_SUBSTITUTION", "LINESTYLE_SUBSTITUTION"]);
        assert_eq!(writer.layers()["Road?"], "Road_");
        assert_eq!(writer.layers()["road*"], "road__2");
        writer.save(&path).unwrap();
        validate_dxf(&path, 2, false, None).unwrap();
        assert!(validate_dxf(&path, 3, false, None).is_err());
    }

    #[test]
    fn rust_08_rejects_wrong_version_elevation_thickness_extrusion_and_truncation() {
        let path = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let mut elevated = Primitive::new(Kind::Point, vec![[1.0, 2.0]], "green", "1");
        elevated.elevations = vec![12.0];
        let mut writer = DxfWriter::new(Some("feet")).unwrap();
        writer.write(&elevated, "Road", &mut |_, _| {}).unwrap();
        writer.save(&path).unwrap();
        assert!(validate_dxf(&path, 1, false, None).is_err());
        validate_dxf(&path, 1, true, None).unwrap();
        for (version, thickness, normal) in [
            (DxfVersion::AC1027, 0.0, Vector3::UNIT_Z),
            (DxfVersion::AC1032, 2.0, Vector3::UNIT_Z),
            (DxfVersion::AC1032, 0.0, Vector3::UNIT_X),
        ] {
            let mut document = CadDocument::with_version(version);
            document
                .add_entity(EntityType::Point(Point {
                    normal,
                    thickness,
                    ..Point::new()
                }))
                .unwrap();
            acadrust::DxfWriter::new(&document)
                .write_to_file(&path)
                .unwrap();
            assert!(validate_dxf(&path, 1, false, None).is_err());
        }
        std::fs::write(&path, b"0\nSECTION\n2\nENTITIES\n0\nPOINT\n").unwrap();
        assert!(validate_dxf(&path, 1, false, None).is_err());
    }
}
