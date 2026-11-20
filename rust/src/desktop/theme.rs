use eframe::egui::{self, Color32, FontId, Frame, Margin, RichText, Stroke, TextStyle, Ui, Vec2};

pub const SHELL: Color32 = Color32::from_rgb(24, 30, 39);
pub const PANEL: Color32 = Color32::from_rgb(34, 42, 54);
pub const FIELD: Color32 = Color32::from_rgb(26, 33, 44);
pub const LINE: Color32 = Color32::from_rgb(61, 75, 94);
pub const TEXT: Color32 = Color32::from_rgb(231, 237, 248);
pub const MUTED: Color32 = Color32::from_rgb(163, 181, 205);
pub const BLUE: Color32 = Color32::from_rgb(30, 106, 213);
pub const LINK: Color32 = Color32::from_rgb(132, 184, 255);
pub const GREEN: Color32 = Color32::from_rgb(89, 213, 160);
pub const AMBER: Color32 = Color32::from_rgb(255, 199, 109);

pub fn install(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(17.0));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(11.0));
        style.spacing.item_spacing = Vec2::new(7.0, 7.0);
        style.spacing.button_padding = Vec2::new(10.0, 4.0);
        style.spacing.interact_size.y = 25.0;
        style.visuals.panel_fill = SHELL;
        style.visuals.window_fill = PANEL;
        style.visuals.text_edit_bg_color = Some(FIELD);
        style.visuals.hyperlink_color = LINK;
        style.visuals.selection.bg_fill = BLUE;
        style.visuals.selection.stroke = Stroke::new(1.5, Color32::WHITE);
        let widgets = &mut style.visuals.widgets;
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        for (visual, fill, outline) in [
            (&mut widgets.inactive, Color32::from_rgb(46, 57, 74), LINE),
            (&mut widgets.hovered, Color32::from_rgb(58, 75, 99), LINK),
            (&mut widgets.active, BLUE, LINK),
            (&mut widgets.open, Color32::from_rgb(46, 57, 74), LINK),
        ] {
            visual.bg_fill = fill;
            visual.weak_bg_fill = fill;
            visual.bg_stroke = Stroke::new(1.0, outline);
            visual.fg_stroke = Stroke::new(1.4, TEXT);
            visual.corner_radius = 5.into();
        }
    });
}

pub fn panel() -> Frame {
    Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(8)
        .outer_margin(Margin::symmetric(4, 4))
        .inner_margin(Margin::same(10))
}

pub fn field() -> Frame {
    Frame::new()
        .fill(FIELD)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(5)
        .inner_margin(Margin::symmetric(9, 5))
}

pub fn heading(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).heading().strong().color(TEXT));
}
