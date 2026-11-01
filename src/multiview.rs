use crate::{Studio, theme::*};
use eframe::egui::*;
use std::{collections::HashSet, time::Duration};

impl Studio {
    pub(crate) fn multiview(&mut self, ctx: &Context) {
        if !self.multiview_open {
            // Libera as capturas extras sem interromper a cena atual ou suas câmeras.
            let active = self.scenes.active;
            let keep: HashSet<_> = self
                .scenes
                .scenes
                .iter()
                .enumerate()
                .flat_map(|(index, scene)| {
                    scene
                        .sources
                        .iter()
                        .filter(move |source| {
                            index == active
                                || matches!(source.input, crate::scenes::SourceInput::Camera(_))
                        })
                        .map(|source| source.id)
                })
                .collect();
            self.runtimes.retain(|id, _| keep.contains(id));
            return;
        }
        let missing: Vec<_> = self
            .scenes
            .scenes
            .iter()
            .flat_map(|scene| &scene.sources)
            .filter(|source| !self.runtimes.contains_key(&source.id))
            .map(|source| source.id)
            .collect();
        for id in missing {
            self.activate_source(ctx, id);
        }
        let mut open = true;
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("multiview"),
            ViewportBuilder::default()
                .with_title("Mini Studio — Visualização múltipla")
                .with_inner_size([800.0, 520.0])
                .with_min_inner_size([360.0, 240.0]),
            |ctx, class| {
                if class == ViewportClass::Embedded {
                    Window::new("Visualização múltipla")
                        .open(&mut open)
                        .default_size([780.0, 480.0])
                        .show(ctx, |ui| self.multiview_grid(ui, ctx));
                } else {
                    open = !ctx.input(|i| i.viewport().close_requested());
                    CentralPanel::default()
                        .frame(Frame::new().fill(BG).inner_margin(10))
                        .show(ctx, |ui| self.multiview_grid(ui, ctx));
                }
                ctx.request_repaint_after(Duration::from_millis(33));
            },
        );
        self.multiview_open = open;
        ctx.request_repaint_after(Duration::from_millis(33));
    }

    fn multiview_grid(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.label(muted("Clique em uma cena para colocá-la na saída."));
        ui.add_space(8.0);
        let columns = ((ui.available_width() / 240.0).floor() as usize)
            .max(1)
            .min(self.scenes.scenes.len().max(1));
        let width = ((ui.available_width() - (columns - 1) as f32 * 8.0) / columns as f32).max(1.0);
        let mut switch = None;
        ScrollArea::vertical().show(ui, |ui| {
            Grid::new("multiview_grid")
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    for (index, saved) in self.scenes.scenes.iter().enumerate() {
                        let (tile, response) = ui.allocate_exact_size(
                            vec2(width, width * 9.0 / 16.0 + 30.0),
                            Sense::click(),
                        );
                        response.widget_info(|| {
                            WidgetInfo::labeled(
                                WidgetType::Button,
                                true,
                                format!("Alternar para {}", saved.name),
                            )
                        });
                        let active = index == self.scenes.active;
                        let canvas = Rect::from_min_size(tile.min, vec2(width, width * 9.0 / 16.0));
                        let painter = ui.painter_at(canvas);
                        self.paint_scene(&painter, canvas, &self.scene_composition(saved));
                        let status = if saved.sources.is_empty() {
                            "Cena vazia"
                        } else if saved.sources.iter().filter(|s| s.visible).any(|s| {
                            self.runtimes
                                .get(&s.id)
                                .is_some_and(|r| r.borrow().error.is_some())
                        }) {
                            "Fonte indisponível"
                        } else if saved.sources.iter().filter(|s| s.visible).any(|s| {
                            self.runtimes
                                .get(&s.id)
                                .is_none_or(|r| r.borrow().loading_now())
                        }) {
                            "Carregando…"
                        } else {
                            ""
                        };
                        if !status.is_empty() {
                            painter.text(
                                canvas.center(),
                                Align2::CENTER_CENTER,
                                status,
                                FontId::proportional(13.0),
                                TEXT,
                            );
                        }
                        let label =
                            Rect::from_min_max(pos2(tile.left(), canvas.bottom()), tile.max);
                        ui.painter()
                            .rect_filled(label, 0, if active { ACCENT } else { PANEL });
                        ui.painter_at(label.shrink(5.0)).text(
                            label.left_center() + vec2(8.0, 0.0),
                            Align2::LEFT_CENTER,
                            format!("{}{}", if active { "● ATIVA · " } else { "" }, saved.name),
                            FontId::proportional(13.0),
                            if active { BG } else { TEXT },
                        );
                        ui.painter().rect_stroke(
                            tile,
                            0,
                            Stroke::new(
                                if active { 2.0_f32 } else { 1.0_f32 },
                                if active || response.hovered() {
                                    ACCENT
                                } else {
                                    LINE
                                },
                            ),
                            StrokeKind::Inside,
                        );
                        if response.clicked() {
                            switch = Some(index);
                        }
                        response
                            .on_hover_text(&saved.name)
                            .on_hover_cursor(CursorIcon::PointingHand);
                        if (index + 1) % columns == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
        if let Some(index) = switch {
            self.switch_scene(ctx, index);
        }
    }
}
