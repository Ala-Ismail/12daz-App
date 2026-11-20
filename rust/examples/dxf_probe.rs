//! Writes a representative DXF using the conversion backend.

use std::error::Error;
use std::path::PathBuf;

use az12_converter::dxf::{DxfWriter, validate_dxf};
use az12_converter::geometry::{Kind, Primitive};

fn main() -> Result<(), Box<dyn Error>> {
    let path = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Supply the output DXF path.")?,
    );
    let mut writer = DxfWriter::new(Some("metres"))?;
    let label = Primitive {
        text: "Café 測量".into(),
        height: 0.5,
        rotation: 30.0,
        justify: "top-centre".into(),
        elevations: vec![7.0],
        ..Primitive::new(Kind::Text, vec![[5.0, 8.0]], "red", "1")
    };
    let level = Primitive {
        bulges: vec![-0.5, 0.0],
        elevations: vec![12.0, 12.0],
        ..Primitive::new(
            Kind::Polyline,
            vec![[500000.0, 6200000.0], [500001.25, 6200002.5]],
            "green",
            "1",
        )
    };
    let graded = Primitive {
        elevations: vec![12.0, 16.0],
        ..Primitive::new(Kind::Polyline, vec![[1.0, 2.0], [3.0, 4.0]], "blue", "1")
    };
    for primitive in [label, level, graded] {
        writer.write(&primitive, "Survey", &mut |code, message| {
            panic!("{code}: {message}")
        })?;
    }
    writer.save(&path)?;
    validate_dxf(&path, 3, true, None)?;
    Ok(())
}
