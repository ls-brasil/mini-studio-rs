use eframe::egui::*;

pub const BG: Color32 = Color32::from_rgb(14, 18, 24);
pub const PANEL: Color32 = Color32::from_rgb(20, 25, 33);
pub const CARD: Color32 = Color32::from_rgb(28, 35, 44);
pub const HOVER: Color32 = Color32::from_rgb(38, 48, 59);
pub const LINE: Color32 = Color32::from_rgb(43, 54, 65);
pub const TEXT: Color32 = Color32::from_rgb(237, 242, 246);
pub const MUTED: Color32 = Color32::from_rgb(151, 166, 181);
pub const ACCENT: Color32 = Color32::from_rgb(82, 222, 183);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(21, 58, 51);
pub const AMBER: Color32 = Color32::from_rgb(239, 194, 115);

#[derive(Clone, Copy)]
pub enum Icon {
    Monitor,
    Window,
    Camera,
    Image,
    Eye,
    Hidden,
    Lock,
    Unlock,
    Trash,
    Play,
    Stop,
    Settings,
    Help,
    Close,
    Grid,
    Folder,
    Mirror,
    Refresh,
}
#[derive(Clone, Copy, PartialEq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Quiet,
}

pub fn configure(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    if let Some(windows) = std::env::var_os("WINDIR") {
        let path = std::path::PathBuf::from(windows).join("Fonts");
        for (name, file) in [("Segoe", "segoeui.ttf"), ("Segoe Semibold", "seguisb.ttf")] {
            if let Ok(bytes) = std::fs::read(path.join(file)) {
                fonts
                    .font_data
                    .insert(name.into(), FontData::from_owned(bytes).into());
                if name == "Segoe" {
                    fonts
                        .families
                        .entry(FontFamily::Proportional)
                        .or_default()
                        .insert(0, name.into());
                } else {
                    fonts
                        .families
                        .insert(FontFamily::Name("semibold".into()), vec![name.into()]);
                }
            }
        }
    }
    if !fonts
        .families
        .contains_key(&FontFamily::Name("semibold".into()))
    {
        fonts.families.insert(
            FontFamily::Name("semibold".into()),
            fonts.families[&FontFamily::Proportional].clone(),
        );
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(23.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    style.spacing.item_spacing = vec2(10.0, 10.0);
    style.spacing.button_padding = vec2(14.0, 10.0);
    style.spacing.interact_size = vec2(36.0, 34.0);
    style.spacing.slider_width = 120.0;
    style.spacing.scroll = style::ScrollStyle::thin();
    style.spacing.scroll.foreground_color = true;
    style.visuals = Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = BG;
    style.visuals.faint_bg_color = CARD;
    style.visuals.window_stroke = Stroke::new(1.0_f32, LINE);
    style.visuals.window_corner_radius = CornerRadius::same(16);
    style.visuals.selection.bg_fill = ACCENT_DARK;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.hyperlink_color = ACCENT;
    for widgets in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widgets.corner_radius = CornerRadius::same(8);
        widgets.bg_fill = CARD;
        widgets.weak_bg_fill = CARD;
        widgets.bg_stroke = Stroke::new(1.0_f32, LINE);
        widgets.fg_stroke = Stroke::new(1.4_f32, TEXT);
    }
    style.visuals.widgets.hovered.bg_fill = HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = HOVER;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MUTED);
    style.visuals.widgets.active.bg_fill = ACCENT_DARK;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, ACCENT);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, MUTED);
    style.animation_time = 0.15;
    ctx.set_style(style);
}

pub fn title(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text)
        .font(FontId::new(size, FontFamily::Name("semibold".into())))
        .color(TEXT)
}
pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(MUTED).size(12.0)
}
pub fn section(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .color(MUTED)
            .size(11.0)
            .extra_letter_spacing(1.5),
    );
}
pub fn card() -> Frame {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(12)
        .inner_margin(16)
}
pub fn modal_frame() -> Frame {
    Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(18)
        .inner_margin(28)
        .shadow(epaint::Shadow {
            offset: [0, 12],
            blur: 40,
            spread: 0,
            color: Color32::from_black_alpha(100),
        })
}

pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.into(), FontId::proportional(11.0), color);
    let (rect, _) = ui.allocate_exact_size(vec2(galley.size().x + 18.0, 24.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 6, color.gamma_multiply(0.12));
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
}

