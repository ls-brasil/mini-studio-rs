use crate::{
    Choice, PickerKind, Studio,
    composition::{HEIGHT, Source, Transform, WIDTH},
    preferences::Preferences,
    scenes::SourceInput,
    theme::{self, *},
    virtual_camera,
};
use eframe::egui::*;
use std::{
    path::Path,
    time::{Duration, Instant},
};

impl Studio {
    fn has_sources(&self) -> bool {
        !self.current_scene().sources.is_empty()
    }
    fn remove_source(&mut self, id: Source) {
        self.current_scene_mut().sources.retain(|s| s.id != id);
        self.runtimes.remove(&id);
        if self.selected == Some(id) {
            self.selected = None;
        }
        if self.properties == Some(id) {
            self.properties = None;
        }
    }
    fn duplicate_source(&mut self, ctx: &Context, id: Source) {
        let Some(mut source) = self.source(id).cloned() else {
            return;
        };
        source.id = self.allocate_source_id();
        source.name = format!("{} — cópia", source.name);
        source.locked = false;
        let new_id = source.id;
        let index = self
            .current_scene()
            .sources
            .iter()
            .position(|s| s.id == id)
            .unwrap();
        self.current_scene_mut().sources.insert(index + 1, source);
        if let Some(runtime) = self.runtimes.get(&id).cloned() {
            self.runtimes.insert(new_id, runtime);
        } else {
            self.activate_source(ctx, new_id);
        }
        self.selected = Some(new_id);
    }
    fn move_source(&mut self, id: Source, up: bool) {
        let sources = &mut self.current_scene_mut().sources;
        if let Some(index) = sources.iter().position(|s| s.id == id) {
            let other = if up {
                (index + 1).min(sources.len() - 1)
            } else {
                index.saturating_sub(1)
            };
            sources.swap(index, other);
        }
    }
    fn fit_source(&mut self, id: Source) {
        if let Some(source) = self.source_mut(id) {
            source.transform = Transform {
                mirror: source.transform.mirror,
                opacity: source.transform.opacity,
                ..Default::default()
            };
        }
    }
    fn persist_preferences(&mut self) {
        if self.options.screenshot.is_some() || self.options.benchmark.is_some() {
            return;
        }
        let current = Preferences {
            guides: self.guides,
            scale: self.scale,
            ..self.saved_preferences.clone()
        };
        if current != self.saved_preferences {
            match current.save() {
                Ok(()) => self.saved_preferences = current,
                Err(e) => self.show_error(format!("Não foi possível guardar os ajustes: {e}")),
            }
        }
    }
    fn start_output(&mut self, ctx: &Context) {
        if self.virtual_camera.is_some() {
            return;
        }
        if !virtual_camera::installed() {
            self.setup_open = true;
            return;
        }
        match virtual_camera::VirtualCamera::start(self.scene(), ctx.clone()) {
            Ok(output) => self.virtual_camera = Some(output),
            Err(e) => self.virtual_notice = e,
        }
    }
    fn header(&mut self, ctx: &Context) {
        TopBottomPanel::top("app_header")
            .exact_height(60.0)
            .frame(
                Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(18, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                    theme::logo(ui.painter(), rect);
                    ui.label(title("Mini Studio", 20.0));
                    ui.label(muted("Seu estúdio de composição"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if small_icon(ui, Icon::Help, "Ajuda").clicked() {
                            self.help_open = true;
                        }
                        badge(
                            ui,
                            if self.virtual_camera.as_ref().is_some_and(|o| o.active()) {
                                "CÂMERA VIRTUAL ATIVA"
                            } else {
                                "ESTÚDIO"
                            },
                            ACCENT,
                        );
                    });
                });
            });
    }
    fn footer(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("app_footer")
            .exact_height(30.0)
            .frame(
                Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(16, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if !self.notice.is_empty()
                        && (self.notice_error || self.notice_at.elapsed() < Duration::from_secs(8))
                    {
                        let width = (ui.available_width() - 300.0).max(100.0);
                        let color = if self.notice_error { AMBER } else { ACCENT };
                        ui.add_sized(
                            [width, 20.0],
                            Label::new(RichText::new(&self.notice).size(12.0).color(color))
                                .truncate(),
                        )
                        .on_hover_text(&self.notice);
                    } else {
                        ui.label(muted(if self.scenes_load_error.is_some() {
                            "Erro ao ler cenas salvas"
                        } else if self.scenes == self.saved_scenes {
                            "Todas as cenas salvas"
                        } else {
                            "Salvando alterações…"
                        }));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(muted("LS BRASIL"));
                        ui.separator();
                        ui.label(muted("1280 × 720"));
                        if self.runtimes.values().any(|r| r.borrow().live()) {
                            ui.separator();
                            ui.label(muted(format!("{:.0} FPS", self.fps)));
                        }
                    });
                });
            });
    }
    fn bottom_dock(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("studio_dock")
            .resizable(true)
            .default_height(220.0)
            .height_range(200.0..=(ctx.content_rect().height() * 0.55).max(200.0))
            .frame(Frame::new().fill(LINE).inner_margin(1))
            .show(ctx, |ui| {
                let available = ui.available_size();
                let gap = 2.0;
                let width = available.x - gap * 2.0;
                ui.spacing_mut().item_spacing = vec2(gap, 6.0);
                ui.horizontal_top(|ui| {
                    for (index, ratio) in [0.24, 0.46, 0.30].into_iter().enumerate() {
                        ui.allocate_ui_with_layout(
                            vec2(width * ratio, available.y),
                            Layout::top_down(Align::Min),
                            |ui| {
                                Frame::new().fill(PANEL).inner_margin(12).show(ui, |ui| {
                                    ui.set_min_size(vec2(
                                        (width * ratio - 24.0).max(10.0),
                                        (available.y - 24.0).max(10.0),
                                    ));
                                    ui.set_max_width((width * ratio - 24.0).max(10.0));
                                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                                    ui.spacing_mut().button_padding = vec2(8.0, 4.0);
                                    ui.spacing_mut().interact_size.y = 28.0;
                                    match index {
                                        0 => self.scenes_dock(ui, ctx),
                                        1 => self.sources_dock(ui, ctx),
                                        _ => self.controls_dock(ui, ctx),
                                    }
                                });
                            },
                        );
                    }
                });
            });
    }
    fn sources_dock(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.label(title("Fontes", 16.0));
            ui.label(muted(format!("{}", self.current_scene().sources.len())));
        });
        TopBottomPanel::bottom("source_toolbar")
            .frame(Frame::NONE)
            .show_inside(ui, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.menu_button("+", |ui| {
                        for (name, kind) in [
                            ("Tela", Some(PickerKind::Screen)),
                            ("Janela", Some(PickerKind::Window)),
                            ("Câmera", Some(PickerKind::Camera)),
                            ("Imagem", None),
                        ] {
                            if ui
                                .add_enabled(
                                    kind.is_some() || self.image_receiver.is_none(),
                                    Button::new(name),
                                )
                                .clicked()
                            {
                                if let Some(kind) = kind {
                                    self.open_picker(ctx, kind, None);
                                } else {
                                    self.open_image(ctx);
                                }
                                ui.close();
                            }
                        }
                        if let Some(id) = self.selected
                            && ui.button("Duplicar fonte selecionada").clicked()
                        {
                            self.duplicate_source(ctx, id);
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("Adicionar fonte");
                    let selected = self.selected;
                    ui.add_enabled_ui(selected.is_some(), |ui| {
                        if small_icon(ui, Icon::Trash, "Remover fonte").clicked()
                            && let Some(id) = selected
                        {
                            self.remove_source(id);
                        }
                        if small_icon(ui, Icon::Settings, "Propriedades").clicked() {
                            self.properties = selected;
                        }
                        if glyph_button(ui, "↑", "Mover fonte para cima").clicked()
                            && let Some(id) = selected
                        {
                            self.move_source(id, true);
                        }
                        if glyph_button(ui, "↓", "Mover fonte para baixo").clicked()
                            && let Some(id) = selected
                        {
                            self.move_source(id, false);
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(muted("Topo = frente"));
                    });
                });
            });
        ScrollArea::vertical()
            .id_salt("source_list")
            .show(ui, |ui| {
                let sources = self.current_scene().sources.clone();
                if sources.is_empty() {
                    ui.add_space(14.0);
                    ui.label(muted("Esta cena está vazia."));
                    ui.label(muted("Use + para adicionar e misturar suas fontes."));
                }
                for source in sources.iter().rev() {
                    ui.push_id(source.id, |ui| {
                        Frame::new()
                            .fill(if self.selected == Some(source.id) {
                                ACCENT_DARK
                            } else {
                                Color32::TRANSPARENT
                            })
                            .corner_radius(4)
                            .inner_margin(Margin::symmetric(5, 1))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let icon = match &source.input {
                                        SourceInput::Desktop(d) => {
                                            if d.window {
                                                Icon::Window
                                            } else {
                                                Icon::Monitor
                                            }
                                        }
                                        SourceInput::Camera(_) => Icon::Camera,
                                        SourceInput::Image(_) => Icon::Image,
                                    };
                                    let (rect, _) =
                                        ui.allocate_exact_size(vec2(18.0, 22.0), Sense::hover());
                                    paint_icon(
                                        ui.painter(),
                                        icon,
                                        rect.shrink2(vec2(0.0, 2.0)),
                                        MUTED,
                                    );
                                    let status = self
                                        .runtimes
                                        .get(&source.id)
                                        .map(|r| {
                                            let r = r.borrow();
                                            if let Some(error) = &r.error {
                                                error.clone()
                                            } else if r.loading_now() {
                                                "Abrindo fonte…".into()
                                            } else {
                                                "".into()
                                            }
                                        })
                                        .unwrap_or_else(|| {
                                            "Fonte salva. Clique em Ativar fontes.".into()
                                        });
                                    let r =
                                        ui.add_sized(
                                            [(ui.available_width() - 68.0).max(20.0), 28.0],
                                            Button::new(RichText::new(&source.name).color(
                                                if status.is_empty() { TEXT } else { AMBER },
                                            ))
                                            .frame(false)
                                            .truncate(),
                                        );
                                    if r.clicked() {
                                        self.selected = Some(source.id);
                                    }
                                    if r.double_clicked() {
                                        self.selected = Some(source.id);
                                        self.properties = Some(source.id);
                                    }
                                    r.on_hover_text(format!(
                                        "{} · {}\n{}",
                                        source.name,
                                        source.input.label(),
                                        status
                                    ));
                                    if small_icon(
                                        ui,
                                        if source.visible {
                                            Icon::Eye
                                        } else {
                                            Icon::Hidden
                                        },
                                        if source.visible {
                                            "Ocultar fonte"
                                        } else {
                                            "Mostrar fonte"
                                        },
                                    )
                                    .clicked()
                                        && let Some(s) = self.source_mut(source.id)
                                    {
                                        s.visible = !s.visible;
                                    }
                                    if small_icon(
                                        ui,
                                        if source.locked {
                                            Icon::Lock
                                        } else {
                                            Icon::Unlock
                                        },
                                        if source.locked {
                                            "Desbloquear fonte"
                                        } else {
                                            "Bloquear fonte"
                                        },
                                    )
                                    .clicked()
                                        && let Some(s) = self.source_mut(source.id)
                                    {
                                        s.locked = !s.locked;
                                    }
                                });
                            });
                    });
                }
            });
    }
    fn controls_dock(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.label(title("Controles", 16.0));
        ScrollArea::vertical()
            .id_salt("controls_scroll")
            .show(ui, |ui| {
                if let Some(output) = &self.virtual_camera {
                    if ui
                        .add_enabled_ui(!output.stopping(), |ui| {
                            button(
                                ui,
                                Icon::Stop,
                                "Parar câmera virtual",
                                ButtonKind::Primary,
                                ui.available_width(),
                            )
                        })
                        .inner
                        .clicked()
                    {
                        output.stop();
                    }
                    ui.add(Label::new(muted(&self.virtual_notice)).truncate())
                        .on_hover_text(&self.virtual_notice);
                } else {
                    if button(
                        ui,
                        Icon::Play,
                        "Iniciar câmera virtual",
                        ButtonKind::Primary,
                        ui.available_width(),
                    )
                    .clicked()
                    {
                        self.start_output(ctx);
                    }
                    if self.virtual_notice != "Câmera virtual parada." {
                        ui.add(Label::new(muted(&self.virtual_notice)).truncate())
                            .on_hover_text(&self.virtual_notice);
                    }
                }
                if ui
                    .add_enabled_ui(self.has_sources() && self.save_receiver.is_none(), |ui| {
                        button(
                            ui,
                            Icon::Image,
                            "Salvar imagem",
                            ButtonKind::Secondary,
                            ui.available_width(),
                        )
                    })
                    .inner
                    .clicked()
                {
                    self.save_snapshot(ctx);
                }
                if button(
                    ui,
                    Icon::Save,
                    "Salvar cena",
                    ButtonKind::Secondary,
                    ui.available_width(),
                )
                .clicked()
                {
                    self.persist_scenes(true);
                }
                let missing = self.current_scene().sources.iter().any(|s| {
                    self.runtimes
                        .get(&s.id)
                        .is_none_or(|r| r.borrow().error.is_some())
                });
                if missing
                    && button(
                        ui,
                        Icon::Refresh,
                        "Ativar fontes",
                        ButtonKind::Quiet,
                        ui.available_width(),
                    )
                    .clicked()
                {
                    self.activate_scene(ctx);
                }
                ui.label(muted("720p · 30 FPS · Vídeo sem áudio"));
            });
    }
    fn selection_strip(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("selection_strip")
            .exact_height(42.0)
            .frame(
                Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(16, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let name = self
                        .selected
                        .and_then(|id| self.source(id))
                        .map(|s| s.name.as_str())
                        .unwrap_or("Nenhuma fonte selecionada");
                    ui.add_sized([220.0, 28.0], Label::new(muted(name)).truncate());
                    ui.add_enabled_ui(self.selected.is_some(), |ui| {
                        if ui.button("Propriedades").clicked() {
                            self.properties = self.selected;
                        }
                        if ui.button("Ajustar à tela").clicked()
                            && let Some(id) = self.selected
                        {
                            self.fit_source(id);
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(muted("Arraste para mover · Canto para redimensionar"));
                    });
                });
            });
    }
    fn properties_dialog(&mut self, ctx: &Context) {
        let Some(id) = self.properties else {
            return;
        };
        if self.source(id).is_none() {
            self.properties = None;
            return;
        }
        let mut close = false;
        let mut replace = false;
        let mut activate = false;
        let error = self
            .runtimes
            .get(&id)
            .and_then(|r| r.borrow().error.clone());
        let result = Modal::new(Id::new("source_properties"))
            .frame(modal_frame())
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 130.0).max(200.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(title("Propriedades da fonte", 22.0));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if small_icon(ui, Icon::Close, "Fechar propriedades").clicked() {
                                    close = true;
                                }
                            });
                        });
                        let source = self.source_mut(id).unwrap();
                        let label = ui.label(muted("Nome da fonte"));
                        let response = ui
                            .add(
                                TextEdit::singleline(&mut source.name)
                                    .id_salt(("source_name", id))
                                    .char_limit(80)
                                    .desired_width(f32::INFINITY),
                            )
                            .labelled_by(label.id);
                        if response.lost_focus() && source.name.trim().is_empty() {
                            source.name = source.input.name();
                        }
                        ui.add(
                            Label::new(muted(format!(
                                "{} · {}",
                                source.input.label(),
                                source.input.name()
                            )))
                            .truncate(),
                        )
                        .on_hover_text(source.input.name());
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut source.visible, "Visível");
                            ui.checkbox(&mut source.locked, "Bloquear na prévia");
                        });
                        ui.separator();
                        section(ui, "TAMANHO E POSIÇÃO");
                        ui.label(muted(
                            "Percentuais da tela. A proporção original da fonte é preservada.",
                        ));
                        percent(ui, "Horizontal", &mut source.transform.x, 0.0..=1.0);
                        percent(ui, "Vertical", &mut source.transform.y, 0.0..=1.0);
                        percent(ui, "Largura", &mut source.transform.width, 0.01..=1.0);
                        percent(ui, "Altura", &mut source.transform.height, 0.01..=1.0);
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                            paint_icon(ui.painter(), Icon::Mirror, r, MUTED);
                            ui.checkbox(&mut source.transform.mirror, "Espelhar horizontalmente");
                        });
                        let mut opacity = source.transform.opacity as f32 / 255.0;
                        percent(ui, "Opacidade", &mut opacity, 0.0..=1.0);
                        source.transform.opacity = (opacity * 255.0).round() as u8;
                        if ui.button("Ajustar à tela").clicked() {
                            self.fit_source(id);
                        }
                        if let Some(error) = &error {
                            ui.label(RichText::new(error).color(AMBER));
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            replace = ui.button("Escolher outra fonte").clicked();
                            activate = ui.button("Reconectar").clicked();
                        });
                        ui.label(muted("Os ajustes desta fonte são salvos com a cena."));
                        if ui.button("Concluir").clicked() {
                            close = true;
                        }
                    });
            });
        if activate {
            self.runtimes.remove(&id);
            self.activate_source(ctx, id);
        }
        if replace {
            let input = self.source(id).unwrap().input.clone();
            self.properties = None;
            match input {
                SourceInput::Image(_) => self.load_image(ctx, None, Some(id)),
                SourceInput::Camera(_) => self.open_picker(ctx, PickerKind::Camera, Some(id)),
                SourceInput::Desktop(d) => self.open_picker(
                    ctx,
                    if d.window {
                        PickerKind::Window
                    } else {
                        PickerKind::Screen
                    },
                    Some(id),
                ),
            }
        } else if close || result.should_close() {
            if let Some(source) = self.source_mut(id)
                && source.name.trim().is_empty()
            {
                source.name = source.input.name();
            }
            self.properties = None;
            self.persist_scenes(false);
        }
    }
    fn canvas(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.label(title(&self.current_scene().name, 17.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if small_icon(ui, Icon::Grid, "Guias de enquadramento").clicked() {
                    self.guides = !self.guides;
                }
                ui.label(muted("PRÉVIA · 16:9"));
            });
        });
        let available = ui.available_size();
        let (outer, _) =
            ui.allocate_exact_size(vec2(available.x, available.y.max(20.0)), Sense::hover());
        ui.painter()
            .rect_filled(outer, 8, Color32::from_rgb(18, 23, 30));
        let fit = crate::composition::fit_rect(
            WIDTH,
            HEIGHT,
            [
                outer.left() + 10.0,
                outer.top() + 10.0,
                (outer.width() - 20.0).max(1.0),
                (outer.height() - 20.0).max(1.0),
            ],
        );
        let rect = Rect::from_min_size(pos2(fit[0], fit[1]), vec2(fit[2], fit[3]));
        let response = ui.interact(rect, Id::new("composition_canvas"), Sense::click_and_drag());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Image, true, "Prévia da cena"));
        let painter = ui.painter_at(rect);
        let scene = self.scene();
        let layers = scene.layers();
        let convert = |r: [f32; 4]| {
            Rect::from_min_size(
                pos2(
                    rect.left() + r[0] * rect.width() / WIDTH as f32,
                    rect.top() + r[1] * rect.height() / HEIGHT as f32,
                ),
                vec2(
                    r[2] * rect.width() / WIDTH as f32,
                    r[3] * rect.height() / HEIGHT as f32,
                ),
            )
        };
        for layer in &layers {
            let area = convert(layer.rect);
            if let Some(id) = layer.source
                && let Some(runtime) = self.runtimes.get(&id)
                && let Some(texture) = &runtime.borrow().texture
            {
                let uv = if layer.mirror {
                    Rect::from_min_max(pos2(1.0, 0.0), pos2(0.0, 1.0))
                } else {
                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0))
                };
                painter.image(
                    texture.id(),
                    area,
                    uv,
                    Color32::from_white_alpha(layer.opacity),
                );
            } else {
                painter.rect_filled(
                    area,
                    0,
                    Color32::from_rgb(layer.color[0], layer.color[1], layer.color[2]),
                );
            }
        }
        if scene.sources.is_empty() {
            let (heading, instruction) = if !self.has_sources() {
                ("Monte sua cena", "Use + no painel Fontes abaixo")
            } else if self
                .current_scene()
                .sources
                .iter()
                .all(|source| !source.visible)
            {
                (
                    "Fontes ocultas",
                    "Use o olho na lista de fontes para mostrá-las",
                )
            } else if self
                .runtimes
                .values()
                .any(|runtime| runtime.borrow().loading_now())
            {
                (
                    "Preparando fontes…",
                    "Sua composição estará pronta em instantes",
                )
            } else {
                (
                    "Cena salva",
                    "Use Ativar fontes em Controles para abrir a composição",
                )
            };
            painter.text(
                rect.center() - vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                heading,
                FontId::proportional(24.0),
                TEXT,
            );
            painter.text(
                rect.center() + vec2(0.0, 22.0),
                Align2::CENTER_CENTER,
                instruction,
                FontId::proportional(14.0),
                MUTED,
            );
        }
        if self.guides {
            for ratio in [1.0 / 3.0, 2.0 / 3.0] {
                let stroke = Stroke::new(1.0_f32, Color32::from_white_alpha(60));
                painter.line_segment(
                    [
                        pos2(rect.left() + ratio * rect.width(), rect.top()),
                        pos2(rect.left() + ratio * rect.width(), rect.bottom()),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        pos2(rect.left(), rect.top() + ratio * rect.height()),
                        pos2(rect.right(), rect.top() + ratio * rect.height()),
                    ],
                    stroke,
                );
            }
        }
        if (response.clicked() || response.drag_started())
            && let Some(pointer) = ctx
                .input(|i| i.pointer.press_origin())
                .or(response.interact_pointer_pos())
        {
            let resize = self
                .selected
                .and_then(|id| layers.iter().find(|l| l.source == Some(id)))
                .is_some_and(|l| convert(l.rect).right_bottom().distance(pointer) < 14.0);
            if !resize {
                self.selected = layers
                    .iter()
                    .rev()
                    .filter_map(|l| l.source.map(|id| (id, l)))
                    .find(|(id, l)| {
                        self.source(*id).is_some_and(|s| !s.locked)
                            && convert(l.rect).contains(pointer)
                    })
                    .map(|(id, _)| id);
            }
            if response.drag_started()
                && let Some(id) = self.selected
                && self.source(id).is_some_and(|s| !s.locked)
                && let Some(layer) = layers.iter().find(|l| l.source == Some(id))
            {
                let mut transform = Transform::from_rect(layer.rect);
                transform.mirror = layer.mirror;
                transform.opacity = layer.opacity;
                self.canvas_drag = Some((id, transform, resize));
            }
        }
        if response.double_clicked() {
            self.properties = self.selected;
        }
        if response.dragged()
            && let Some((id, start, resize)) = self.canvas_drag
        {
            let delta = response.total_drag_delta().unwrap_or_default() / rect.size();
            if let Some(source) = self.source_mut(id)
                && !source.locked
            {
                source.transform = drag_transform(start, [delta.x, delta.y], resize);
            }
            ctx.set_cursor_icon(if resize {
                CursorIcon::ResizeNwSe
            } else {
                CursorIcon::Grabbing
            });
        }
        if response.drag_stopped() {
            self.canvas_drag = None;
        }
        if let Some(id) = self.selected
            && let Some(layer) = layers.iter().find(|l| l.source == Some(id))
        {
            let area = convert(layer.rect);
            let locked = self.source(id).is_some_and(|s| s.locked);
            let color = if locked { MUTED } else { ACCENT };
            painter.rect_stroke(area, 0, Stroke::new(1.5_f32, color), StrokeKind::Inside);
            if !locked {
                for point in [
                    area.left_top(),
                    area.right_top(),
                    area.left_bottom(),
                    area.right_bottom(),
                ] {
                    painter.rect_filled(Rect::from_center_size(point, vec2(7.0, 7.0)), 1, color);
                }
                if response
                    .hover_pos()
                    .is_some_and(|p| p.distance(area.right_bottom()) < 14.0)
                {
                    ctx.set_cursor_icon(CursorIcon::ResizeNwSe);
                }
            }
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            painter.rect_stroke(
                rect.shrink(4.0),
                4,
                Stroke::new(2.0_f32, ACCENT),
                StrokeKind::Inside,
            );
        }
    }
    fn draw_picker(&mut self, ctx: &Context) {
        let busy = false;
        let mut selected = None;
        let mut refresh = None;
        let mut close = false;
        if let Some(picker) = &mut self.picker {
            let (title_, description, icon) = match picker.kind {
                PickerKind::Screen => (
                    "Escolha uma tela",
                    "Selecione o monitor que você quer compartilhar.",
                    Icon::Monitor,
                ),
                PickerKind::Window => (
                    "Escolha uma janela",
                    "Compartilhe apenas o aplicativo que importa.",
                    Icon::Window,
                ),
                PickerKind::Camera => (
                    "Escolha sua câmera",
                    "Conecte um dispositivo para aparecer na composição.",
                    Icon::Camera,
                ),
            };
            let result = Modal::new(Id::new("source_picker"))
                .frame(modal_frame())
                .show(ctx, |ui| {
                    ui.set_width((ctx.content_rect().width() - 80.0).min(530.0));
                    ui.horizontal(|ui| {
                        ui.label(title(title_, 24.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if icon_button(ui, Icon::Close, "Fechar seleção").clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.label(muted(description));
                    ui.add_space(12.0);
                    ui.add(
                        TextEdit::singleline(&mut picker.search)
                            .hint_text("Buscar pelo nome…")
                            .desired_width(f32::INFINITY)
                            .margin(vec2(12.0, 10.0)),
                    );
                    ui.add_space(6.0);
                    if busy || picker.receiver.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(&picker.message);
                        });
                    }
                    ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 300.0).clamp(100.0, 360.0))
                        .show(ui, |ui| {
                            let query = picker.search.trim().to_lowercase();
                            let mut count = 0;
                            for (i, choice) in picker
                                .choices
                                .iter()
                                .enumerate()
                                .filter(|(_, c)| c.name().to_lowercase().contains(&query))
                            {
                                count += 1;
                                let response = ui
                                    .push_id(i, |ui| {
                                        ui.add_enabled_ui(!busy, |ui| {
                                            add_source_tile(
                                                ui,
                                                icon,
                                                choice.name(),
                                                choice.detail(),
                                            )
                                        })
                                        .inner
                                    })
                                    .inner;
                                if response.clicked() {
                                    selected = Some(choice.clone());
                                }
                            }
                            if count == 0 && picker.receiver.is_none() {
                                ui.add_space(16.0);
                                ui.label(if picker.choices.is_empty() {
                                    &picker.message
                                } else {
                                    "Nenhum resultado para essa busca."
                                });
                                ui.add_space(16.0);
                            }
                        });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled_ui(!busy && picker.receiver.is_none(), |ui| {
                                button(
                                    ui,
                                    Icon::Refresh,
                                    "Atualizar lista",
                                    ButtonKind::Secondary,
                                    156.0,
                                )
                            })
                            .inner
                            .clicked()
                        {
                            refresh = Some(picker.kind);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if button(ui, Icon::Close, "Cancelar", ButtonKind::Quiet, 110.0)
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });
                });
            close |= result.should_close();
        }
        if close {
            self.picker = None;
            self.picker_target = None;
        }
        if let Some(kind) = refresh {
            self.open_picker(ctx, kind, self.picker_target);
        }
        if !close {
            match selected {
                Some(Choice::Desktop(kind, source)) => {
                    self.start_desktop(ctx, kind, source);
                }
                Some(Choice::Camera(device)) => {
                    self.start_camera(ctx, device);
                }
                None => {}
            }
        }
    }

    fn dialogs(&mut self, ctx: &Context) {
        if self.help_open {
            let mut close = false;
            let result=Modal::new(Id::new("help")).area(Modal::default_area(Id::new("help")).default_size(vec2(620.0, ctx.content_rect().height() - 40.0))).frame(modal_frame()).show(ctx,|ui|{ScrollArea::vertical().max_height(ctx.content_rect().height()-100.0).show(ui,|ui|{
                ui.set_width(550.0);
                ui.horizontal(|ui|{ui.label(title("Seu estúdio em poucos passos",24.0));ui.with_layout(Layout::right_to_left(Align::Center),|ui|{if icon_button(ui,Icon::Close,"Fechar ajuda").clicked(){close=true;}});});
                ui.label(muted("Mini Studio 1.0.0 · Feito para criar e apresentar."));ui.add_space(12.0);
                for (n,heading,body) in [("01","Crie suas cenas","Use + no painel Cenas, no rodapé. A cena começa vazia; dê um nome e monte sua composição. O menu ••• permite renomear e duplicar."),("02","Monte sua composição","Use + em Fontes para adicionar telas, janelas, câmeras e imagens, inclusive várias do mesmo tipo. A fonte no topo da lista aparece na frente."),("03","Encontre o melhor enquadramento","Selecione e arraste qualquer fonte na prévia. Use o canto inferior direito para redimensionar, as setas para reordenar e o cadeado para bloquear. Dê dois cliques para abrir Propriedades."),("04","Leve sua imagem para outro aplicativo","Inicie a câmera virtual e escolha Mini Studio RS Cam no aplicativo de chamada. Ou salve a composição como PNG.")] {
                    ui.horizontal_top(|ui|{badge(ui,n,ACCENT);ui.vertical(|ui|{ui.label(title(heading,15.0));ui.label(muted(body));});});ui.add_space(6.0);
                }
                ui.separator();
                ui.horizontal(|ui|{section(ui,"ATALHOS");ui.label(muted("Ctrl+O  Imagem     Ctrl+S  Cena     Ctrl+Shift+S  PNG     Del  Remover"));});
                ui.add_space(4.0);section(ui,"TAMANHO DA INTERFACE");
                ui.horizontal(|ui| {for scale in [0.9,1.0,1.1,1.25]{if ui.selectable_label((self.scale-scale).abs()<0.01,format!("{:.0}%",scale*100.0)).clicked(){self.scale=scale;ctx.set_zoom_factor(scale);}}});
                ui.label(muted("Cenas e ajustes são salvos automaticamente. Ao reabrir, clique em Ativar fontes salvas. Durante a sessão, trocar de cena abre suas fontes."));
            });});
            if close || result.should_close() {
                self.help_open = false;
            }
        }
        if self.setup_open {
            let mut close = false;
            let result=Modal::new(Id::new("vcam_setup")).area(Modal::default_area(Id::new("vcam_setup")).default_size(vec2(570.0, ctx.content_rect().height() - 40.0))).frame(modal_frame()).show(ctx,|ui|{ScrollArea::vertical().max_height(ctx.content_rect().height()-100.0).show(ui,|ui|{
                ui.set_width(500.0);
                ui.horizontal(|ui|{ui.label(title("Prepare sua câmera virtual",23.0));ui.with_layout(Layout::right_to_left(Align::Center),|ui|{if icon_button(ui,Icon::Close,"Fechar configuração").clicked(){close=true;}});});
                ui.label(muted("Uma configuração rápida, uma única vez."));ui.add_space(10.0);
                card().show(ui,|ui|{ui.label(title("1. Instale o Mini Studio",16.0));ui.label(muted("Use o instalador .msi que acompanha esta versão. Ele configura a câmera virtual automaticamente. O Windows solicita permissão de administrador."));});
                card().show(ui,|ui|{ui.label(title("2. Inicie e escolha sua câmera",16.0));ui.label(muted("Volte ao estúdio, clique em Iniciar câmera virtual e selecione Mini Studio RS Cam no aplicativo de chamada."));});
                ui.label(RichText::new("A câmera virtual requer Windows 11. No Windows 10, use a composição e a exportação PNG normalmente.").size(12.0).color(AMBER));
                ui.add_space(6.0);
                if button(ui,Icon::Folder,"Abrir pasta do aplicativo",ButtonKind::Secondary,ui.available_width()).clicked() && let Ok(exe)=std::env::current_exe(){reveal_file(&exe);}
                if button(ui,Icon::Refresh,"Verificar instalação",ButtonKind::Primary,ui.available_width()).clicked(){if virtual_camera::installed(){close=true;self.notice_error=false;self.notice="Câmera virtual pronta para iniciar.".into();}else{self.show_error("O componente da câmera virtual ainda não foi encontrado. Execute o instalador desta versão.".into());close=true;}}
            });});
            if close || result.should_close() {
                self.setup_open = false;
            }
        }
    }

    fn shortcuts(&mut self, ctx: &Context) {
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) {
            self.help_open = !self.help_open;
        }
        if self.picker.is_some()
            || self.scene_dialog.is_some()
            || self.properties.is_some()
            || self.help_open
            || self.setup_open
            || ctx.wants_keyboard_input()
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::O))
            && self.image_receiver.is_none()
        {
            self.open_image(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::S))
            && self.has_sources()
            && self.save_receiver.is_none()
        {
            self.save_snapshot(ctx);
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::S)) {
            self.persist_scenes(true);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete))
            && let Some(id) = self.selected
        {
            self.remove_source(id);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.selected = None;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::D))
            && let Some(id) = self.selected
        {
            self.duplicate_source(ctx, id);
        }
        let files = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect::<Vec<_>>()
        });
        for path in files {
            self.add_input(ctx, SourceInput::Image(path));
        }
    }
    fn render(&mut self, ctx: &Context) {
        self.receive_frames(ctx);
        self.shortcuts(ctx);
        if self.notice != self.notice_seen {
            self.notice_seen = self.notice.clone();
            self.notice_at = Instant::now();
        }
        self.header(ctx);
        self.footer(ctx);
        self.bottom_dock(ctx);
        self.selection_strip(ctx);
        CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(16))
            .show(ctx, |ui| self.canvas(ui, ctx));
        self.properties_dialog(ctx);
        self.draw_picker(ctx);
        self.dialogs(ctx);
        self.scene_dialogs(ctx);
        if let Some(output) = &self.virtual_camera {
            output.update(self.scene());
        }
        if self.preferences_at.elapsed() > Duration::from_secs(2) {
            self.persist_preferences();
            self.persist_scenes(false);
            self.preferences_at = Instant::now();
        }
        ctx.request_repaint_after(Duration::from_secs(2));
        self.benchmark_tick(ctx);
    }
}
impl eframe::App for Studio {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.render(ctx);
    }
    fn on_exit(&mut self) {
        self.persist_preferences();
        self.persist_scenes(false);
        self.stop_all();
    }
}

