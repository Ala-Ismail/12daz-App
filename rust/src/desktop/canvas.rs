use az12_converter::geometry::{Kind, Point};
use az12_converter::preview::PreviewRecord;
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, Vec2};

use super::theme::{LINE, MUTED, TEXT};

#[derive(Debug)]
pub struct View {
    origin: Point,
    pan: Vec2,
    scale: f64,
    fit_scale: f64,
    viewport_size: Vec2,
    pub cursor: Option<Point>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            origin: [0.0; 2],
            pan: Vec2::ZERO,
            scale: 1.0,
            fit_scale: 1.0,
            viewport_size: Vec2::ZERO,
            cursor: None,
        }
    }
}

impl View {
    pub fn fit(&mut self, size: Vec2, records: &[&PreviewRecord]) {
        let mut points = records
            .iter()
            .flat_map(|record| record.points.iter().copied());
        let Some(first) = points.next() else {
            *self = Self::default();
            self.viewport_size = size;
            return;
        };
        let (mut low, mut high) = (first, first);
        for point in points {
            for axis in 0..2 {
                low[axis] = low[axis].min(point[axis]);
                high[axis] = high[axis].max(point[axis]);
            }
        }
        self.origin = std::array::from_fn(|axis| low[axis] + (high[axis] - low[axis]) / 2.0);
        let (width, height) = ((high[0] - low[0]).max(1e-12), (high[1] - low[1]).max(1e-12));
        self.fit_scale = if low == high {
            1.0
        } else {
            ((f64::from((size.x - 72.0).max(1.0)) / width)
                .min(f64::from((size.y - 72.0).max(1.0)) / height))
            .min(1e8)
        };
        self.scale = self.fit_scale;
        self.pan = Vec2::ZERO;
        self.viewport_size = size;
    }

    pub fn zoom_percent(&self) -> u32 {
        (100.0 * self.scale / self.fit_scale).round() as u32
    }

    pub fn set_zoom_percent(&mut self, percent: u32) {
        self.zoom(
            f64::from(percent) / 100.0 * self.fit_scale / self.scale,
            Vec2::ZERO,
        );
    }

    pub fn zoom_step(&mut self, forward: bool) {
        self.zoom(if forward { 1.25 } else { 0.8 }, Vec2::ZERO);
    }

    fn zoom(&mut self, factor: f64, anchor: Vec2) {
        let new = (self.scale * factor).clamp(self.fit_scale / 1000.0, self.fit_scale * 1000.0);
        self.pan = anchor + (self.pan - anchor) * (new / self.scale) as f32;
        self.scale = new;
    }

    fn screen(&self, center: Pos2, point: Point) -> Pos2 {
        center
            + self.pan
            + Vec2::new(
                ((point[0] - self.origin[0]) * self.scale) as f32,
                ((self.origin[1] - point[1]) * self.scale) as f32,
            )
    }

