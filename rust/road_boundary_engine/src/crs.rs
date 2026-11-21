use proj4rs::{Proj, transform::transform};

use crate::Point;

fn definition(id: &str) -> Result<(String, bool), String> {
    if id == "EPSG:4326" {
        return Ok(("+proj=longlat +datum=WGS84".into(), true));
    }
    let code = id
        .strip_prefix("EPSG:")
        .and_then(|code| code.parse::<u16>().ok());
    let (conversion, legacy) = match code {
        Some(code @ 7846..=7856) => (format!("+proj=utm +zone={} +south", code - 7800), false),
        Some(code @ 28346..=28356) => (format!("+proj=utm +zone={} +south", code - 28300), true),
        Some(3113) => (
            "+proj=tmerc +lat_0=-28 +lon_0=153 +k=0.99999 +x_0=50000 +y_0=100000".to_owned(),
            true,
        ),
        Some(20047) => (
            "+proj=tmerc +lat_0=-28 +lon_0=153 +k=0.99999 +x_0=150000 +y_0=200000".to_owned(),
            false,
        ),
        _ => {
            return Err(format!(
                "unsupported CRS {id}; use EPSG:4326, MGA zones 46–56 or Brisbane City Survey Grid"
            ));
        }
    };
    // Match the backdrop's EPSG:8048 coordinate-frame conversion and display approximation.
    let shift = if legacy {
        "0.06155,-0.01087,-0.04019,0.0394924,0.0327221,0.0328979,-0.009994"
    } else {
        "0,0,0,0,0,0,0"
    };
    Ok((
        format!("{conversion} +ellps=GRS80 +towgs84={shift} +units=m"),
        false,
    ))
}

pub(crate) struct Projection {
    from: Proj,
    to: Proj,
    geographic: bool,
    identity: bool,
}

impl Projection {
    pub(crate) fn new(input: &str, output: &str) -> Result<Self, String> {
        let (from, geographic) = definition(input)?;
        let (to, output_geographic) = definition(output)?;
        if output_geographic {
            return Err("output CRS must be a projected metre CRS".into());
        }
        let read =
            |definition: &str| Proj::from_proj_string(definition).map_err(|e| format!("CRS: {e}"));
        Ok(Self {
            from: read(&from)?,
            to: read(&to)?,
            geographic,
            identity: input == output,
        })
    }

    pub(crate) fn point(&self, [x, y]: Point) -> Result<Point, String> {
        if !x.is_finite() || !y.is_finite() {
            return Err("coordinates must be finite".into());
        }
        if self.geographic && (!(-180.0..=180.0).contains(&x) || !(-90.0..=90.0).contains(&y)) {
            return Err("longitude or latitude out of bounds".into());
        }
        if self.identity {
            return metre_point([x, y]);
        }
        let mut value = if self.geographic {
            (x.to_radians(), y.to_radians(), 0.0)
        } else {
            (x, y, 0.0)
        };
        transform(&self.from, &self.to, &mut value)
            .map_err(|e| format!("coordinate projection failed: {e}"))?;
        metre_point([value.0, value.1])
    }
}

fn metre_point(point: Point) -> Result<Point, String> {
    if point
        .iter()
        .all(|value| (-40_000_000.0..=40_000_000.0).contains(value))
    {
        Ok(point)
    } else {
        Err("point is outside the supported projected coordinate range".into())
    }
}