pub(crate) fn small_icon(ui: &mut Ui, icon: Icon, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 4, HOVER);
    }
    paint_icon(
        ui.painter(),
        icon,
        rect.shrink(6.0),
        if ui.is_enabled() {
            TEXT
        } else {
            MUTED.gamma_multiply(0.3)
        },
    );
    response.on_hover_text(label)
}
pub(crate) fn glyph_button(ui: &mut Ui, glyph: &str, label: &str) -> Response {
    let response = ui.add_sized([28.0, 28.0], Button::new(glyph));
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
    response.on_hover_text(label)
}
fn percent(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) {
    let mut percent = *value * 100.0;
    ui.horizontal(|ui| {
        ui.add_sized([100.0, 24.0], Label::new(muted(label)));
        ui.add(
            Slider::new(
                &mut percent,
                (*range.start() * 100.0)..=(*range.end() * 100.0),
            )
            .suffix("%")
            .step_by(0.5),
        );
    });
    *value = percent / 100.0;
}
fn drag_transform(mut start: Transform, delta: [f32; 2], resize: bool) -> Transform {
    if resize {
        let dx = delta[0] / start.width;
        let dy = delta[1] / start.height;
        let scale = 1.0 + if dx.abs() > dy.abs() { dx } else { dy };
        let max = ((1.0 - start.x) / start.width)
            .min((1.0 - start.y) / start.height)
            .max(0.01);
        let min = (0.01 / start.width).max(0.01 / start.height).min(max);
        let scale = scale.clamp(min, max);
        start.width *= scale;
        start.height *= scale;
    } else {
        start.x = (start.x + delta[0]).clamp(0.0, (1.0 - start.width).max(0.0));
        start.y = (start.y + delta[1]).clamp(0.0, (1.0 - start.height).max(0.0));
    }
    start
}

