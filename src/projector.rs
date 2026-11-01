use crate::{
    Studio,
    composition::{HEIGHT, Scene, WIDTH},
    theme::*,
};
use eframe::egui::*;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Display {
    id: u32,
    label: String,
    pixels: Rect,
}

fn displays() -> Result<Vec<Display>, String> {
    xcap::Monitor::all()
        .and_then(|monitors| {
            monitors
                .into_iter()
                .map(|monitor| {
                    let width = monitor.width()?;
                    let height = monitor.height()?;
                    let name = monitor.name()?;
                    Ok(Display {
                        id: monitor.id()?,
                        label: format!(
                            "{name} · {width} × {height}{}",
                            if monitor.is_primary()? {
                                " · Principal"
                            } else {
                                ""
                            }
                        ),
                        pixels: Rect::from_min_size(
                            pos2(monitor.x()? as f32, monitor.y()? as f32),
                            vec2(width as f32, height as f32),
                        ),
                    })
                })
                .collect()
        })
        .map_err(|error| format!("Não foi possível listar os monitores: {error}"))
}

#[derive(Default)]
pub(crate) struct Projector {
    pub open: bool,
    pub dialog_open: bool,
    generation: u64,
    fullscreen: bool,
    target: Option<Display>,
    positioning: bool,
    checked_at: Option<Instant>,
    displays: Vec<Display>,
    error: Option<String>,
}

impl Projector {
    pub fn choose_destination(&mut self) {
        self.dialog_open = true;
        match displays() {
            Ok(displays) => {
                self.displays = displays;
                self.error = None;
            }
            Err(error) => {
                self.displays.clear();
                self.error = Some(error);
            }
        }
    }

    fn start(&mut self, target: Option<Display>) {
        // Uma nova janela evita herdar tela cheia/posição da projeção anterior.
        self.generation = self.generation.wrapping_add(1);
        self.open = true;
        self.dialog_open = false;
        self.fullscreen = false;
        self.positioning = target.is_some();
        self.target = target;
        self.checked_at = Some(Instant::now());
    }
}

