use serde::{Deserialize, Serialize};

use super::{Coordinate, CrsId, MapError, Projection, Registration, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceUnit {
    Metres,
    Millimetres,
    Feet,
    Inches,
}

impl SourceUnit {
    pub const ALL: [Self; 4] = [Self::Metres, Self::Millimetres, Self::Feet, Self::Inches];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Metres => "metres",
            Self::Millimetres => "millimetres",
            Self::Feet => "feet",
            Self::Inches => "inches",
        }
    }

    fn metres(self) -> f64 {
        match self {
            Self::Metres => 1.0,
            Self::Millimetres => 0.001,
            Self::Feet => 0.3048,
            Self::Inches => 0.0254,
        }
    }

    fn project(self, point: Coordinate) -> Result<Coordinate> {
        let scale = self.metres();
        Coordinate::new(point.x() * scale, point.y() * scale)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapRegistration {
    units: SourceUnit,
    registration: Registration,
}

impl MapRegistration {
    pub fn assign(crs: CrsId, units: SourceUnit) -> Result<Self> {
        if crs.mga_details().is_none() {
            return Err(MapError::TransformationFailed);
        }
        let origin = Coordinate::new(0.0, 0.0)?;
        Ok(Self {
            units,
            registration: Registration::new(crs, origin, origin, 1.0, 0.0)?,
        })
    }

    pub fn align(
        crs: CrsId,
        units: SourceUnit,
        first: (Coordinate, Coordinate),
        second: Option<(Coordinate, Coordinate)>,
    ) -> Result<Self> {
        if crs.mga_details().is_none() {
            return Err(MapError::TransformationFailed);
        }
        let origin = units.project(first.0)?;
        let projection = Projection::new(crs)?;
        projection.to_wgs84(first.1)?;
        let (scale, rotation) = if let Some(second) = second {
            let drawing = units.project(second.0)?;
            projection.to_wgs84(second.1)?;
            let dx = drawing.x() - origin.x();
            let dy = drawing.y() - origin.y();
            let px = second.1.x() - first.1.x();
            let py = second.1.y() - first.1.y();
            let drawing_length = dx.hypot(dy);
            let project_length = px.hypot(py);
            if drawing_length <= 1e-9 || project_length <= 1e-9 {
                return Err(MapError::InvalidControlPoints);
            }
            (project_length / drawing_length, py.atan2(px) - dy.atan2(dx))
        } else {
            (1.0, 0.0)
        };
        Ok(Self {
            units,
            registration: Registration::new(crs, origin, first.1, scale, rotation)?,
        })
    }

    pub fn crs(self) -> CrsId {
        self.registration.crs()
    }

    pub fn units(self) -> SourceUnit {
        self.units
    }

    pub fn scale(self) -> f64 {
        self.registration.scale()
    }

    pub fn rotation(self) -> f64 {
        self.registration.rotation()
    }

    pub fn to_project(self, point: Coordinate) -> Result<Coordinate> {
        self.registration.to_project(self.units.project(point)?)
    }

    pub fn to_drawing(self, point: Coordinate) -> Result<Coordinate> {
        let point = self.registration.to_drawing(point)?;
        let scale = self.units.metres();
        Coordinate::new(point.x() / scale, point.y() / scale)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> Coordinate {
        Coordinate::new(x, y).unwrap()
    }

    #[test]
    fn map_01_mga_matches_independent_proj_references() {
        // PROJ 9.5.1 via pyproj 3.7.2, EPSG projected CRS to EPSG:7844 with always_xy.
        // GDA94 uses EPSG:8048. This verifies the transform, not basemap imagery accuracy.
        for (crs, easting, northing, longitude, latitude) in [
            (
                CrsId::Gda2020MgaZone55,
                321000.0,
                5812000.0,
                144.96623032504206,
                -37.821870125361194,
            ),
            (
                CrsId::Gda2020MgaZone56,
                334368.0,
                6250944.0,
                151.2092923341389,
                -33.86883907372656,
            ),
            (
                CrsId::Gda94MgaZone55,
                321000.0,
                5812000.0,
                144.96623626027593,
                -37.82185696303591,
            ),
            (
                CrsId::Gda94MgaZone50,
                392000.0,
                6464000.0,
                115.85720665752436,
                -31.954825244797924,
            ),
        ] {
            let projection = Projection::new(crs).unwrap();
            let geo = projection.to_wgs84(point(easting, northing)).unwrap();
            assert!(
                (geo.longitude() - longitude).abs() < 5e-8,
                "{crs:?}: {geo:?}"
            );
            assert!((geo.latitude() - latitude).abs() < 5e-8, "{crs:?}: {geo:?}");
            let grid = projection
                .from_wgs84(super::super::LonLat::new(longitude, latitude).unwrap())
                .unwrap();
            assert!((grid.x() - easting).abs() < 0.01, "{crs:?}: {grid:?}");
            assert!((grid.y() - northing).abs() < 0.01, "{crs:?}: {grid:?}");
        }
    }

    #[test]
    fn map_02_source_units_only_scale_the_map_registration() {
        let metres = MapRegistration::assign(CrsId::Gda2020MgaZone55, SourceUnit::Metres).unwrap();
        let millimetres =
            MapRegistration::assign(CrsId::Gda2020MgaZone55, SourceUnit::Millimetres).unwrap();
        let drawing = point(321000000.0, 5812000000.0);
        let expected = metres.to_project(point(321000.0, 5812000.0)).unwrap();
        assert_eq!(millimetres.to_project(drawing).unwrap(), expected);
        assert_eq!(millimetres.to_drawing(expected).unwrap(), drawing);
    }

    #[test]
    fn map_04_and_05_control_points_place_an_independent_point() {
        let crs = CrsId::Gda2020MgaZone55;
        let first = (point(100.0, 200.0), point(300000.0, 5800000.0));
        let second = (point(110.0, 200.0), point(300000.0, 5800020.0));
        let independent = point(100.0, 210.0);
        let translated = MapRegistration::align(crs, SourceUnit::Metres, first, None).unwrap();
        assert_eq!(
            translated.to_project(independent).unwrap(),
            point(300000.0, 5800010.0)
        );
        let aligned = MapRegistration::align(crs, SourceUnit::Metres, first, Some(second)).unwrap();
        assert_eq!(aligned.scale(), 2.0);
        assert!((aligned.rotation().to_degrees() - 90.0).abs() < 1e-10);
        assert_eq!(
            aligned.to_project(independent).unwrap(),
            point(299980.0, 5800000.0)
        );
        assert_eq!(aligned.to_drawing(second.1).unwrap(), second.0);
        assert_eq!(
            MapRegistration::align(crs, SourceUnit::Metres, first, Some(first)),
            Err(MapError::InvalidControlPoints)
        );
    }
}
