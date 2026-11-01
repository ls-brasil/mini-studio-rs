use crate::{
    Choice, PickerKind, Studio,
    canvas_editor::{self, Drag, Handle},
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
    pub(crate) fn sync_library(&mut self) {
        for id in self.scenes.reconcile_sources() {
            if self.runtimes.remove(&id).is_some() {
                self.library_restarts.push(id);
            }
        }
    }
    pub(crate) fn accept_library_input(&mut self, input: SourceInput) -> bool {
        let Some(target) = self.library_target.take() else {
            return false;
        };
        if target == 0 {
            let id = self.scenes.sources.iter().map(|s| s.id).max().unwrap_or(0) + 1;
            self.scenes.sources.push(crate::scenes::SourceDefinition {
                id,
                name: input.name(),
                input,
            });
        } else if let Some(definition) = self.scenes.sources.iter_mut().find(|s| s.id == target) {
            definition.input = input;
            let definition = definition.clone();
            for item in self
                .scenes
                .scenes
                .iter_mut()
                .flat_map(|s| &mut s.sources)
                .filter(|s| s.source_id == target)
            {
                item.input = definition.input.clone();
                if self.runtimes.remove(&item.id).is_some() {
                    self.library_restarts.push(item.id);
                }
            }
        }
        true
    }
    fn library_dock(&mut self, ui: &mut Ui, ctx: &Context) {
        self.sync_library();
        ui.horizontal(|ui| {
            ui.label(title("Fontes", 16.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button("+", |ui| {
                    for (label, kind) in [
                        ("Tela", Some(PickerKind::Screen)),
                        ("Janela", Some(PickerKind::Window)),
                        ("Câmera", Some(PickerKind::Camera)),
                        ("NDI", Some(PickerKind::Ndi)),
                        ("Imagem", None),
                    ] {
                        if ui
                            .add_enabled(
                                self.picker.is_none() && self.image_receiver.is_none(),
                                Button::new(label),
                            )
                            .clicked()
                        {
                            if let Some(kind) = kind {
                                self.open_picker(ctx, kind, None);
                            } else {
                                self.open_image(ctx);
                            }
                            self.library_target = Some(0);
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("Nova fonte")
                .widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "Nova fonte"));
            });
        });
        ui.label(muted("Arraste para uma cena"));
        ui.add_space(4.0);
        ScrollArea::vertical().id_salt("library_sources").show(ui, |ui| {
            if self.scenes.sources.is_empty() { ui.label(muted("Use + para cadastrar sua primeira fonte.")); }
            for definition in self.scenes.sources.clone() {
                ui.push_id(definition.id, |ui| {
                    Frame::new().fill(if self.library_edit == Some(definition.id) { ACCENT_DARK } else { Color32::TRANSPARENT }).corner_radius(4).inner_margin(4).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 38.0).max(30.0);
                            let drag = ui.dnd_drag_source(Id::new(("library_drag", definition.id)), definition.id, |ui| {
                                ui.horizontal(|ui| {
                                    let icon = match &definition.input { SourceInput::Camera(_) => Icon::Camera, SourceInput::Image(_) => Icon::Image, SourceInput::Desktop(d) if d.window => Icon::Window, _ => Icon::Monitor };
                                    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 28.0), Sense::hover());
                                    paint_icon(ui.painter(), icon, rect.shrink2(vec2(0.0, 5.0)), MUTED);
                                    ui.add_sized([(width - 26.0).max(10.0), 28.0], Label::new(&definition.name).truncate()).widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Arrastar fonte: {}", definition.name)));
                                });
                            });
                            if drag.response.double_clicked() { self.library_edit = Some(definition.id); }
                            drag.response.on_hover_text(format!("{} · {}\nArraste para uma cena. Use a engrenagem para configurar.", definition.name, definition.input.label()));
                            if small_icon(ui, Icon::Settings, &format!("Configurar fonte: {}", definition.name)).clicked() { self.library_edit = Some(definition.id); }
                        });
                    });
                });
            }
        });
    }
    fn library_editor(&mut self, ui: &mut Ui, ctx: &Context, id: u64) {
        let Some(mut definition) = self.scenes.sources.iter().find(|s| s.id == id).cloned() else {
            self.library_edit = None;
            return;
        };
        ui.horizontal(|ui| {
            ui.label(title("Configurar fonte", 22.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Voltar à prévia").clicked() {
                    self.library_edit = None;
                }
            });
        });
        ui.separator();
        ScrollArea::vertical().id_salt("source_editor").show(ui, |ui| {
            ui.set_max_width(760.0);
            ui.add_space(12.0);
            ui.label(muted("Nome na biblioteca"));
            if ui.add(TextEdit::singleline(&mut definition.name).desired_width(f32::INFINITY).char_limit(80)).changed() && !definition.name.trim().is_empty() {
                self.scenes.sources.iter_mut().find(|s| s.id == id).unwrap().name = definition.name.clone();
                for item in self.scenes.scenes.iter_mut().flat_map(|scene| &mut scene.sources).filter(|s| s.source_id == id) { item.name = definition.name.clone(); }
            }
            ui.add_space(16.0);
            ui.label(title(definition.input.label(), 16.0));
            let detail = match &definition.input { SourceInput::Image(path) => path.display().to_string(), _ => definition.input.name() };
            ui.add(Label::new(detail).wrap());
            if ui.add_enabled(self.picker.is_none() && self.image_receiver.is_none(), Button::new("Alterar origem")).clicked() {
                match &definition.input {
                    SourceInput::Image(_) => self.open_image(ctx),
                    SourceInput::Camera(_) => self.open_picker(ctx, PickerKind::Camera, None),
                    SourceInput::Ndi(_) => self.open_picker(ctx, PickerKind::Ndi, None),
                    SourceInput::Desktop(d) => self.open_picker(ctx, if d.window { PickerKind::Window } else { PickerKind::Screen }, None),
                }
                self.library_target = Some(id);
            }
            if let SourceInput::Desktop(desktop) = &mut definition.input
                && ui.checkbox(&mut desktop.capture_cursor, "Capturar ponteiro do mouse").changed() {
                self.library_target = Some(id);
                self.accept_library_input(definition.input.clone());
            }
            let uses: Vec<_> = self.scenes.scenes.iter().filter(|scene| scene.sources.iter().any(|s| s.source_id == id)).map(|scene| scene.name.clone()).collect();
            ui.add_space(16.0);
            ui.label(title("Utilização", 16.0));
            ui.add(Label::new(if uses.is_empty() { "Esta fonte ainda não foi adicionada a uma cena.".into() } else { format!("Usada em: {}", uses.join(", ")) }).wrap());
            ui.label(muted("Nome e origem são compartilhados. Posição e tamanho são definidos em cada cena."));
            ui.add_space(16.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Criar cópia independente").clicked() {
                    definition.id = self.scenes.sources.iter().map(|s| s.id).max().unwrap_or(0) + 1;
                    definition.name = format!("{} — cópia", definition.name);
                    self.library_edit = Some(definition.id);
                    self.scenes.sources.push(definition.clone());
                }
                if ui.add_enabled(uses.is_empty(), Button::new("Excluir fonte")).on_hover_text("Remova a fonte das cenas antes de excluí-la da biblioteca.").clicked() {
                    self.scenes.sources.retain(|s| s.id != id);
                    self.library_edit = None;
                }
            });
        });
    }
    pub(crate) fn add_library_to_scene(
        &mut self,
        ctx: &Context,
        scene_index: usize,
        definition_id: u64,
    ) {
        let Some(definition) = self
            .scenes
            .sources
            .iter()
            .find(|s| s.id == definition_id)
            .cloned()
        else {
            return;
        };
        if scene_index >= self.scenes.scenes.len() {
            return;
        }
        let id = self.allocate_source_id();
        self.scenes.scenes[scene_index]
            .sources
            .push(crate::scenes::SavedSource {
                id,
                source_id: definition.id,
                name: definition.name,
                input: definition.input,
                visible: true,
                locked: false,
                transform: Default::default(),
            });
        if scene_index == self.scenes.active {
            self.selected = Some(id);
            self.activate_source(ctx, id);
        }
        self.notice = format!(
            "Fonte adicionada à cena ‘{}’.",
            self.scenes.scenes[scene_index].name
        );
        self.notice_error = false;
    }
    fn library_choices(&mut self, ui: &mut Ui, ctx: &Context) {
        if self.scenes.sources.is_empty() {
            ui.label("Cadastre uma fonte no painel à esquerda.");
        }
        for definition in self.scenes.sources.clone() {
            if ui.button(&definition.name).clicked() {
                let id = self.allocate_source_id();
                self.current_scene_mut()
                    .sources
                    .push(crate::scenes::SavedSource {
                        id,
                        source_id: definition.id,
                        name: definition.name,
                        input: definition.input,
                        visible: true,
                        locked: false,
                        transform: Default::default(),
                    });
                self.selected = Some(id);
                self.activate_source(ctx, id);
                ui.close();
            }
        }
    }
    fn has_sources(&self) -> bool {
        !self.current_scene().sources.is_empty()
    }
    fn remove_source(&mut self, id: Source) {
        self.transform_undo.retain(|(source, _)| *source != id);
        self.transform_redo.retain(|(source, _)| *source != id);
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
        self.sync_library();
        let Some(mut source) = self.source(id).cloned() else {
            return;
        };
        source.id = self.allocate_source_id();
        // Duplicating an item keeps its shared source and independent composition.
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
        self.remember_transform(id);
        if let Some(source) = self.source_mut(id) {
            source.transform = Transform {
                mirror: source.transform.mirror,
                opacity: source.transform.opacity,
                crop: source.transform.crop,
                ..Default::default()
            };
        }
    }
    fn remember_transform(&mut self, id: Source) {
        if let Some(source) = self.source(id) {
            self.transform_undo.push((id, source.transform));
            if self.transform_undo.len() > 100 {
                self.transform_undo.remove(0);
            }
            self.transform_redo.clear();
        }
    }
    fn undo_transform(&mut self, redo: bool) {
        let entry = if redo {
            self.transform_redo.pop()
        } else {
            self.transform_undo.pop()
        };
        if let Some((id, previous)) = entry
            && let Some(source) = self.source_mut(id)
        {
            let current = std::mem::replace(&mut source.transform, previous);
            if redo {
                self.transform_undo.push((id, current));
            } else {
                self.transform_redo.push((id, current));
            }
            self.selected = Some(id);
        }
    }
    fn center_source(&mut self, id: Source) {
        if let Some(layer) = self.scene().layers().iter().find(|l| l.source == Some(id)) {
            let rect = layer.rect;
            self.remember_transform(id);
            if let Some(source) = self.source_mut(id) {
                source.transform.x = (1.0 - rect[2] / WIDTH as f32) * 0.5;
                source.transform.y = (1.0 - rect[3] / HEIGHT as f32) * 0.5;
                source.transform.width = rect[2] / WIDTH as f32;
                source.transform.height = rect[3] / HEIGHT as f32;
                source.transform.stretch = true;
            }
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
            .default_height(250.0)
            .height_range(200.0..=(ctx.content_rect().height() * 0.55).max(200.0))
            .frame(Frame::new().fill(LINE).inner_margin(1))
            .show(ctx, |ui| {
                let available = ui.available_size();
                let gap = 2.0;
                let width = available.x - gap * 3.0;
                ui.spacing_mut().item_spacing = vec2(gap, 6.0);
                ui.horizontal_top(|ui| {
                    for (index, ratio) in [0.32, 0.20, 0.28, 0.20].into_iter().enumerate() {
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
                                        0 => self.library_dock(ui, ctx),
                                        1 => self.scenes_dock(ui, ctx),
                                        2 => self.sources_dock(ui, ctx),
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
            ui.label(title("Fontes da cena", 16.0));
            ui.label(muted(format!("{}", self.current_scene().sources.len())));
        });
        TopBottomPanel::bottom("source_toolbar")
            .frame(Frame::NONE)
            .show_inside(ui, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.menu_button("+", |ui| {
                        self.library_choices(ui, ctx);
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
                        if small_icon(ui, Icon::Fit, "Ajustar à tela").clicked()
                            && let Some(id) = selected
                        {
                            self.fit_source(id);
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
                    ui.label(muted("Escolha uma fonte na prévia ou use + abaixo."));
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
                                        SourceInput::Ndi(_) => Icon::Monitor,
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
                                            } else if r.waiting_ndi() {
                                                "Aguardando NDI…".into()
                                            } else if r.loading_now() {
                                                "Abrindo fonte…".into()
                                            } else {
                                                "".into()
                                            }
                                        })
                                        .unwrap_or_else(|| {
                                            "Preparando fonte automaticamente…".into()
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
                                let state = self
                                    .runtimes
                                    .get(&source.id)
                                    .map(|runtime| runtime.borrow().error.clone());
                                if state.is_none() || state.as_ref().is_some_and(Option::is_some) {
                                    let error = state.as_ref().and_then(|error| error.as_deref());
                                    ui.add(
                                        Label::new(
                                            RichText::new(error.unwrap_or("Captura interrompida"))
                                                .size(11.0)
                                                .color(AMBER),
                                        )
                                        .wrap(),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        if error.is_some() {
                                            ui.label(muted("Reconectando automaticamente…"));
                                        }
                                        let image = matches!(source.input, SourceInput::Image(_));
                                        if ui
                                            .add_enabled(
                                                !image || self.image_receiver.is_none(),
                                                Button::new(if image {
                                                    "Localizar imagem"
                                                } else {
                                                    "Escolher outra"
                                                })
                                                .small(),
                                            )
                                            .clicked()
                                        {
                                            self.replace_source_input(ctx, source.id);
                                        }
                                    });
                                }
                            });
                    });
                }
            });
    }
    fn replace_source_input(&mut self, ctx: &Context, id: Source) {
        let Some(input) = self.source(id).map(|source| source.input.clone()) else {
            return;
        };
        self.selected = Some(id);
        self.properties = None;
        match input {
            SourceInput::Image(_) => self.load_image(ctx, None, Some(id)),
            SourceInput::Camera(_) => self.open_picker(ctx, PickerKind::Camera, Some(id)),
            SourceInput::Ndi(_) => self.open_picker(ctx, PickerKind::Ndi, Some(id)),
            SourceInput::Desktop(desktop) => self.open_picker(
                ctx,
                if desktop.window {
                    PickerKind::Window
                } else {
                    PickerKind::Screen
                },
                Some(id),
            ),
        }
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
                    Icon::Grid,
                    "Visualização múltipla",
                    if self.multiview_open {
                        ButtonKind::Primary
                    } else {
                        ButtonKind::Secondary
                    },
                    ui.available_width(),
                )
                .clicked()
                {
                    self.multiview_open = !self.multiview_open;
                }
                ui.horizontal(|ui| {
                    let reserve = if self.projector.open { 36.0 } else { 0.0 };
                    if button(
                        ui,
                        Icon::Monitor,
                        "Projetor de saída",
                        if self.projector.open {
                            ButtonKind::Primary
                        } else {
                            ButtonKind::Secondary
                        },
                        (ui.available_width() - reserve).max(1.0),
                    )
                    .clicked()
                    {
                        self.projector.choose_destination();
                    }
                    if self.projector.open
                        && small_icon(ui, Icon::Close, "Fechar projetor").clicked()
                    {
                        self.projector.open = false;
                    }
                });
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
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(muted("Alças: tamanho · Alt: recorte · Shift: livre"))
                            .on_hover_text("Arraste a fonte para mover. Ctrl desativa o encaixe. Clique direito para mais opções. Ctrl+Z desfaz o enquadramento.");
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
                ui.set_width((ctx.content_rect().width() - 64.0).min(420.0));
                ui.horizontal(|ui| {
                    ui.label(title("Propriedades da fonte", 22.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if small_icon(ui, Icon::Close, "Fechar propriedades").clicked() {
                            close = true;
                        }
                    });
                });
                ui.separator();
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
                ui.add_space(6.0);
                if let SourceInput::Desktop(desktop) = &mut source.input {
                    activate |= ui
                        .checkbox(&mut desktop.capture_cursor, "Capturar ponteiro do mouse")
                        .changed();
                }
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                    paint_icon(ui.painter(), Icon::Mirror, rect, MUTED);
                    ui.checkbox(&mut source.transform.mirror, "Espelhar horizontalmente");
                });
                let mut opacity = source.transform.opacity as f32 / 255.0;
                percent(ui, "Opacidade", &mut opacity, 0.0..=1.0);
                source.transform.opacity = (opacity * 255.0).round() as u8;
                let image = matches!(source.input, SourceInput::Image(_));
                ui.add_space(6.0);
                if let Some(error) = &error {
                    ui.add(Label::new(RichText::new(error).color(AMBER)).truncate())
                        .on_hover_text(error);
                }
                ui.horizontal_wrapped(|ui| {
                    replace = ui
                        .add_enabled(
                            !image || self.image_receiver.is_none(),
                            Button::new("Escolher outra fonte"),
                        )
                        .clicked();
                });
                ui.separator();
                ui.label(muted("Nome e captura são compartilhados entre cenas. Enquadramento e aparência pertencem a este uso."));
                ui.label(muted(
                    "Mova e redimensione na prévia. Alt + arrastar recorta.",
                ));
                ui.horizontal(|ui| {
                    ui.label(muted("Alterações salvas automaticamente."));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Concluir").clicked() {
                            close = true;
                        }
                    });
                });
            });
        if activate {
            self.runtimes.remove(&id);
            self.activate_source(ctx, id);
        }
        if replace {
            self.replace_source_input(ctx, id);
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
        ui.interact(rect, Id::new("composition_canvas"), Sense::hover())
            .widget_info(|| WidgetInfo::labeled(WidgetType::Image, true, "Prévia da cena"));
        let response = ui.interact(
            outer,
            Id::new("composition_editor"),
            Sense::click_and_drag(),
        );
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
        self.paint_scene(&painter, rect, &scene);
        if !self.has_sources() {
            let content = Rect::from_center_size(
                outer.center(),
                vec2(outer.width().min(520.0) - 24.0, outer.height().min(200.0)),
            );
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(content)
                    .layout(Layout::top_down(Align::Center)),
                |ui| {
                    ui.label(title("Monte sua cena", 22.0));
                    ui.label(muted(
                        "Prepare as fontes à esquerda e escolha quais usar nesta cena.",
                    ));
                    ui.menu_button("Adicionar fonte da biblioteca", |ui| {
                        self.library_choices(ui, ctx)
                    });
                },
            );
        } else if scene.sources.is_empty() {
            let (heading, instruction) = if self
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
                .current_scene()
                .sources
                .iter()
                .filter(|source| source.visible)
                .any(|source| {
                    self.runtimes
                        .get(&source.id)
                        .is_some_and(|runtime| runtime.borrow().waiting_ndi())
                })
            {
                (
                    "Aguardando NDI…",
                    "Confira o transmissor e a rede; a conexão será retomada automaticamente",
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
                    "Aguardando fonte…",
                    "A reconexão é automática assim que a origem estiver disponível",
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
        let modifiers = ctx.input(|i| i.modifiers);
        if (response.clicked() || response.secondary_clicked() || response.drag_started())
            && let Some(pointer) = ctx
                .input(|i| i.pointer.press_origin())
                .or(response.interact_pointer_pos())
        {
            let handle = self
                .selected
                .filter(|id| self.source(*id).is_some_and(|s| !s.locked))
                .and_then(|id| layers.iter().find(|l| l.source == Some(id)))
                .and_then(|l| canvas_editor::hit_handle(convert(l.rect), pointer));
            if handle.is_none() {
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
                && let Some(source) = self.source(id).filter(|s| !s.locked)
                && let Some(layer) = layers.iter().find(|l| l.source == Some(id))
            {
                let original = source.transform;
                let mut start = Transform::from_rect(layer.rect);
                start.mirror = original.mirror;
                start.opacity = original.opacity;
                start.crop = original.crop;
                start.stretch = true;
                self.canvas_drag = Some(Drag {
                    id,
                    original,
                    start,
                    handle: handle.unwrap_or(Handle::Move),
                    crop: modifiers.alt,
                });
            }
        }
        if response.double_clicked() {
            self.properties = self.selected;
        }
        let targets: Vec<_> = std::iter::once([0.0, 0.0, 1.0, 1.0])
            .chain(
                layers
                    .iter()
                    .filter(|l| l.source.is_some() && l.source != self.selected)
                    .map(|l| {
                        [
                            l.rect[0] / WIDTH as f32,
                            l.rect[1] / HEIGHT as f32,
                            l.rect[2] / WIDTH as f32,
                            l.rect[3] / HEIGHT as f32,
                        ]
                    }),
            )
            .collect();
        if response.dragged()
            && let Some(drag) = self.canvas_drag
        {
            let delta = response.total_drag_delta().unwrap_or_default() / rect.size();
            let mut transformed = drag.apply(delta, modifiers.shift);
            if drag.handle == Handle::Move && !modifiers.ctrl {
                canvas_editor::snap_move(&mut transformed, &targets, vec2(7.0, 7.0) / rect.size());
            }
            if let Some(source) = self.source_mut(drag.id).filter(|s| !s.locked) {
                source.transform = transformed;
            }
            ctx.set_cursor_icon(if drag.handle == Handle::Move {
                CursorIcon::Grabbing
            } else {
                drag.handle.cursor()
            });
        }
        if response.drag_stopped()
            && let Some(drag) = self.canvas_drag.take()
            && self
                .source(drag.id)
                .is_some_and(|s| s.transform != drag.original)
        {
            self.transform_undo.push((drag.id, drag.original));
            if self.transform_undo.len() > 100 {
                self.transform_undo.remove(0);
            }
            self.transform_redo.clear();
        }
        if let Some(id) = self.selected
            && let Some(layer) = layers.iter().find(|l| l.source == Some(id))
            && let Some(source) = self.source(id)
        {
            let area = convert(layer.rect);
            canvas_editor::paint_selection(
                &ui.painter_at(outer),
                area,
                rect,
                source.transform,
                source.locked,
            );
            if !source.locked
                && !response.dragged()
                && let Some(pointer) = response.hover_pos()
            {
                if let Some(handle) = canvas_editor::hit_handle(area, pointer) {
                    ctx.set_cursor_icon(handle.cursor());
                } else if area.contains(pointer) {
                    ctx.set_cursor_icon(CursorIcon::Grab);
                }
            }
            if self.canvas_drag.is_some() && !modifiers.ctrl {
                let t = source.transform;
                for target in &targets {
                    for axis in 0..2 {
                        let (position, size) = if axis == 0 {
                            (t.x, t.width)
                        } else {
                            (t.y, t.height)
                        };
                        for line in [
                            target[axis],
                            target[axis] + target[axis + 2] * 0.5,
                            target[axis] + target[axis + 2],
                        ] {
                            if [position, position + size * 0.5, position + size]
                                .iter()
                                .any(|p| (p - line).abs() < 0.0001)
                            {
                                let (a, b) = if axis == 0 {
                                    (
                                        pos2(rect.left() + line * rect.width(), rect.top()),
                                        pos2(rect.left() + line * rect.width(), rect.bottom()),
                                    )
                                } else {
                                    (
                                        pos2(rect.left(), rect.top() + line * rect.height()),
                                        pos2(rect.right(), rect.top() + line * rect.height()),
                                    )
                                };
                                painter.line_segment(
                                    [a, b],
                                    Stroke::new(1.0_f32, Color32::from_rgb(90, 175, 255)),
                                );
                            }
                        }
                    }
                }
            }
        }
        response.context_menu(|ui| {
            if let Some(id) = self.selected {
                if ui.button("Propriedades do enquadramento").clicked() {
                    self.properties = Some(id);
                    ui.close();
                }
                let locked = self.source(id).is_some_and(|s| s.locked);
                ui.add_enabled_ui(!locked, |ui| {
                    if ui.button("Ajustar à tela").clicked() {
                        self.fit_source(id);
                        ui.close();
                    }
                    if ui.button("Centralizar na tela").clicked() {
                        self.center_source(id);
                        ui.close();
                    }
                    if ui.button("Espelhar horizontalmente").clicked() {
                        self.remember_transform(id);
                        let t = &mut self.source_mut(id).unwrap().transform;
                        t.mirror = !t.mirror;
                        ui.close();
                    }
                    if ui.button("Restaurar enquadramento").clicked() {
                        self.remember_transform(id);
                        let t = &mut self.source_mut(id).unwrap().transform;
                        *t = Transform {
                            opacity: t.opacity,
                            ..Default::default()
                        };
                        ui.close();
                    }
                });
            }
            ui.separator();
            if ui
                .add_enabled(
                    !self.transform_undo.is_empty(),
                    Button::new("Desfazer enquadramento   Ctrl+Z"),
                )
                .clicked()
            {
                self.undo_transform(false);
                ui.close();
            }
            if ui
                .add_enabled(
                    !self.transform_redo.is_empty(),
                    Button::new("Refazer enquadramento   Ctrl+Y"),
                )
                .clicked()
            {
                self.undo_transform(true);
                ui.close();
            }
        });
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
                PickerKind::Ndi => (
                    "Receber canal NDI",
                    "Escolha um transmissor na mesma rede. Recepção somente de vídeo.",
                    Icon::Monitor,
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
                    if matches!(picker.kind, PickerKind::Ndi) {
                        ui.label(muted("Requer o NDI Runtime instalado neste computador."));
                        ui.hyperlink_to("Baixar NDI Runtime", "https://ndi.link/NDIRedistV6");
                    }
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
            self.library_target = None;
            self.picker_target = None;
        }
        if let Some(kind) = refresh {
            self.refresh_picker(ctx, kind);
        }
        if !close {
            match selected {
                Some(Choice::Desktop(kind, source)) => {
                    self.start_desktop(ctx, kind, source);
                }
                Some(Choice::Camera(device)) => {
                    self.start_camera(ctx, device);
                }
                Some(Choice::Ndi(source)) => {
                    self.accept_input(ctx, SourceInput::Ndi(source));
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
                for (n,heading,body) in [("01","Crie suas cenas","Use + no painel Cenas, dê um nome e crie uma cena vazia. Arraste fontes da biblioteca para a cena. O menu ••• permite renomear e duplicar."),("02","Monte sua composição","Use + em Fontes para cadastrar Tela, Janela, Câmera, NDI ou Imagem. Configure pela engrenagem e arraste a fonte sobre uma cena para reutilizá-la. As fontes da cena ativa iniciam automaticamente."),("03","Encontre o melhor enquadramento","Selecione e arraste qualquer fonte na prévia. Arraste as oito alças para redimensionar; Alt recorta e Shift permite tamanho livre. Ctrl desativa o encaixe; Ctrl+Z desfaz. Use o cadeado para bloquear. Dê dois cliques para abrir Propriedades."),("04","Leve sua imagem para outro aplicativo","Inicie a câmera virtual e escolha Mini Studio RS Cam no aplicativo de chamada. Ou salve a composição como PNG.")] {
                    ui.horizontal_top(|ui|{badge(ui,n,ACCENT);ui.vertical(|ui|{ui.label(title(heading,15.0));ui.label(muted(body));});});ui.add_space(6.0);
                }
                ui.separator();
                ui.horizontal(|ui|{section(ui,"ATALHOS");ui.label(muted("Ctrl+O  Imagem     Ctrl+S  Cena     Ctrl+Shift+S  PNG     Del  Remover"));});
                ui.add_space(4.0);section(ui,"TAMANHO DA INTERFACE");
                ui.horizontal(|ui| {for scale in [0.9,1.0,1.1,1.25]{if ui.selectable_label((self.scale-scale).abs()<0.01,format!("{:.0}%",scale*100.0)).clicked(){self.scale=scale;ctx.set_zoom_factor(scale);}}});
                ui.label(muted("Cenas e ajustes são salvos automaticamente. Ao reabrir e ao trocar de cena, suas fontes iniciam automaticamente. Se uma origem estiver indisponível, o aplicativo tenta reconectar automaticamente."));
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
        if self.library_edit.is_some()
            && self.picker.is_none()
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.library_edit = None;
        }
        if self.library_edit.is_some()
            || self.picker.is_some()
            || self.scene_dialog.is_some()
            || self.properties.is_some()
            || self.help_open
            || self.setup_open
            || self.projector.dialog_open
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
            if let Some(drag) = self.canvas_drag.take()
                && let Some(source) = self.source_mut(drag.id)
            {
                source.transform = drag.original;
            }
            self.selected = None;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
            self.undo_transform(false);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Y)) {
            self.undo_transform(true);
        }
        if let Some(id) = self.selected {
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::E)) {
                self.properties = Some(id);
            }
            if self.source(id).is_some_and(|s| !s.locked) {
                if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::F)) {
                    self.fit_source(id);
                }
                for (key, delta) in [
                    (Key::ArrowLeft, vec2(-1.0, 0.0)),
                    (Key::ArrowRight, vec2(1.0, 0.0)),
                    (Key::ArrowUp, vec2(0.0, -1.0)),
                    (Key::ArrowDown, vec2(0.0, 1.0)),
                ] {
                    let shift = ctx.input(|i| i.modifiers.shift);
                    if ctx.input_mut(|i| {
                        i.consume_key(
                            if shift {
                                Modifiers::SHIFT
                            } else {
                                Modifiers::NONE
                            },
                            key,
                        )
                    }) {
                        self.remember_transform(id);
                        let source = self.source_mut(id).unwrap();
                        let step = if shift { 10.0 } else { 1.0 };
                        source.transform.x += delta.x * step / WIDTH as f32;
                        source.transform.y += delta.y * step / HEIGHT as f32;
                        source.transform.sanitize();
                    }
                }
            }
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
            self.library_target = Some(0);
            self.accept_library_input(SourceInput::Image(path));
        }
    }
    fn render(&mut self, ctx: &Context) {
        self.ensure_scene_sources(ctx);
        self.receive_frames(ctx);
        self.shortcuts(ctx);
        if self.notice != self.notice_seen {
            self.notice_seen = self.notice.clone();
            self.notice_at = Instant::now();
        }
        self.header(ctx);
        self.footer(ctx);
        self.bottom_dock(ctx);
        if self.library_edit.is_none() {
            self.selection_strip(ctx);
        }
        CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(16))
            .show(ctx, |ui| {
                if let Some(id) = self.library_edit {
                    self.library_editor(ui, ctx, id);
                } else {
                    self.canvas(ui, ctx);
                }
            });
        self.properties_dialog(ctx);
        self.draw_picker(ctx);
        self.dialogs(ctx);
        self.scene_dialogs(ctx);
        self.projector_dialog(ctx);
        self.sync_library();
        for id in std::mem::take(&mut self.library_restarts) {
            if self.current_scene().sources.iter().any(|s| s.id == id) || self.multiview_open {
                self.activate_source(ctx, id);
            }
        }
        self.multiview(ctx);
        self.projector(ctx);
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
    #[test]
    fn restored_image_scene_produces_pixels_without_activation_click() {
        let folder =
            std::env::temp_dir().join(format!("mini-studio-autostart-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let image_path = folder.join("source.png");
        let scene_path = folder.join("scenes.json");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([20_u8, 90, 140, 255]))
            .save(&image_path)
            .unwrap();
        let mut h = Harness::new(vec2(1024.0, 720.0));
        let id = h.image("Imagem salva", [255; 4]);
        h.app.source_mut(id).unwrap().input = SourceInput::Image(image_path.clone());
        h.app.source_mut(id).unwrap().transform.width = 0.4;
        h.app.sync_library();
        h.app.scenes.save(&scene_path).unwrap();
        h.app.scenes = crate::scenes::SceneLibrary::load(&scene_path)
            .unwrap()
            .unwrap();
        h.app.runtimes.clear();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            h.frame(vec![]);
            if h.app.runtimes[&id].borrow().pixels.is_some() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "A fonte salva não iniciou automaticamente"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            &h.app.runtimes[&id].borrow().pixels.as_ref().unwrap().rgba[..4],
            &[20, 90, 140, 255]
        );
        assert_eq!(h.app.source(id).unwrap().transform.width, 0.4);
        assert!(!h.controls.contains_key("Retomar"));
        h.app.stop_all();
        std::fs::remove_file(image_path).unwrap();
        std::fs::remove_file(scene_path).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }

    #[test]
    fn saved_scene_starts_automatically_and_errors_do_not_retry_every_frame() {
        use std::rc::Rc;
        let mut h = Harness::new(vec2(1024.0, 720.0));
        let id = h.image("Fonte salva", [255; 4]);
        h.app.runtimes.clear(); // Loaded scenes have definitions but no capture sessions.
        h.frame(vec![]);
        assert!(h.app.runtimes.contains_key(&id));
        assert!(!h.controls.contains_key("Retomar"));
        let runtime = h.app.runtimes[&id].clone();
        {
            let mut source = runtime.borrow_mut();
            source.stream = crate::source_runtime::Stream::Offline;
            source.error = Some("Origem indisponível".into());
        }
        for _ in 0..3 {
            h.frame(vec![]);
        }
        assert!(Rc::ptr_eq(&runtime, &h.app.runtimes[&id]));
        assert!(!h.controls.contains_key("Reconectar"));
        h.app.stop_all();
        h.frame(vec![]);
        assert!(h.app.runtimes.is_empty());
        h.app.activate_scene(&h.ctx);
        assert!(h.app.runtimes.contains_key(&id));
    }

    #[test]
    fn library_editor_uses_main_area_and_blocks_scene_shortcuts() {
        for size in [vec2(800.0, 600.0), vec2(1440.0, 900.0)] {
            let mut h = Harness::new(size);
            let id = h.image("Câmera com nome longo para configurar", [255; 4]);
            h.click("Configurar fonte: Câmera com nome longo para configurar");
            assert!(h.app.library_edit.is_some());
            let edit = h.controls["Alterar origem"];
            let gear = h.controls["Configurar fonte: Câmera com nome longo para configurar"];
            assert!(edit.bottom() < gear.top());
            assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(edit));
            h.frame(vec![Event::Key {
                key: Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]);
            assert!(h.app.source(id).is_some());
            h.click("Voltar à prévia");
            assert!(h.app.library_edit.is_none());
            h.frame(vec![]);
            assert!(h.controls.contains_key("Prévia da cena"));
        }
    }

    #[test]
    fn dragging_library_source_to_scenes_creates_independent_uses() {
        let mut h = Harness::new(vec2(1440.0, 900.0));
        h.app.library_target = Some(0);
        h.app
            .accept_library_input(SourceInput::Image("Arte.png".into()));
        h.app.scenes.scenes.push(crate::scenes::SavedScene {
            name: "Cena destino".into(),
            sources: vec![],
        });
        h.frame(vec![]);
        h.frame(vec![]);
        let from = h.controls["Arrastar fonte: Arte.png"].center();
        let to = h.controls["Cena destino"].center();
        h.drag(from, to);
        assert!(h.app.scenes.scenes[0].sources.is_empty());
        assert_eq!(h.app.scenes.scenes[1].sources.len(), 1);
        assert_eq!(h.app.scenes.sources.len(), 1);
        h.app.scenes.scenes[1].sources[0].transform.width = 0.4;
        h.frame(vec![]);
        h.drag(
            h.controls["Arrastar fonte: Arte.png"].center(),
            h.controls["Cena 1"].center(),
        );
        assert_eq!(h.app.scenes.scenes[0].sources.len(), 1);
        assert_eq!(
            h.app.scenes.scenes[0].sources[0].source_id,
            h.app.scenes.scenes[1].sources[0].source_id
        );
        assert_eq!(h.app.scenes.scenes[0].sources[0].transform.width, 1.0);
        assert_eq!(h.app.scenes.scenes[1].sources[0].transform.width, 0.4);
    }
    #[test]
    fn library_creation_reuse_and_removal_preserve_definition() {
        let mut h = Harness::new(vec2(1024.0, 720.0));
        h.app.library_target = Some(0);
        assert!(
            h.app
                .accept_library_input(SourceInput::Image("C:/library.png".into()))
        );
        assert!(h.app.current_scene().sources.is_empty());
        assert_eq!(h.app.scenes.sources.len(), 1);
        h.click("Adicionar fonte da biblioteca");
        h.click("library.png");
        assert_eq!(h.app.current_scene().sources.len(), 1);
        let original = h.app.selected.unwrap();
        h.app.duplicate_source(&h.ctx, original);
        let copy = h.app.selected.unwrap();
        h.app.source_mut(copy).unwrap().transform.width = 0.3;
        h.app.source_mut(copy).unwrap().name = "Fonte comum".into();
        h.app.sync_library();
        assert_eq!(h.app.source(original).unwrap().name, "Fonte comum");
        assert_eq!(h.app.source(original).unwrap().transform.width, 1.0);
        assert_eq!(h.app.source(copy).unwrap().transform.width, 0.3);
        h.app.remove_source(original);
        h.app.remove_source(copy);
        assert_eq!(h.app.scenes.sources.len(), 1);
    }

    #[test]
    fn ndi_picker_adds_and_replaces_channels_without_losing_geometry() {
        use crate::{Picker, scenes::NdiSource};
        use std::rc::Rc;

        let mut h = Harness::new(vec2(1024.0, 720.0));
        let channel = NdiSource {
            name: "ESTUDIO (Câmera 1)".into(),
        };
        // Um runtime em memória evita acesso à rede neste teste do seletor.
        let shared_id = h.image("Canal existente", [255; 4]);
        h.app.source_mut(shared_id).unwrap().input = SourceInput::Ndi(channel.clone());
        let shared = h.app.runtimes[&shared_id].clone();
        let picker = || Picker {
            kind: PickerKind::Ndi,
            receiver: None,
            choices: vec![Choice::Ndi(channel.clone())],
            message: "Selecione uma fonte.".into(),
            search: "câmera".into(),
        };
        h.app.picker = Some(picker());
        h.click(&channel.name);
        let added = h.app.selected.unwrap();
        assert_ne!(added, shared_id);
        assert_eq!(
            h.app.source(added).unwrap().input,
            SourceInput::Ndi(channel.clone())
        );
        assert!(Rc::ptr_eq(&h.app.runtimes[&added], &shared));
        assert!(h.app.picker.is_none());

        let target = h.image("Enquadramento preservado", [0, 0, 0, 255]);
        h.app.source_mut(target).unwrap().transform = Transform {
            x: 0.2,
            y: 0.1,
            width: 0.5,
            height: 0.6,
            mirror: true,
            ..Default::default()
        };
        let before = h.app.source(target).unwrap().clone();
        h.app.picker = Some(picker());
        h.app.picker_target = Some(target);
        h.click(&channel.name);
        let after = h.app.source(target).unwrap();
        assert_eq!(after.name, before.name);
        assert_eq!(after.transform, before.transform);
        assert_eq!(after.input, SourceInput::Ndi(channel));
        assert!(Rc::ptr_eq(&h.app.runtimes[&target], &shared));
        assert_eq!(h.app.current_scene().sources.len(), 3);
        assert!(h.app.picker_target.is_none());
    }

    #[test]
    fn ndi_picker_cancel_preserves_saved_source() {
        use crate::{Picker, scenes::NdiSource};

        let mut h = Harness::new(vec2(800.0, 600.0));
        let id = h.image("Canal salvo", [255; 4]);
        h.app.source_mut(id).unwrap().input = SourceInput::Ndi(NdiSource {
            name: "ESTUDIO (Canal salvo)".into(),
        });
        let before = h.app.source(id).unwrap().clone();
        h.app.picker_target = Some(id);
        h.app.picker = Some(Picker {
            kind: PickerKind::Ndi,
            receiver: None,
            choices: vec![],
            message: "Nenhum canal NDI encontrado.".into(),
            search: String::new(),
        });
        h.click("Cancelar");
        assert_eq!(h.app.source(id).unwrap(), &before);
        assert!(h.app.picker.is_none());
        assert!(h.app.picker_target.is_none());
    }

    #[test]
    fn desktop_properties_toggle_cursor_without_changing_shared_source_or_geometry() {
        use std::rc::Rc;

        for window in [false, true] {
            let mut h = Harness::new(vec2(800.0, 600.0));
            let id = h.image("Desktop", [255; 4]);
            h.app.source_mut(id).unwrap().input =
                SourceInput::Desktop(crate::scenes::DesktopSource {
                    selected_id: Some(u32::MAX),
                    window,
                    capture_cursor: true,
                    name: "Fonte de teste indisponível".into(),
                    detail: "Teste".into(),
                });
            let transform = h.app.source(id).unwrap().transform;
            let mut duplicate = h.app.source(id).unwrap().clone();
            duplicate.id = h.app.allocate_source_id();
            duplicate.source_id = 0;
            let other_id = duplicate.id;
            h.app.current_scene_mut().sources.push(duplicate);
            let original_runtime = h.app.runtimes[&id].clone();
            h.app.runtimes.insert(other_id, original_runtime.clone());
            h.app.properties = Some(id);
            h.frame(vec![]);
            h.frame(vec![]);
            assert!(
                Rect::from_min_size(Pos2::ZERO, h.size)
                    .contains_rect(h.controls["Capturar ponteiro do mouse"])
            );
            h.click("Capturar ponteiro do mouse");
            for (source_id, expected) in [(id, false), (other_id, true)] {
                let SourceInput::Desktop(desktop) = &h.app.source(source_id).unwrap().input else {
                    panic!("Esperava fonte de desktop");
                };
                assert_eq!(desktop.capture_cursor, expected);
                assert_eq!(h.app.source(source_id).unwrap().transform, transform);
            }
            assert!(!Rc::ptr_eq(&h.app.runtimes[&id], &original_runtime));
            assert!(Rc::ptr_eq(&h.app.runtimes[&other_id], &original_runtime));
            h.click("Capturar ponteiro do mouse");
            assert_eq!(
                h.app.source(id).unwrap().input,
                h.app.source(other_id).unwrap().input
            );
            assert!(Rc::ptr_eq(&h.app.runtimes[&id], &original_runtime));
            h.click("Concluir");
        }
    }

    #[test]
    fn compact_properties_preserve_geometry_and_fit_small_window() {
        let mut h = Harness::new(vec2(800.0, 600.0));
        let id = h.image("Enquadramento", [255; 4]);
        let transform = Transform {
            x: -0.2,
            y: 0.3,
            width: 1.4,
            height: 0.6,
            stretch: true,
            crop: [0.1, 0.2, 0.05, 0.15],
            ..Default::default()
        };
        h.app.source_mut(id).unwrap().transform = transform;
        h.app.properties = Some(id);
        h.frame(vec![]);
        h.frame(vec![]);
        for label in [
            "Fechar propriedades",
            "Escolher outra fonte",
            "Espelhar horizontalmente",
            "Concluir",
        ] {
            assert!(
                Rect::from_min_size(Pos2::ZERO, h.size).contains_rect(h.controls[label]),
                "{label} fora da janela"
            );
        }
        h.click("Espelhar horizontalmente");
        h.click("Concluir");
        assert!(h.app.properties.is_none());
        assert_eq!(
            h.app.source(id).unwrap().transform,
            Transform {
                mirror: !transform.mirror,
                ..transform
            }
        );
    }

    #[test]
    fn empty_preview_offers_direct_sources_at_small_and_large_sizes() {
        for size in [vec2(800.0, 600.0), vec2(1440.0, 900.0)] {
            let mut h = Harness::new(size);
            h.frame(vec![]);
            h.frame(vec![]);
            h.click("Nova fonte");
            for label in ["Tela", "Janela", "Câmera", "NDI", "Imagem"] {
                let bounds = h
                    .controls
                    .get(label)
                    .unwrap_or_else(|| panic!("Ausente: {label}"));
                assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(*bounds));
            }
            assert!(h.app.runtimes.is_empty());
            h.click("Tela");
            assert!(matches!(
                h.app.picker.as_ref().map(|picker| picker.kind),
                Some(PickerKind::Screen)
            ));
            assert!(h.app.current_scene().sources.is_empty());
            h.freeze_picker();
            h.click("Cancelar");
            assert!(h.app.picker.is_none());
        }
    }

    #[test]
    fn saved_source_can_be_replaced_from_list_without_opening_properties() {
        use crate::scenes::{CameraSource, SavedSource};
        let mut h = Harness::new(vec2(1024.0, 720.0));
        h.app.current_scene_mut().sources.push(SavedSource {
            source_id: 0,
            id: 91,
            name: "Minha câmera".into(),
            input: SourceInput::Camera(CameraSource {
                id: "unavailable-test-camera".into(),
                name: "Minha câmera".into(),
            }),
            visible: true,
            locked: false,
            transform: Transform {
                x: 0.7,
                width: 0.25,
                ..Default::default()
            },
        });
        h.app.sync_library();
        let before = h.app.current_scene().sources[0].clone();
        h.app.runtimes.insert(
            91,
            std::rc::Rc::new(std::cell::RefCell::new(
                crate::source_runtime::RuntimeSource {
                    retry: Default::default(),
                    pixels: None,
                    texture: None,
                    stream: crate::source_runtime::Stream::Offline,
                    error: Some("Câmera indisponível".into()),
                },
            )),
        );
        h.frame(vec![]);
        assert!(!h.controls.contains_key("Reconectar"));
        assert!(!h.controls.contains_key("Retomar"));
        assert!(!h.controls.contains_key("Usar modelo"));
        h.click("Escolher outra");
        assert_eq!(h.app.picker_target, Some(91));
        assert!(h.app.properties.is_none());
        h.freeze_picker();
        h.click("Cancelar");
        assert_eq!(h.app.current_scene().sources[0], before);
        assert!(h.app.runtimes.contains_key(&91));
    }

    #[test]
    fn camera_runtime_is_kept_and_shared_across_scenes_until_stop() {
        use crate::scenes::{CameraSource, SavedScene, SavedSource};
        use crate::source_runtime::{RuntimeSource, Stream};
        use std::{cell::RefCell, rc::Rc};
        let mut h = Harness::new(vec2(1000.0, 800.0));
        let input = SourceInput::Camera(CameraSource {
            id: "camera-test".into(),
            name: "Camera".into(),
        });
        let source = SavedSource {
            source_id: 0,
            id: 100,
            name: "Camera".into(),
            input,
            visible: true,
            locked: false,
            transform: Transform::default(),
        };
        let mut other = source.clone();
        other.id = 101;
        h.app.scenes.scenes = vec![
            SavedScene {
                name: "A".into(),
                sources: vec![source],
            },
            SavedScene {
                name: "B".into(),
                sources: vec![other],
            },
        ];
        h.app.scenes.active = 0;
        let runtime = Rc::new(RefCell::new(RuntimeSource {
            retry: Default::default(),
            pixels: None,
            texture: None,
            stream: Stream::Offline,
            error: None,
        }));
        h.app.runtimes.insert(100, runtime.clone());
        h.app.switch_scene(&h.ctx, 1);
        assert!(Rc::ptr_eq(&h.app.runtimes[&100], &runtime));
        assert!(Rc::ptr_eq(&h.app.runtimes[&101], &runtime));
        h.app.switch_scene(&h.ctx, 0);
        assert!(Rc::ptr_eq(&h.app.runtimes[&100], &runtime));
        h.app.stop_all();
        assert!(h.app.runtimes.is_empty());
    }

    use super::*;
    use crate::{Options, composition::Pixels, image_source};
    use std::collections::HashMap;
    struct Harness {
        ctx: Context,
        app: Studio,
        size: Vec2,
        controls: HashMap<String, Rect>,
        time: f64,
        modifiers: Modifiers,
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
                modifiers: Modifiers::NONE,
            }
        }
        fn frame(&mut self, events: Vec<Event>) {
            self.time += 1.0 / 60.0;
            let output = self.ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                    time: Some(self.time),
                    modifiers: self.modifiers,
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
        fn freeze_picker(&mut self) {
            // Enumeração real pode alterar a altura do modal entre pressionar e soltar.
            let picker = self.app.picker.as_mut().unwrap();
            picker.receiver = None;
            picker.choices.clear();
            picker.message = "Nenhum dispositivo no cenário de teste.".into();
        }
        fn click(&mut self, label: &str) {
            self.time += 0.5;
            self.frame(vec![]);
            self.frame(vec![]);
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
    fn multiview_switches_with_one_click_and_releases_extra_sources() {
        use std::rc::Rc;
        let mut h = Harness::new(vec2(1440.0, 900.0));
        let first = h.image("Primeira", [255, 0, 0, 255]);
        let first_runtime = h.app.runtimes[&first].clone();
        h.app.scenes.scenes.push(crate::scenes::SavedScene {
            name: "Segunda".into(),
            sources: vec![],
        });
        h.click("Visualização múltipla");
        assert!(h.app.multiview_open);
        h.click("Alternar para Segunda");
        assert_eq!(h.app.scenes.active, 1);
        assert!(Rc::ptr_eq(&h.app.runtimes[&first], &first_runtime));
        let second = h.image("Segunda", [0, 255, 0, 255]);
        h.click("Alternar para Cena 1");
        assert_eq!(h.app.scenes.active, 0);
        assert!(h.app.runtimes.contains_key(&second));
        h.app.multiview_open = false;
        h.frame(vec![]);
        assert!(Rc::ptr_eq(&h.app.runtimes[&first], &first_runtime));
        assert!(!h.app.runtimes.contains_key(&second));
        h.app.stop_all();
    }

    #[test]
    fn projector_destination_blocks_editing_and_opens_without_virtual_camera() {
        let mut h = Harness::new(vec2(1440.0, 900.0));
        let id = h.image("Projetor", [255, 0, 0, 255]);
        h.click("Projetor de saída");
        assert!(h.app.projector.dialog_open);
        h.frame(vec![Event::Key {
            key: Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        assert!(h.app.source(id).is_some());
        h.click("Abrir em janela");
        assert!(h.app.projector.open);
        assert!(!h.app.projector.dialog_open);
        assert!(h.app.virtual_camera.is_none());
        assert_eq!(h.app.scenes.active, 0);
        h.click("Fechar projetor");
        assert!(!h.app.projector.open);
        assert!(h.app.runtimes.contains_key(&id));
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
            let library = h.controls["Nova fonte"];
            assert!(library.top() > preview.bottom());
            assert!(library.left() < scenes.left());
            assert!((preview.center().x - size.x / 2.0).abs() < 2.0);
            assert!(preview.width() > size.x * 0.3);
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
        assert_eq!(h.app.source(green).unwrap().name, "Outro logo");
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
        if let Some(crate::scene_ui::SceneDialog::Create { name, .. }) = &mut h.app.scene_dialog {
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
    fn mouse_all_handles_crop_stretch_and_keyboard_undo() {
        let mut h = Harness::new(vec2(1440.0, 900.0));
        let id = h.image("Enquadramento", [255; 4]);
        let initial = Transform {
            x: 0.25,
            y: 0.25,
            width: 0.5,
            height: 0.5,
            stretch: true,
            ..Default::default()
        };
        h.app.source_mut(id).unwrap().transform = initial;
        h.frame(vec![]);
        let preview = h.controls["Prévia da cena"];
        let area = Rect::from_min_size(preview.min + preview.size() * 0.25, preview.size() * 0.5);
        for (handle, point) in canvas_editor::handles(area) {
            h.app.source_mut(id).unwrap().transform = initial;
            h.app.selected = Some(id);
            h.frame(vec![]);
            let Handle::Resize(x, y) = handle else {
                unreachable!()
            };
            h.drag(
                point,
                point + vec2(x as f32 * 0.05, y as f32 * 0.05) * preview.size(),
            );
            let after = h.app.source(id).unwrap().transform;
            assert!((after.width - 0.55).abs() < 0.002, "{handle:?}: {after:?}");
            assert!((after.height - 0.55).abs() < 0.002);
        }
        h.app.source_mut(id).unwrap().transform = initial;
        h.modifiers = Modifiers::ALT;
        h.frame(vec![]);
        h.drag(
            area.left_center(),
            area.left_center() + vec2(preview.width() * 0.1, 0.0),
        );
        let crop = h.app.source(id).unwrap().transform;
        assert!((crop.crop[0] - 0.2).abs() < 0.002);
        assert!((crop.width - 0.4).abs() < 0.002);
        h.modifiers = Modifiers::CTRL;
        let key = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL,
        };
        h.frame(vec![key(Key::Z)]);
        assert_eq!(h.app.source(id).unwrap().transform, initial);
        h.frame(vec![key(Key::Y)]);
        assert_eq!(h.app.source(id).unwrap().transform, crop);
        h.app.source_mut(id).unwrap().transform = initial;
        h.modifiers = Modifiers::SHIFT;
        h.frame(vec![]);
        h.drag(
            area.right_center(),
            area.right_center() + vec2(preview.width() * 0.1, 0.0),
        );
        let stretched = h.app.source(id).unwrap().transform;
        assert!((stretched.width - 0.6).abs() < 0.002);
        assert!((stretched.height - 0.5).abs() < 0.002);
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
        assert!(h.app.runtimes.values().all(|r| r.borrow().error.is_some()));
        assert!(!h.app.notice_error);
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
}