    fn drawing(&self, center: Pos2, point: Pos2) -> Point {
        let offset = point - center - self.pan;
        [
            self.origin[0] + f64::from(offset.x) / self.scale,
            self.origin[1] - f64::from(offset.y) / self.scale,
        ]
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        records: &[&PreviewRecord],
        fit: bool,
        units: Option<&str>,
    ) {
        let boundary = ui.clip_rect().intersect(ui.ctx().content_rect());
        let available = ((boundary.max - ui.cursor().min).min(ui.available_size())
            - Vec2::new(0.0, 23.0))
        .max(Vec2::splat(32.0));
        let (response, painter) = ui.allocate_painter(available, egui::Sense::drag());
        let rect = response.rect;
        let center = rect.center();
        if fit || (self.viewport_size - rect.size()).length_sq() > 1.0 {
            self.fit(rect.size(), records);
        }
        if response.dragged() {
            self.pan += response.drag_delta();
        }
        self.cursor = response.hover_pos().map(|point| {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom((f64::from(scroll) / 400.0).exp(), point - center);
            }
            self.drawing(center, point)
        });
        painter.rect_filled(rect, 5.0, Color32::from_rgb(11, 21, 37));
        let grid = Color32::from_rgb(20, 34, 52);
        for i in 1..(rect.width() / 52.0) as usize {
            let x = rect.left() + i as f32 * 52.0;
            painter.vline(x, rect.y_range(), Stroke::new(1.0, grid));
        }
        for i in 1..(rect.height() / 52.0) as usize {
            let y = rect.top() + i as f32 * 52.0;
            painter.hline(rect.x_range(), y, Stroke::new(1.0, grid));
        }
        painter.rect_stroke(rect, 5.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
        if records.is_empty() {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "Select models to preview geometry",
                FontId::proportional(14.0),
                MUTED,
            );
            return;
        }
        for record in records {
            let colour = preview_colour(&record.colour);
            let points = record
                .points
                .iter()
                .map(|&point| self.screen(center, point));
            match record.kind {
                Kind::Point => {
                    for point in points {
                        let stroke = Stroke::new(1.0, colour);
                        painter.line_segment(
                            [point - Vec2::new(3.0, 0.0), point + Vec2::new(3.0, 0.0)],
                            stroke,
                        );
                        painter.line_segment(
                            [point - Vec2::new(0.0, 3.0), point + Vec2::new(0.0, 3.0)],
                            stroke,
                        );
                    }
                }
                Kind::Text => {
                    if let Some(point) = points.into_iter().next() {
                        painter.text(
                            point,
                            egui::Align2::LEFT_BOTTOM,
                            &record.text,
                            FontId::proportional(12.0),
                            colour,
                        );
                    }
                }
                _ => {
                    painter.add(egui::Shape::line(
                        points.collect(),
                        Stroke::new(1.3, colour),
                    ));
                }
            }
        }
        if rect.width() > 160.0 && rect.height() > 100.0 {
            let tip = rect.left_top() + Vec2::new(25.0, 16.0);
            painter.add(egui::Shape::convex_polygon(
                vec![tip, tip + Vec2::new(9.0, 23.0), tip + Vec2::new(-9.0, 23.0)],
                TEXT,
                Stroke::NONE,
            ));
            painter.text(
                tip + Vec2::new(0.0, 28.0),
                egui::Align2::CENTER_TOP,
                "+Y",
                FontId::proportional(11.0),
                TEXT,
            );
            self.scale_bar(&painter, rect, units);
        }
    }

    fn scale_bar(&self, painter: &egui::Painter, rect: Rect, units: Option<&str>) {
        let target = 116.0 / self.scale;
        let base = 10.0_f64.powf(target.log10().floor());
        let length = if target / base >= 5.0 {
            5.0 * base
        } else if target / base >= 2.0 {
            2.0 * base
        } else {
            base
        };
        let suffix = match units {
            Some("metres") => " m",
            Some("millimetres") => " mm",
            Some("feet") => " ft",
            Some("inches") => " in",
            _ => "",
        };
        let end = rect.right_bottom() - Vec2::new(26.0, 16.0);
        let start = end - Vec2::new((length * self.scale) as f32, 0.0);
        let stroke = Stroke::new(1.0, TEXT);
        painter.line_segment([start, end], stroke);
        for (x, label) in [
            (start.x, "0".into()),
            ((start.x + end.x) / 2.0, format!("{}", length / 2.0)),
            (end.x, format!("{length}{suffix}")),
        ] {
            painter.line_segment([Pos2::new(x, start.y - 5.0), Pos2::new(x, start.y)], stroke);
            painter.text(
                Pos2::new(x, start.y - 8.0),
                egui::Align2::CENTER_BOTTOM,
                label,
                FontId::proportional(11.0),
                TEXT,
            );
        }
    }
}

fn preview_colour(name: &str) -> Color32 {
    match name.to_ascii_lowercase().as_str() {
        "red" | "1" => Color32::from_rgb(255, 116, 116),
        "green" | "3" => Color32::from_rgb(104, 227, 92),
        "cyan" | "4" => Color32::from_rgb(118, 217, 250),
        "blue" | "5" => Color32::from_rgb(130, 157, 255),
        "yellow" | "2" => Color32::from_rgb(255, 219, 120),
        "magenta" | "6" => Color32::from_rgb(233, 161, 255),
        "gray" | "grey" | "8" => Color32::from_rgb(155, 170, 187),
        _ => Color32::from_rgb(229, 237, 246),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_14_transforms_large_survey_coordinates_and_anchors_zoom() {
        let geometry = PreviewRecord {
            model: "Survey".into(),
            kind: Kind::Polyline,
            points: vec![[500000.0, 6200000.0], [500001.25, 6200002.5]],
            colour: "green".into(),
            text: String::new(),
            closed: false,
        };
        let original = geometry.points.clone();
        let mut view = View::default();
        let size = Vec2::new(800.0, 450.0);
        let center = Pos2::new(size.x / 2.0, size.y / 2.0);
        view.fit(size, &[&geometry]);
        let xy = original[0];
        let screen = view.screen(center, xy);
        view.zoom(2.0, screen - center);
        assert_eq!(view.screen(center, xy), screen);
        let projected = view.drawing(center, screen);
        assert!((projected[0] - xy[0]).abs() < 1e-6 && (projected[1] - xy[1]).abs() < 1e-6);
        assert_eq!(geometry.points, original);
    }
}
