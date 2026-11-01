use crate::{
    Studio,
    canvas_editor::{self, Drag, Handle},
    composition::{HEIGHT, Scene, SceneSource, Transform, WIDTH},
    scenes::SourceInput,
    source_runtime::RuntimeSource,
    theme::*,
};
use eframe::egui::*;
use std::{cell::RefCell, rc::Rc, time::Duration};

#[derive(Clone)]
struct Snapshot {
    transform: Transform,
    uses: Vec<(u64, Transform)>,
}

pub(crate) struct LibraryPreview {
    id: u64,
    input: SourceInput,
    runtime: Rc<RefCell<RuntimeSource>>,
    drag: Option<(Drag, Snapshot)>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Studio {
    fn library_snapshot(&self, id: u64) -> Snapshot {
        Snapshot {
            transform: self
                .scenes
                .sources
                .iter()
                .find(|s| s.id == id)
                .unwrap()
                .transform,
            uses: self
                .scenes
                .scenes
                .iter()
                .flat_map(|s| &s.sources)
                .filter(|s| s.source_id == id)
                .map(|s| (s.id, s.transform))
                .collect(),
        }
    }

    fn restore_library_snapshot(&mut self, id: u64, snapshot: Snapshot) {
        self.scenes
            .sources
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap()
            .transform = snapshot.transform;
        for source in self
            .scenes
            .scenes
            .iter_mut()
            .flat_map(|s| &mut s.sources)
            .filter(|s| s.source_id == id)
        {
            source.transform = snapshot
                .uses
                .iter()
                .find(|(key, _)| *key == source.id)
                .map(|(_, transform)| *transform)
                .unwrap_or(snapshot.transform);
        }
    }

    fn apply_library_transform(&mut self, id: u64, mut transform: Transform) {
        transform.sanitize();
        self.scenes
            .sources
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap()
            .transform = transform;
        for source in self
            .scenes
            .scenes
            .iter_mut()
            .flat_map(|s| &mut s.sources)
            .filter(|s| s.source_id == id)
        {
            source.transform = transform;
        }
        // Scene undo entries must not restore stale geometry over a library edit.
        let ids: Vec<_> = self
            .library_snapshot(id)
            .uses
            .iter()
            .map(|(id, _)| *id)
            .collect();
        self.transform_undo.retain(|(id, _)| !ids.contains(id));
        self.transform_redo.retain(|(id, _)| !ids.contains(id));
    }

    pub(crate) fn library_canvas(&mut self, ui: &mut Ui, ctx: &Context, id: u64) {
        let Some(definition) = self.scenes.sources.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        if self
            .library_preview
            .as_ref()
            .is_none_or(|p| p.id != id || p.input != definition.input)
        {
            let runtime = self
                .scenes
                .scenes
                .iter()
                .flat_map(|s| &s.sources)
                .filter(|s| s.source_id == id && s.input == definition.input)
                .find_map(|s| self.runtimes.get(&s.id).cloned())
                .unwrap_or_else(|| {
                    Rc::new(RefCell::new(RuntimeSource::loading(
                        ctx,
                        definition.input.clone(),
                    )))
                });
            self.library_preview = Some(LibraryPreview {
                id,
                input: definition.input,
                runtime,
                drag: None,
                undo: vec![],
                redo: vec![],
            });
        }
        let mut preview = self.library_preview.take().unwrap();
        preview.runtime.borrow_mut().poll(ctx);
        let mut transform = definition.transform;
        let before = self.library_snapshot(id);
        let mut force_apply = false;
        ui.horizontal_wrapped(|ui| {
        ui.label(title("Prévia da fonte", 17.0)).on_hover_text("Todas as cenas · Arraste para mover · Alças: tamanho · Alt: recorte · Shift: livre");
        ui.menu_button("Ajustes da fonte", |ui| {
            if ui.button("Ajustar fonte à tela").clicked() {
                force_apply = true;
                transform = Transform {
                    crop: transform.crop,
                    mirror: transform.mirror,
                    opacity: transform.opacity,
                    ..Default::default()
                };
            }
            if ui.button("Restaurar fonte").clicked() {
                force_apply = true;
                transform = Transform::default();
            }
            ui.checkbox(&mut transform.mirror, "Espelhar fonte");
            ui.add(Slider::new(&mut transform.opacity, 0..=255).text("Opacidade"));
        });
        if force_apply || transform != before.transform {
            preview.undo.push(before);
            preview.redo.clear();
            self.apply_library_transform(id, transform);
        }
        ui.horizontal_wrapped(|ui| {
            let keyboard = !ctx.wants_keyboard_input();
            let undo = ui
                .add_enabled(
                    !preview.undo.is_empty(),
                    Button::new("Desfazer"),
                )
                .clicked()
                || (keyboard && ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)));
            let redo = ui
                .add_enabled(
                    !preview.redo.is_empty(),
                    Button::new("Refazer"),
                )
                .clicked()
                || (keyboard && ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Y)));
            let (from, to) = if undo {
                (&mut preview.undo, &mut preview.redo)
            } else {
                (&mut preview.redo, &mut preview.undo)
            };
            if (undo || redo)
                && let Some(snapshot) = from.pop()
            {
                to.push(self.library_snapshot(id));
                self.restore_library_snapshot(id, snapshot);
                transform = self.library_snapshot(id).transform;
            }
        });

        });
        let (outer, response) = ui.allocate_exact_size(
            ui.available_size().max(vec2(1.0, 1.0)),
            Sense::click_and_drag(),
        );
        let fit = crate::composition::fit_rect(
            WIDTH,
            HEIGHT,
            [
                outer.left() + 12.0,
                outer.top() + 12.0,
                (outer.width() - 24.0).max(1.0),
                (outer.height() - 24.0).max(1.0),
            ],
        );
        let canvas = Rect::from_min_size(pos2(fit[0], fit[1]), vec2(fit[2], fit[3]));
        ui.interact(canvas, Id::new("library_preview_canvas"), Sense::hover())
            .widget_info(|| WidgetInfo::labeled(WidgetType::Image, true, "Prévia visual da fonte"));
        let painter = ui.painter_at(canvas);
        painter.rect_filled(canvas, 0, Color32::from_rgb(8, 11, 16));
        let runtime = preview.runtime.borrow();
        if let (Some(pixels), Some(texture)) = (&runtime.pixels, &runtime.texture) {
            let scene = Scene {
                sources: vec![SceneSource {
                    id,
                    pixels: pixels.clone(),
                    transform,
                }],
            };
            let layer = scene
                .layers()
                .into_iter()
                .find(|l| l.source == Some(id))
                .unwrap();
            let area = Rect::from_min_size(
                canvas.min
                    + vec2(layer.rect[0] / WIDTH as f32, layer.rect[1] / HEIGHT as f32)
                        * canvas.size(),
                vec2(layer.rect[2] / WIDTH as f32, layer.rect[3] / HEIGHT as f32) * canvas.size(),
            );
            let [l, t, r, b] = layer.uv;
            painter.image(
                texture.id(),
                area,
                Rect::from_min_max(
                    pos2(if layer.mirror { r } else { l }, t),
                    pos2(if layer.mirror { l } else { r }, b),
                ),
                Color32::from_white_alpha(layer.opacity),
            );
            canvas_editor::paint_selection(&ui.painter_at(outer), area, canvas, transform, false);
            let modifiers = ctx.input(|i| i.modifiers);
            if response.drag_started()
                && let Some(pointer) = ctx.input(|i| i.pointer.press_origin())
            {
                let handle = canvas_editor::hit_handle(area, pointer);
                if handle.is_some() || area.contains(pointer) {
                    let start = Transform {
                        mirror: transform.mirror,
                        opacity: transform.opacity,
                        crop: transform.crop,
                        stretch: true,
                        ..Transform::from_rect(layer.rect)
                    };
                    preview.drag = Some((
                        Drag {
                            id,
                            original: transform,
                            start,
                            handle: handle.unwrap_or(Handle::Move),
                            crop: modifiers.alt,
                        },
                        self.library_snapshot(id),
                    ));
                }
            }
            if response.dragged()
                && let Some((drag, _)) = &preview.drag
            {
                let mut next = drag.apply(
                    response.total_drag_delta().unwrap_or_default() / canvas.size(),
                    modifiers.shift,
                );
                if drag.handle == Handle::Move && !modifiers.ctrl {
                    canvas_editor::snap_move(
                        &mut next,
                        &[[0.0, 0.0, 1.0, 1.0]],
                        vec2(7.0, 7.0) / canvas.size(),
                    );
                }
                self.apply_library_transform(id, next);
                ctx.set_cursor_icon(drag.handle.cursor());
            }
            if let Some(pointer) = response.hover_pos() {
                if let Some(handle) = canvas_editor::hit_handle(area, pointer) {
                    ctx.set_cursor_icon(handle.cursor());
                } else if area.contains(pointer) {
                    ctx.set_cursor_icon(CursorIcon::Grab);
                }
            }
        } else {
            let message = runtime.error.as_deref().unwrap_or("Preparando fonte…");
            painter.text(
                canvas.center(),
                Align2::CENTER_CENTER,
                "Aguardando fonte…",
                FontId::proportional(18.0),
                MUTED,
            );
            response.clone().on_hover_text(message);
        }
        drop(runtime);
        if response.drag_stopped()
            && let Some((drag, snapshot)) = preview.drag.take()
            && self.library_snapshot(id).transform != drag.original
        {
            preview.undo.push(snapshot);
            preview.redo.clear();
        }
        if preview.undo.len() > 100 {
            preview.undo.remove(0);
        }
        self.library_preview = Some(preview);
        ctx.request_repaint_after(Duration::from_millis(33));
    }
}