pub fn button(ui: &mut Ui, icon: Icon, text: &str, kind: ButtonKind, width: f32) -> Response {
    let height = 40.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), text));
    let hovered = response.hovered() || response.has_focus();
    let (fill, fg) = match kind {
        ButtonKind::Primary => (
            if hovered {
                Color32::from_rgb(115, 239, 204)
            } else {
                ACCENT
            },
            BG,
        ),
        ButtonKind::Secondary => (if hovered { HOVER } else { CARD }, TEXT),
        ButtonKind::Quiet => (if hovered { HOVER } else { Color32::TRANSPARENT }, MUTED),
    };
    let opacity = if ui.is_enabled() { 1.0 } else { 0.35 };
    let p = ui.painter();
    p.rect_filled(rect, 8, fill.gamma_multiply(opacity));
    if kind != ButtonKind::Quiet {
        p.rect_stroke(
            rect,
            8,
            Stroke::new(
                1.0_f32,
                if response.has_focus() {
                    ACCENT
                } else {
                    if kind == ButtonKind::Primary {
                        fill
                    } else {
                        LINE
                    }
                },
            ),
            StrokeKind::Inside,
        );
    }
    let font = FontId::proportional(13.0);
    let galley = p.layout_no_wrap(text.into(), font, fg.gamma_multiply(opacity));
    let total = if text.is_empty() {
        18.0
    } else {
        galley.size().x + 28.0
    };
    let x = rect.center().x - total / 2.0;
    paint_icon(
        p,
        icon,
        Rect::from_center_size(pos2(x + 9.0, rect.center().y), vec2(18.0, 18.0)),
        fg.gamma_multiply(opacity),
    );
    if !text.is_empty() {
        p.galley(
            pos2(x + 28.0, rect.center().y - galley.size().y / 2.0),
            galley,
            fg.gamma_multiply(opacity),
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn icon_button(ui: &mut Ui, icon: Icon, label: &str) -> Response {
    let response = button(ui, icon, "", ButtonKind::Quiet, 32.0);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
    response.on_hover_text(label)
}

pub fn logo(p: &Painter, rect: Rect) {
    p.rect_filled(rect, 12, ACCENT);
    let a = rect.shrink(rect.width() * 0.24);
    p.rect_stroke(
        a,
        4,
        Stroke::new(rect.width() * 0.065, BG),
        StrokeKind::Inside,
    );
    p.circle_filled(a.center(), rect.width() * 0.09, BG);
    p.circle_filled(
        pos2(
            a.right() - rect.width() * 0.08,
            a.top() + rect.width() * 0.08,
        ),
        rect.width() * 0.025,
        BG,
    );
}

pub fn paint_icon(p: &Painter, icon: Icon, r: Rect, color: Color32) {
    let pt = |x: f32, y: f32| {
        pos2(
            r.left() + x * r.width() / 24.0,
            r.top() + y * r.height() / 24.0,
        )
    };
    let stroke = Stroke::new((r.width() / 14.0).max(1.2), color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([pt(a.0, a.1), pt(b.0, b.1)], stroke);
    };
    let box_ = |x: f32, y: f32, w: f32, h: f32| {
        p.rect_stroke(
            Rect::from_min_max(pt(x, y), pt(x + w, y + h)),
            2,
            stroke,
            StrokeKind::Inside,
        );
    };
    match icon {
        Icon::Lock | Icon::Unlock => {
            box_(5.0, 10.0, 14.0, 11.0);
            line((8.0, 10.0), (8.0, 4.0));
            line((8.0, 4.0), (16.0, 4.0));
            line(
                (16.0, 4.0),
                (
                    16.0,
                    if matches!(icon, Icon::Lock) {
                        10.0
                    } else {
                        7.0
                    },
                ),
            );
            line((12.0, 14.0), (12.0, 17.0));
        }
        Icon::Monitor => {
            box_(2.0, 3.0, 20.0, 14.0);
            line((12.0, 17.0), (12.0, 21.0));
            line((7.0, 21.0), (17.0, 21.0));
        }
        Icon::Window => {
            box_(2.0, 3.0, 20.0, 18.0);
            line((3.0, 8.0), (21.0, 8.0));
            p.circle_filled(pt(6.0, 5.5), 0.8, color);
        }
        Icon::Camera => {
            box_(2.0, 6.0, 15.0, 13.0);
            line((17.0, 10.0), (22.0, 7.0));
            line((22.0, 7.0), (22.0, 18.0));
            line((22.0, 18.0), (17.0, 15.0));
        }
        Icon::Image => {
            box_(2.0, 2.0, 20.0, 20.0);
            p.circle_stroke(pt(8.0, 8.0), r.width() / 10.0, stroke);
            line((3.0, 19.0), (10.0, 13.0));
            line((10.0, 13.0), (14.0, 16.0));
            line((14.0, 16.0), (19.0, 11.0));
            line((19.0, 11.0), (21.0, 14.0));
        }
        Icon::Eye | Icon::Hidden => {
            p.add(Shape::closed_line(
                vec![
                    pt(2.0, 12.0),
                    pt(7.0, 7.0),
                    pt(17.0, 7.0),
                    pt(22.0, 12.0),
                    pt(17.0, 17.0),
                    pt(7.0, 17.0),
                ],
                stroke,
            ));
            p.circle_stroke(pt(12.0, 12.0), r.width() / 8.0, stroke);
            if matches!(icon, Icon::Hidden) {
                line((2.0, 2.0), (22.0, 22.0));
            }
        }
        Icon::Trash => {
            line((3.0, 6.0), (21.0, 6.0));
            box_(6.0, 6.0, 12.0, 15.0);
            line((9.0, 3.0), (15.0, 3.0));
            line((10.0, 10.0), (10.0, 17.0));
            line((14.0, 10.0), (14.0, 17.0));
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(
                vec![pt(7.0, 4.0), pt(21.0, 12.0), pt(7.0, 20.0)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_min_max(pt(5.0, 5.0), pt(19.0, 19.0)), 2, color);
        }
        Icon::Settings => {
            for (y, x) in [(5.0, 9.0), (12.0, 16.0), (19.0, 7.0)] {
                line((3.0, y), (21.0, y));
                p.circle_filled(pt(x, y), r.width() / 7.0, PANEL);
                p.circle_stroke(pt(x, y), r.width() / 7.0, stroke);
            }
        }
        Icon::Help => {
            p.circle_stroke(r.center(), r.width() * 0.43, stroke);
            p.text(
                r.center(),
                Align2::CENTER_CENTER,
                "?",
                FontId::proportional(r.height() * 0.8),
                color,
            );
        }
        Icon::Close => {
            line((6.0, 6.0), (18.0, 18.0));
            line((18.0, 6.0), (6.0, 18.0));
        }
        Icon::Grid => {
            box_(3.0, 3.0, 18.0, 18.0);
            line((9.0, 3.0), (9.0, 21.0));
            line((15.0, 3.0), (15.0, 21.0));
            line((3.0, 9.0), (21.0, 9.0));
            line((3.0, 15.0), (21.0, 15.0));
        }
        Icon::Folder => {
            p.add(Shape::closed_line(
                vec![
                    pt(2.0, 5.0),
                    pt(10.0, 5.0),
                    pt(12.0, 8.0),
                    pt(22.0, 8.0),
                    pt(22.0, 20.0),
                    pt(2.0, 20.0),
                ],
                stroke,
            ));
        }
        Icon::Mirror => {
            line((12.0, 2.0), (12.0, 22.0));
            p.add(Shape::closed_line(
                vec![pt(3.0, 18.0), pt(9.0, 6.0), pt(9.0, 18.0)],
                stroke,
            ));
            p.add(Shape::closed_line(
                vec![pt(15.0, 18.0), pt(15.0, 6.0), pt(21.0, 18.0)],
                stroke,
            ));
        }
        Icon::Refresh => {
            p.add(Shape::line(
                vec![
                    pt(20.0, 8.0),
                    pt(17.0, 4.0),
                    pt(7.0, 4.0),
                    pt(3.0, 9.0),
                    pt(3.0, 16.0),
                    pt(8.0, 20.0),
                    pt(17.0, 20.0),
                    pt(21.0, 16.0),
                ],
                stroke,
            ));
            line((20.0, 2.0), (20.0, 8.0));
            line((14.0, 8.0), (20.0, 8.0));
        }
    }
}

pub fn app_icon() -> IconData {
    let image = image::load_from_memory(include_bytes!("../assets/mini-studio.png"))
        .expect("Ícone integrado inválido")
        .to_rgba8();
    IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}