fn add_source_tile(ui: &mut Ui, icon: Icon, name: &str, detail: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), name));
    let hovered = response.hovered() || response.has_focus();
    let p = ui.painter();
    p.rect_filled(rect, 10, if hovered { HOVER } else { CARD });
    p.rect_stroke(
        rect,
        10,
        Stroke::new(1.0_f32, if response.has_focus() { ACCENT } else { LINE }),
        StrokeKind::Inside,
    );
    let opacity = if ui.is_enabled() { 1.0 } else { 0.35 };
    paint_icon(
        p,
        icon,
        Rect::from_center_size(pos2(rect.left() + 28.0, rect.center().y), vec2(22.0, 22.0)),
        if hovered { ACCENT } else { MUTED }.gamma_multiply(opacity),
    );
    let text_p = p.with_clip_rect(Rect::from_min_max(
        pos2(rect.left() + 50.0, rect.top()),
        pos2(rect.right() - 12.0, rect.bottom()),
    ));
    text_p.text(
        pos2(rect.left() + 50.0, rect.top() + 13.0),
        Align2::LEFT_TOP,
        name,
        FontId::proportional(14.0),
        TEXT.gamma_multiply(opacity),
    );
    text_p.text(
        pos2(rect.left() + 50.0, rect.top() + 34.0),
        Align2::LEFT_TOP,
        detail,
        FontId::proportional(11.0),
        MUTED.gamma_multiply(opacity),
    );
    response
        .on_hover_text(format!("{name}\n{detail}"))
        .on_hover_cursor(CursorIcon::PointingHand)
}