impl Studio {
    pub(crate) fn projector_dialog(&mut self, ctx: &Context) {
        if !self.projector.dialog_open {
            return;
        }
        let mut destination = None;
        let result = Modal::new(Id::new("projector_destination"))
            .frame(modal_frame())
            .show(ctx, |ui| {
                ui.set_width(420.0_f32.min(ctx.content_rect().width() - 48.0));
                ui.label(title("Projetor de saída", 22.0));
                ui.label(muted("Exibe a cena ativa sem controles ou guias."));
                ui.add_space(10.0);
                if button(
                    ui,
                    Icon::Window,
                    "Abrir em janela",
                    ButtonKind::Primary,
                    ui.available_width(),
                )
                .clicked()
                {
                    destination = Some(None);
                }
                ui.add_space(8.0);
                ui.label("Tela cheia em:");
                ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    for display in &self.projector.displays {
                        if button(
                            ui,
                            Icon::Monitor,
                            &display.label,
                            ButtonKind::Secondary,
                            ui.available_width(),
                        )
                        .clicked()
                        {
                            destination = Some(Some(display.clone()));
                        }
                    }
                });
                if let Some(error) = &self.projector.error {
                    ui.label(RichText::new(error).color(AMBER));
                } else if self.projector.displays.is_empty() {
                    ui.label(muted("Nenhum monitor disponível."));
                }
                ui.add_space(8.0);
                ui.label(muted("No projetor: F11 alterna tela cheia; Esc fecha."));
                ui.horizontal(|ui| {
                    if ui.button("Atualizar monitores").clicked() {
                        self.projector.choose_destination();
                    }
                    if ui.button("Cancelar").clicked() {
                        self.projector.dialog_open = false;
                    }
                });
            });
        if result.should_close() {
            self.projector.dialog_open = false;
        }
        if let Some(target) = destination {
            self.projector.start(target);
        }
    }

    pub(crate) fn projector(&mut self, ctx: &Context) {
        if !self.projector.open {
            return;
        }
        if self.projector.target.is_some()
            && self
                .projector
                .checked_at
                .is_none_or(|time| time.elapsed() >= Duration::from_secs(2))
        {
            self.projector.checked_at = Some(Instant::now());
            if let Ok(displays) = displays() {
                let target = self.projector.target.as_ref().unwrap();
                if !displays
                    .iter()
                    .any(|display| display.id == target.id && display.pixels == target.pixels)
                {
                    self.projector.open = false;
                    self.show_error("O monitor do projetor foi desconectado ou reconfigurado. Escolha o destino novamente.".into());
                    return;
                }
            }
        }
        let id = ViewportId::from_hash_of(("clean_projector", self.projector.generation));
        ctx.show_viewport_immediate(
            id,
            ViewportBuilder::default()
                .with_title("Mini Studio — Projetor de saída")
                .with_inner_size([640.0, 360.0])
                .with_min_inner_size([160.0, 90.0]),
            |ctx, class| {
                if class == ViewportClass::Embedded {
                    let mut open = self.projector.open;
                    Window::new("Projetor de saída")
                        .open(&mut open)
                        .default_pos(pos2(20.0, 20.0))
                        .default_size([640.0, 360.0])
                        .show(ctx, |ui| self.projector_content(ui));
                    self.projector.open &= open;
                    return;
                }
                if ctx.input(|i| i.viewport().close_requested())
                    || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
                {
                    self.projector.open = false;
                    return;
                }
                // Posiciona em pixels físicos, inclusive com DPI misto e monitores à esquerda.
                // Só entra em tela cheia depois que o Windows confirma a mudança de monitor.
                if self.projector.positioning {
                    if let Some(target) = &self.projector.target {
                        let ppp = ctx.pixels_per_point();
                        let outer = ctx.input(|i| i.viewport().outer_rect);
                        if outer.is_some_and(|rect| target.pixels.contains(rect.center() * ppp)) {
                            self.projector.positioning = false;
                            self.projector.fullscreen = true;
                            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
                        } else {
                            let size = outer.map_or(vec2(640.0, 360.0), |rect| rect.size());
                            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(
                                target.pixels.center() / ppp - size * 0.5,
                            ));
                        }
                    }
                } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F11)) {
                    self.toggle_projector_fullscreen(ctx);
                }
                CentralPanel::default()
                    .frame(Frame::NONE.fill(Color32::BLACK))
                    .show(ctx, |ui| self.projector_content(ui));
                ctx.request_repaint_after(Duration::from_millis(33));
            },
        );
        ctx.request_repaint_after(Duration::from_millis(33));
    }

    fn toggle_projector_fullscreen(&mut self, ctx: &Context) {
        self.projector.positioning = false;
        self.projector.fullscreen = !self.projector.fullscreen;
        if !self.projector.fullscreen {
            self.projector.target = None;
        }
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.projector.fullscreen));
    }

    fn projector_content(&mut self, ui: &mut Ui) {
        let (outer, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        ui.painter().rect_filled(outer, 0, Color32::BLACK);
        let fit = crate::composition::fit_rect(
            WIDTH,
            HEIGHT,
            [outer.left(), outer.top(), outer.width(), outer.height()],
        );
        let rect = Rect::from_min_size(pos2(fit[0], fit[1]), vec2(fit[2], fit[3]));
        self.paint_scene(&ui.painter_at(rect), rect, &self.scene());
        response.context_menu(|ui| {
            if ui
                .button(if self.projector.fullscreen {
                    "Voltar à janela"
                } else {
                    "Tela cheia (F11)"
                })
                .clicked()
            {
                self.toggle_projector_fullscreen(ui.ctx());
                ui.close();
            }
            if ui.button("Fechar projetor").clicked() {
                self.projector.open = false;
                ui.close();
            }
        });
        if self.projector.fullscreen && response.hovered() && !response.context_menu_opened() {
            ui.ctx().set_cursor_icon(CursorIcon::None);
        }
    }

    pub(crate) fn paint_scene(&self, painter: &Painter, rect: Rect, scene: &Scene) {
        for layer in scene.layers() {
            let [x, y, w, h] = layer.rect;
            let area = Rect::from_min_size(
                rect.min
                    + vec2(
                        x / WIDTH as f32 * rect.width(),
                        y / HEIGHT as f32 * rect.height(),
                    ),
                vec2(
                    w / WIDTH as f32 * rect.width(),
                    h / HEIGHT as f32 * rect.height(),
                ),
            );
            if let Some(id) = layer.source {
                if let Some(runtime) = self.runtimes.get(&id)
                    && let Some(texture) = &runtime.borrow().texture
                {
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
                }
            } else {
                painter.rect_filled(
                    area,
                    0,
                    Color32::from_rgb(layer.color[0], layer.color[1], layer.color[2]),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Options,
        composition::{Pixels, Transform},
        image_source::LoadedImage,
    };

    fn draw(app: &mut Studio, ctx: &Context) -> FullOutput {
        ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default()
                    .frame(Frame::NONE)
                    .show(ctx, |ui| app.projector_content(ui));
            },
        )
    }

    #[test]
    fn clean_output_clips_crop_mirror_and_opacity_without_editor_overlays() {
        let ctx = Context::default();
        let mut app = Studio::initialize(
            &ctx,
            "Teste".into(),
            Options {
                screenshot: Some("unused.png".into()),
                ..Default::default()
            },
        );
        app.accept_image(
            &ctx,
            LoadedImage {
                path: "teste.png".into(),
                name: "Teste".into(),
                pixels: Pixels {
                    width: 4,
                    height: 2,
                    rgba: [255u8, 0, 0, 255].repeat(8).into(),
                },
            },
            None,
        );
        let id = app.selected.unwrap();
        app.guides = true;
        app.source_mut(id).unwrap().transform = Transform {
            x: -0.25,
            stretch: true,
            mirror: true,
            opacity: 128,
            crop: [0.25, 0.0, 0.0, 0.0],
            ..Default::default()
        };
        let texture = app.runtimes[&id].borrow().texture.as_ref().unwrap().id();
        let output = draw(&mut app, &ctx);
        let images: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                epaint::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                    Some((shape.clip_rect, mesh))
                }
                _ => None,
            })
            .collect();
        assert_eq!(images.len(), 1);
        let (clip, mesh) = images[0];
        assert_eq!(
            clip,
            Rect::from_min_max(pos2(0.0, 75.0), pos2(800.0, 525.0))
        );
        assert_eq!(
            mesh.calc_bounds(),
            Rect::from_min_max(pos2(-200.0, 75.0), pos2(600.0, 525.0))
        );
        assert_eq!(mesh.vertices[0].uv.x, 1.0);
        assert_eq!(mesh.vertices[1].uv.x, 0.25);
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| vertex.color == Color32::from_white_alpha(128))
        );
        assert!(output.shapes.iter().all(|shape| matches!(
            shape.shape,
            epaint::Shape::Noop | epaint::Shape::Rect(_) | epaint::Shape::Mesh(_)
        )));

        // Ocultar a fonte e trocar de cena precisam atualizar a projeção imediatamente.
        app.source_mut(id).unwrap().visible = false;
        assert!(
            !draw(&mut app, &ctx)
                .shapes
                .iter()
                .any(|shape| matches!(shape.shape, epaint::Shape::Mesh(_)))
        );
        app.source_mut(id).unwrap().visible = true;
        app.scenes.scenes.push(crate::scenes::SavedScene {
            name: "Vazia".into(),
            sources: vec![],
        });
        app.switch_scene(&ctx, 1);
        assert!(
            !draw(&mut app, &ctx)
                .shapes
                .iter()
                .any(|shape| matches!(shape.shape, epaint::Shape::Mesh(_)))
        );
        assert!(app.virtual_camera.is_none());
    }
}