fn reveal_file(path: &Path) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .creation_flags(0x08000000)
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, composition::Pixels, image_source};
    use std::collections::HashMap;
    struct Harness {
        ctx: Context,
        app: Studio,
        size: Vec2,
        controls: HashMap<String, Rect>,
        time: f64,
    }
    impl Harness {
        fn new(size: Vec2) -> Self {
            let ctx = Context::default();
            ctx.enable_accesskit();
            let app = Studio::initialize(
                &ctx,
                "Teste".into(),
                Options {
                    screenshot: Some("unused.png".into()),
                    ..Default::default()
                },
            );
            Self {
                ctx,
                app,
                size,
                controls: HashMap::new(),
                time: 0.0,
            }
        }
        fn frame(&mut self, events: Vec<Event>) {
            self.time += 1.0 / 60.0;
            let output = self.ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                    time: Some(self.time),
                    events,
                    ..Default::default()
                },
                |ctx| self.app.render(ctx),
            );
            self.controls.clear();
            for (_, node) in output.platform_output.accesskit_update.unwrap().nodes {
                if matches!(
                    node.role(),
                    accesskit::Role::Button
                        | accesskit::Role::CheckBox
                        | accesskit::Role::MenuItem
                        | accesskit::Role::ComboBox
                        | accesskit::Role::Image
                ) && let (Some(label), Some(bounds)) = (node.label(), node.bounds())
                {
                    self.controls.insert(
                        label.to_owned(),
                        Rect::from_min_max(
                            pos2(bounds.x0 as f32, bounds.y0 as f32),
                            pos2(bounds.x1 as f32, bounds.y1 as f32),
                        ),
                    );
                }
            }
        }
        fn click(&mut self, label: &str) {
            self.time += 0.5;
            self.frame(vec![]);
            let pos = self
                .controls
                .get(label)
                .unwrap_or_else(|| panic!("Controle ausente: {label}; {:?}", self.controls.keys()))
                .center();
            self.frame(vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ]);
            self.frame(vec![Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }]);
        }
        fn image(&mut self, name: &str, color: [u8; 4]) -> Source {
            self.app.accept_image(
                &self.ctx,
                image_source::LoadedImage {
                    path: format!("{name}.png").into(),
                    name: name.into(),
                    pixels: Pixels {
                        width: 16,
                        height: 9,
                        rgba: color.repeat(144).into(),
                    },
                },
                None,
            );
            self.frame(vec![]);
            self.app.selected.unwrap()
        }
        fn drag(&mut self, from: Pos2, to: Pos2) {
            self.time += 0.5;
            self.frame(vec![
                Event::PointerMoved(from),
                Event::PointerButton {
                    pos: from,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ]);
            self.frame(vec![Event::PointerMoved(to)]);
            self.frame(vec![Event::PointerButton {
                pos: to,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }]);
        }
    }
    #[test]
    fn dock_controls_stay_below_preview_at_both_window_sizes() {
        for size in [vec2(1440.0, 900.0), vec2(1024.0, 720.0)] {
            let mut h = Harness::new(size);
            h.frame(vec![]);
            h.frame(vec![]);
            let preview = h.controls["Prévia da cena"];
            let scenes = h.controls["Nova cena"];
            let output = h.controls["Iniciar câmera virtual"];
            assert!(scenes.top() > preview.bottom());
            assert!(output.top() > preview.bottom());
            assert!(output.right() <= size.x && output.bottom() < size.y);
            assert!((preview.center().x - size.x / 2.0).abs() < 2.0);
            assert!(preview.width() > size.x * 0.4);
        }
    }
    #[test]
    fn two_images_mix_reorder_hide_duplicate_and_remove_independently() {
        let mut h = Harness::new(vec2(1440.0, 900.0));
        let red = h.image("Fundo", [255, 0, 0, 255]);
        let green = h.image("Logo", [0, 255, 0, 255]);
        h.app.source_mut(green).unwrap().transform = Transform {
            x: 0.5,
            y: 0.5,
            width: 0.5,
            height: 0.5,
            ..Default::default()
        };
        assert_eq!(h.app.scene().sources.len(), 2);
        assert_eq!(
            h.app.scene().compose().unwrap().get_pixel(900, 500).0,
            [0, 255, 0, 255]
        );
        h.click("Fundo");
        h.click("Mover fonte para cima");
        assert_eq!(
            h.app.scene().compose().unwrap().get_pixel(900, 500).0,
            [255, 0, 0, 255]
        );
        h.app.source_mut(red).unwrap().visible = false;
        assert_eq!(
            h.app.scene().compose().unwrap().get_pixel(900, 500).0,
            [0, 255, 0, 255]
        );
        h.click("Logo");
        h.app.duplicate_source(&h.ctx, green);
        let copy = h.app.selected.unwrap();
        assert_ne!(copy, green);
        assert!(std::rc::Rc::ptr_eq(
            &h.app.runtimes[&copy],
            &h.app.runtimes[&green]
        ));
        h.app.source_mut(copy).unwrap().name = "Outro logo".into();
        h.click("Remover fonte");
        assert!(h.app.source(copy).is_none());
        assert!(h.app.source(green).is_some());
        assert_eq!(h.app.source(green).unwrap().name, "Logo");
    }
    #[test]
    fn preview_moves_resizes_and_respects_source_lock() {
        let mut h = Harness::new(vec2(1440.0, 900.0));
        let id = h.image("Arte", [255; 4]);
        h.app.source_mut(id).unwrap().transform = Transform {
            x: 0.2,
            y: 0.2,
            width: 0.3,
            height: 0.3,
            ..Default::default()
        };
        h.frame(vec![]);
        let preview = h.controls["Prévia da cena"];
        let point = |x, y| preview.min + preview.size() * vec2(x, y);
        h.drag(point(0.35, 0.35), point(0.45, 0.45));
        let moved = h.app.source(id).unwrap().transform;
        assert!((moved.x - 0.3).abs() < 0.01);
        h.click("Bloquear fonte");
        let before = h.app.source(id).unwrap().transform;
        h.drag(point(0.45, 0.45), point(0.55, 0.55));
        assert_eq!(h.app.source(id).unwrap().transform, before);
        h.click("Arte");
        h.click("Desbloquear fonte");
        h.drag(point(0.6, 0.6), point(0.7, 0.7));
        assert!(
            h.app.source(id).unwrap().transform.width > before.width,
            "before {:?}, after {:?}, selected {:?}, properties {:?}, drag {:?}",
            before,
            h.app.source(id).unwrap().transform,
            h.app.selected,
            h.app.properties,
            h.app.canvas_drag
        );
    }
    #[test]
    fn scene_creation_rename_and_duplicate_use_empty_compositions() {
        let mut h = Harness::new(vec2(1024.0, 720.0));
        h.image("Fundo", [255; 4]);
        h.click("Nova cena");
        if let Some(crate::scene_ui::SceneDialog::Create { name }) = &mut h.app.scene_dialog {
            *name = "Apresentação".into();
        }
        h.click("Criar cena");
        assert_eq!(h.app.scenes.active, 1);
        assert!(h.app.current_scene().sources.is_empty());
        h.click("Cena 1");
        assert_eq!(h.app.current_scene().sources.len(), 1);
        h.click("•••");
        h.click("Renomear cena");
        if let Some(crate::scene_ui::SceneDialog::Rename { name }) = &mut h.app.scene_dialog {
            *name = "Aula".into();
        }
        h.click("Salvar nome");
        assert_eq!(h.app.current_scene().name, "Aula");
        h.click("•••");
        h.click("Duplicar cena");
        assert_eq!(h.app.scenes.active, 2);
        assert_ne!(
            h.app.scenes.scenes[0].sources[0].id,
            h.app.scenes.scenes[2].sources[0].id
        );
        h.app.current_scene_mut().sources[0].visible = false;
        assert!(h.app.scenes.scenes[0].sources[0].visible);
    }
    #[test]
    fn image_reloads_after_scene_switch_and_failed_files_remain_configured() {
        let mut h = Harness::new(vec2(1024.0, 720.0));
        let path =
            std::env::temp_dir().join(format!("studio-multi-image-{}.png", std::process::id()));
        image::RgbaImage::from_pixel(16, 9, image::Rgba([30, 50, 70, 255]))
            .save(&path)
            .unwrap();
        let first = h.app.add_input(&h.ctx, SourceInput::Image(path.clone()));
        let second = h.app.add_input(&h.ctx, SourceInput::Image(path.clone()));
        assert!(std::rc::Rc::ptr_eq(
            &h.app.runtimes[&first],
            &h.app.runtimes[&second]
        ));
        h.app.source_mut(second).unwrap().visible = false;
        h.app.scenes.scenes.push(crate::scenes::SavedScene {
            name: "Vazia".into(),
            sources: vec![],
        });
        h.app.switch_scene(&h.ctx, 1);
        h.frame(vec![]);
        assert!(h.app.runtimes.is_empty());
        h.app.switch_scene(&h.ctx, 0);
        let deadline = Instant::now() + Duration::from_secs(5);
        while h.app.runtimes.values().any(|r| r.borrow().loading_now()) && Instant::now() < deadline
        {
            h.frame(vec![]);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(h.app.scene().sources.len(), 1);
        assert!(!h.app.source(second).unwrap().visible);
        std::fs::remove_file(path).unwrap();
        h.app.switch_scene(&h.ctx, 1);
        h.app.switch_scene(&h.ctx, 0);
        let deadline = Instant::now() + Duration::from_secs(5);
        while h.app.runtimes.values().any(|r| r.borrow().loading_now()) && Instant::now() < deadline
        {
            h.frame(vec![]);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(h.app.notice_error);
        assert_eq!(h.app.current_scene().sources.len(), 2);
        assert!(h.app.scene().sources.is_empty());
    }
    #[test]
    fn properties_block_delete_and_old_replacement_cannot_overwrite_a_new_source() {
        let mut h = Harness::new(vec2(1024.0, 720.0));
        let id = h.image("Original", [255; 4]);
        h.click("Propriedades");
        h.frame(vec![Event::Key {
            key: Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        assert!(h.app.source(id).is_some());
        h.click("Concluir");
        h.app.remove_source(id);
        let new_id = h.image("Nova", [0, 0, 255, 255]);
        assert_ne!(id, new_id);
        h.app.accept_image(
            &h.ctx,
            image_source::LoadedImage {
                path: "old.png".into(),
                name: "Antiga".into(),
                pixels: Pixels {
                    width: 1,
                    height: 1,
                    rgba: vec![255; 4].into(),
                },
            },
            Some(id),
        );
        assert_eq!(h.app.current_scene().sources.len(), 1);
        assert_eq!(h.app.source(new_id).unwrap().name, "Nova");
    }
    #[test]
    fn dragging_and_resizing_stay_in_bounds() {
        let t = Transform {
            x: 0.2,
            y: 0.1,
            width: 0.4,
            height: 0.3,
            ..Default::default()
        };
        let moved = drag_transform(t, [4.0, -5.0], false);
        assert_eq!(moved.x, 0.6);
        assert_eq!(moved.y, 0.0);
        let resized = drag_transform(t, [8.0, 9.0], true);
        assert!(resized.x + resized.width <= 1.0001);
        assert!(resized.y + resized.height <= 1.0001);
        assert!((resized.width / resized.height - t.width / t.height).abs() < 0.0001);
    }
}
