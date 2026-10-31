use crate::{
    Choice, PickerKind, Studio,
    composition::{CameraPosition, HEIGHT, ImageMode, Source, WIDTH},
    preferences::Preferences,
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
        self.desktop.is_some() || self.camera.is_some() || self.image.is_some()
    }
    fn source_count(&self) -> usize {
        usize::from(self.desktop.is_some())
            + usize::from(self.camera.is_some())
            + usize::from(self.image.is_some())
    }
    fn visible(&self, source: Source) -> bool {
        match source {
            Source::Desktop => self.desktop_visible,
            Source::Camera => self.camera_visible,
            Source::Image => self.image_settings.visible,
        }
    }
    fn toggle_source(&mut self, source: Source) {
        match source {
            Source::Desktop => self.desktop_visible = !self.desktop_visible,
            Source::Camera => self.camera_visible = !self.camera_visible,
            Source::Image => self.image_settings.visible = !self.image_settings.visible,
        }
    }
    fn remove_source(&mut self, source: Source) {
        match source {
            Source::Desktop => {
                self.desktop = None;
                self.pending_desktop = None;
            }
            Source::Camera => {
                self.camera = None;
                self.pending_camera = None;
            }
            Source::Image => {
                self.image = None;
                self.image_receiver = None;
            }
        }
        self.selected = None;
        self.inspector_tab = false;
        self.notice_error = false;
        self.notice_at = Instant::now();
        self.notice = "Fonte removida da composição.".into();
    }
    fn current_preferences(&self) -> Preferences {
        Preferences {
            mirror: self.mirror,
            camera_size: self.camera_size,
            camera_position: self.camera_position,
            image: self.image_settings,
            guides: self.guides,
            scale: self.scale,
        }
    }
    fn persist_preferences(&mut self) {
        if self.options.screenshot.is_some() || self.options.benchmark.is_some() {
            return;
        }
        let current = self.current_preferences();
        if current != self.saved_preferences {
            match current.save() {
                Ok(()) => self.saved_preferences = current,
                Err(error) => {
                    self.show_error(format!("Não foi possível guardar os ajustes: {error}"))
                }
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
            Err(error) => self.virtual_notice = error,
        }
    }

    fn header(&mut self, ctx: &Context) {
        TopBottomPanel::top("app_header")
            .exact_height(84.0)
            .frame(
                Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(24, 16)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::hover());
                    theme::logo(ui.painter(), rect);
                    ui.add_space(3.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(title("Mini Studio", 22.0));
                        ui.label(muted("Seu espaço para criar e apresentar."));
                    });
                    ui.add_space(8.0);
                    badge(ui, env!("CARGO_PKG_VERSION"), MUTED);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, Icon::Help, "Ajuda", ButtonKind::Quiet, 86.0).clicked() {
                            self.help_open = true;
                        }
                        ui.separator();
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
        TopBottomPanel::bottom("app_footer").exact_height(40.0).frame(Frame::new().fill(PANEL).inner_margin(Margin::symmetric(24,9))).show(ctx,|ui| {
            ui.horizontal(|ui| {
                let (r,_)=ui.allocate_exact_size(vec2(8.0,12.0),Sense::hover());
                ui.painter().circle_filled(r.center(),3.0,if self.has_sources(){ACCENT}else{MUTED});
                ui.label(muted(if self.has_sources() {format!("{} fonte{} na composição",self.source_count(),if self.source_count()==1 {""}else{"s"})} else {"Pronto para criar".into()}));
                ui.with_layout(Layout::right_to_left(Align::Center),|ui| {
                    ui.label(muted("LS BRASIL"));ui.separator();
                    ui.label(muted(if self.desktop.is_some() || self.camera.is_some() {format!("{:.0} FPS na prévia",self.fps)} else {"Imagem estática".into()}))
                        .on_hover_text("Quadros novos recebidos. Imagens estáticas não precisam gerar novos quadros. A câmera virtual tem seu próprio relógio de saída.");
                    ui.separator();ui.label(muted("1280 × 720")).on_hover_text(format!("Renderização acelerada · {}",self.adapter));
                });
            });
        });
    }

    fn sources_panel(&mut self, ctx: &Context, compact: bool) {
        SidePanel::left("sources_panel").exact_width(264.0).resizable(false).frame(panel()).show(ctx,|ui| {
            if compact {
                TopBottomPanel::bottom("compact_output").frame(Frame::new().inner_margin(Margin {top:16,..Margin::ZERO})).show_inside(ui,|ui|self.output_card(ui,ctx));
                ui.columns(2,|columns| {
                    if columns[0].add_sized([columns[0].available_width(),34.0],Button::selectable(!self.inspector_tab,"Fontes")).clicked(){self.inspector_tab=false;}
                    if columns[1].add_sized([columns[1].available_width(),34.0],Button::selectable(self.inspector_tab,"Ajustes")).clicked(){self.inspector_tab=true;}
                });ui.add_space(8.0);
            }
            ScrollArea::vertical().scroll_bar_visibility(scroll_area::ScrollBarVisibility::VisibleWhenNeeded).id_salt(if compact && self.inspector_tab {"compact_inspector"}else{"source_scroll"}).show(ui,|ui| {
                if compact && self.inspector_tab {self.inspector(ui,ctx);return;}

                ui.horizontal(|ui| {ui.label(title("Fontes",19.0));ui.with_layout(Layout::right_to_left(Align::Center),|ui|{badge(ui,&format!("{} / 3",self.source_count()),MUTED);});});
                ui.label(muted("Monte sua composição em camadas."));
                ui.add_space(10.0);
                section(ui,"ADICIONAR FONTE");
                ui.add_space(2.0);
                for (icon,label,description,kind) in [
                    (Icon::Monitor,"Tela","Compartilhe um monitor",Some(PickerKind::Screen)),
                    (Icon::Window,"Janela","Mostre um aplicativo",Some(PickerKind::Window)),
                    (Icon::Camera,"Câmera","Traga você para a cena",Some(PickerKind::Camera)),
                    (Icon::Image,"Imagem","Fotos, artes e logotipos",None),
                ] {
                    let enabled=kind.is_some() || self.image_receiver.is_none();
                    let response=ui.add_enabled_ui(enabled,|ui|add_source_tile(ui,icon,label,description)).inner;
                    if response.clicked() {if let Some(kind)=kind {self.open_picker(ctx,kind);} else {self.open_image(ctx);}}
                }
                if self.image_receiver.is_some() {ui.horizontal(|ui|{ui.spinner();ui.label(muted("Preparando imagem…"));});}
                ui.add_space(14.0);
                section(ui,"NA COMPOSIÇÃO");
                ui.label(muted("As fontes de cima aparecem na frente."));
                ui.add_space(4.0);
                let mut sources=Vec::new();
                if let Some(camera)=&self.camera {sources.push((Source::Camera,Icon::Camera,camera.device.name.clone(),"Câmera"));}
                if self.image_settings.mode==ImageMode::Overlay && let Some(image)=&self.image {sources.push((Source::Image,Icon::Image,image.loaded.name.clone(),"Sobreposição"));}
                if let Some(desktop)=&self.desktop {sources.push((Source::Desktop,if desktop.kind==crate::SourceKind::Screen {Icon::Monitor}else{Icon::Window},desktop.source.name.clone(),"Captura"));}
                if self.image_settings.mode==ImageMode::Background && let Some(image)=&self.image {sources.push((Source::Image,Icon::Image,image.loaded.name.clone(),"Fundo"));}
                for (source,icon,name,kind) in sources {
                    let selected=self.selected==Some(source);
                    let visible=self.visible(source);
                    let (clicked,toggle)=source_tile(ui,icon,&name,kind,selected,visible);
                    if clicked {self.selected=Some(source);self.inspector_tab=true;}
                    if toggle {self.toggle_source(source);}
                }
                if !self.has_sources() {
                    card().fill(BG).show(ui,|ui|{ui.set_min_width(ui.available_width());ui.add_space(4.0);ui.label(RichText::new("Tudo começa com uma fonte.").size(13.0).color(TEXT));ui.label(muted("Escolha uma opção acima ou arraste uma imagem para o estúdio."));ui.add_space(4.0);});
                }
                ui.add_space(18.0);
                if (self.has_sources() || self.pending_desktop.is_some() || self.pending_camera.is_some() || self.virtual_camera.is_some())
                    && button(ui,Icon::Stop,"Limpar composição",ButtonKind::Quiet,ui.available_width()).on_hover_text("Remove todas as fontes, libera os dispositivos e para a câmera virtual.").clicked() {self.stop_all();}
            });
        });
    }

    fn inspector(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.spacing_mut().item_spacing.y = 7.0;
        ui.spacing_mut().interact_size.y = 26.0;
        ui.label(title("Ajustes", 19.0));
        ui.add_space(8.0);
        let Some(source) = self.selected.filter(|s| match s {
            Source::Desktop => self.desktop.is_some(),
            Source::Camera => self.camera.is_some(),
            Source::Image => self.image.is_some(),
        }) else {
            card().fill(BG).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                paint_icon(ui.painter(), Icon::Settings, r, MUTED);
                ui.add_space(4.0);
                ui.label(title("Cada detalhe, no seu lugar.", 15.0));
                ui.label(muted(
                    "Selecione uma fonte para ajustar sua aparência na composição.",
                ));
            });
            return;
        };
        let (name, subtitle) = match source {
            Source::Desktop => (
                self.desktop.as_ref().unwrap().source.name.clone(),
                "Tela ou janela",
            ),
            Source::Camera => (self.camera.as_ref().unwrap().device.name.clone(), "Câmera"),
            Source::Image => (self.image.as_ref().unwrap().loaded.name.clone(), "Imagem"),
        };
        badge(ui, subtitle, ACCENT);
        ui.add(Label::new(title(&name, 15.0)).wrap());
        let mut visible = self.visible(source);
        if ui.checkbox(&mut visible, "Mostrar na composição").changed() {
            self.toggle_source(source);
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        match source {
            Source::Image => {
                section(ui, "CAMADA");
                ui.columns(2, |columns| {
                    if columns[0]
                        .add_sized(
                            [columns[0].available_width(), 34.0],
                            Button::selectable(
                                self.image_settings.mode == ImageMode::Background,
                                "Fundo",
                            ),
                        )
                        .clicked()
                    {
                        self.image_settings.mode = ImageMode::Background;
                    }
                    if columns[1]
                        .add_sized(
                            [columns[1].available_width(), 34.0],
                            Button::selectable(
                                self.image_settings.mode == ImageMode::Overlay,
                                "Sobrepor",
                            ),
                        )
                        .clicked()
                    {
                        self.image_settings.mode = ImageMode::Overlay;
                    }
                });
                ui.label(muted(
                    if self.image_settings.mode == ImageMode::Background {
                        "Atrás da captura de tela ou janela."
                    } else {
                        "Na frente da captura, abaixo da câmera."
                    },
                ));
                ui.add_space(10.0);
                section(ui, "TAMANHO E POSIÇÃO");
                full_slider(ui, "Tamanho", &mut self.image_settings.size, 5..=100);
                percent_slider(ui, "Horizontal", &mut self.image_settings.x);
                percent_slider(ui, "Vertical", &mut self.image_settings.y);
                ui.label(muted("Você também pode arrastar a imagem na prévia."));
                if button(
                    ui,
                    Icon::Refresh,
                    "Redefinir ajustes",
                    ButtonKind::Quiet,
                    ui.available_width(),
                )
                .clicked()
                {
                    self.image_settings = crate::ImageSettings::default();
                }
                if ui
                    .add_enabled_ui(self.image_receiver.is_none(), |ui| {
                        button(
                            ui,
                            Icon::Folder,
                            "Substituir imagem",
                            ButtonKind::Secondary,
                            ui.available_width(),
                        )
                    })
                    .inner
                    .clicked()
                {
                    self.open_image(ctx);
                }
            }
            Source::Camera => {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                    paint_icon(ui.painter(), Icon::Mirror, r, MUTED);
                    ui.checkbox(&mut self.mirror, "Espelhar câmera");
                });
                let pip = self.scene().desktop.is_some()
                    || self
                        .scene()
                        .image
                        .is_some_and(|_| self.image_settings.visible);
                ui.add_space(10.0);
                section(ui, "POSIÇÃO NA COMPOSIÇÃO");
                ui.add_enabled_ui(pip, |ui| {
                    for row in [
                        [
                            (CameraPosition::TopLeft, "Superior esq."),
                            (CameraPosition::TopRight, "Superior dir."),
                        ],
                        [
                            (CameraPosition::BottomLeft, "Inferior esq."),
                            (CameraPosition::BottomRight, "Inferior dir."),
                        ],
                    ] {
                        ui.columns(2, |columns| {
                            for (i, (position, label)) in row.into_iter().enumerate() {
                                let width = columns[i].available_width();
                                if columns[i]
                                    .add_sized(
                                        [width, 34.0],
                                        Button::selectable(self.camera_position == position, label),
                                    )
                                    .clicked()
                                {
                                    self.camera_position = position;
                                }
                            }
                        });
                    }
                    full_slider(ui, "Tamanho", &mut self.camera_size, 15..=40);
                });
                ui.label(muted(if pip {
                    "Arraste a câmera na prévia para trocar de canto."
                } else {
                    "Sozinha, a câmera ocupa toda a composição."
                }));
                if button(
                    ui,
                    Icon::Refresh,
                    "Redefinir ajustes",
                    ButtonKind::Quiet,
                    ui.available_width(),
                )
                .clicked()
                {
                    self.mirror = false;
                    self.camera_size = 25;
                    self.camera_position = CameraPosition::BottomRight;
                }
                if button(
                    ui,
                    Icon::Camera,
                    "Trocar câmera",
                    ButtonKind::Secondary,
                    ui.available_width(),
                )
                .clicked()
                {
                    self.open_picker(ctx, PickerKind::Camera);
                }
            }
            Source::Desktop => {
                let desktop = self.desktop.as_ref().unwrap();
                ui.label(title("Enquadramento automático", 14.0));
                ui.label(muted(
                    "A captura se ajusta ao espaço disponível, preservando a proporção original.",
                ));
                ui.add_space(8.0);
                badge(
                    ui,
                    &format!("{} × {}", desktop.frame.width, desktop.frame.height),
                    MUTED,
                );
                ui.add_space(8.0);
                if button(
                    ui,
                    Icon::Monitor,
                    "Trocar por tela",
                    ButtonKind::Secondary,
                    ui.available_width(),
                )
                .clicked()
                {
                    self.open_picker(ctx, PickerKind::Screen);
                }
                if button(
                    ui,
                    Icon::Window,
                    "Trocar por janela",
                    ButtonKind::Secondary,
                    ui.available_width(),
                )
                .clicked()
                {
                    self.open_picker(ctx, PickerKind::Window);
                }
                ui.label(muted(
                    "Para evitar o efeito de espelho, capture outra tela ou uma janela específica.",
                ));
            }
        }
        ui.add_space(8.0);
        if button(
            ui,
            Icon::Trash,
            "Remover fonte",
            ButtonKind::Danger,
            ui.available_width(),
        )
        .clicked()
        {
            self.remove_source(source);
        }
    }

    fn output_card(&mut self, ui: &mut Ui, ctx: &Context) {
        section(ui, "SAÍDA");
        ui.add_space(4.0);
        card()
            .fill(Color32::from_rgb(23, 39, 39))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(43, 70, 64)))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                    paint_icon(ui.painter(), Icon::Camera, r, ACCENT);
                    ui.label(title("Câmera virtual", 16.0));
                });
                ui.label(muted("720p · 30 FPS · Vídeo sem áudio")).on_hover_text("Use sua composição em aplicativos de chamada e apresentação.");
                if let Some(output) = &self.virtual_camera {
                    let stopping = output.stopping();
                    if output.active() {
                        badge(ui, "PUBLICANDO", ACCENT);
                    } else {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(muted(if stopping {
                                "Encerrando…"
                            } else {
                                "Preparando saída…"
                            }));
                        });
                    }
                    ui.add(Label::new(muted(&self.virtual_notice)).truncate()).on_hover_text(&self.virtual_notice);
                    if ui
                        .add_enabled_ui(!stopping, |ui| {
                            button(
                                ui,
                                Icon::Stop,
                                "Parar câmera virtual",
                                ButtonKind::Secondary,
                                ui.available_width(),
                            )
                        })
                        .inner
                        .clicked()
                    {
                        output.stop();
                    }
                } else {
                    let installed = virtual_camera::installed();
                    if !installed {ui.label(muted("Configure uma vez para começar."));}
                    if button(
                        ui,
                        if installed {
                            Icon::Play
                        } else {
                            Icon::Settings
                        },
                        if installed {
                            "Iniciar câmera virtual"
                        } else {
                            "Configurar câmera virtual"
                        },
                        ButtonKind::Primary,
                        ui.available_width(),
                    )
                    .clicked()
                    {
                        self.start_output(ctx);
                    }
                    if self.virtual_notice != "Câmera virtual parada." {
                        ui.add(Label::new(RichText::new(&self.virtual_notice).size(12.0).color(AMBER)).truncate()).on_hover_text(&self.virtual_notice);
                    }
                }
                ui.add_space(3.0);

                ui.label(
                    RichText::new(virtual_camera::CAMERA_NAME)
                        .size(12.0)
                        .color(TEXT),
                ).on_hover_text("Escolha esta câmera no aplicativo de chamada. O Windows pode acrescentar ‘Câmera Virtual do Windows’ ao nome.");
            });
    }

    fn canvas(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(title("Sua composição", 22.0));
                ui.label(muted("A mesma imagem na exportação e na câmera virtual."));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(
                    ui,
                    Icon::Grid,
                    if self.guides {
                        "Ocultar guias"
                    } else {
                        "Mostrar guias de enquadramento"
                    },
                )
                .clicked()
                {
                    self.guides = !self.guides;
                }
                badge(ui, "16:9", MUTED);
            });
        });
        ui.add_space(16.0);
        let available = ui.available_size();
        let frame_height = (available.y - 144.0).max(240.0);
        let outer = ui
            .allocate_exact_size(vec2(available.x, frame_height), Sense::hover())
            .0;
        ui.painter()
            .rect_filled(outer, 14, Color32::from_rgb(9, 13, 18));
        ui.painter()
            .rect_stroke(outer, 14, Stroke::new(1.0_f32, LINE), StrokeKind::Inside);
        let fit = crate::composition::fit_rect(
            WIDTH,
            HEIGHT,
            [
                outer.left() + 16.0,
                outer.top() + 16.0,
                outer.width() - 32.0,
                outer.height() - 32.0,
            ],
        );
        let rect = Rect::from_min_size(pos2(fit[0], fit[1]), vec2(fit[2], fit[3]));
        let response = ui.interact(rect, Id::new("composition_canvas"), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let scene = self.scene();
        let layers = scene.layers();
        let convert = |area: [f32; 4]| {
            Rect::from_min_size(
                pos2(
                    rect.left() + area[0] * rect.width() / WIDTH as f32,
                    rect.top() + area[1] * rect.height() / HEIGHT as f32,
                ),
                vec2(
                    area[2] * rect.width() / WIDTH as f32,
                    area[3] * rect.height() / HEIGHT as f32,
                ),
            )
        };
        for layer in &layers {
            let area = convert(layer.rect);
            let texture = match layer.source {
                Some(Source::Desktop) => self.desktop.as_ref().map(|s| &s.texture),
                Some(Source::Camera) => self.camera.as_ref().map(|s| &s.texture),
                Some(Source::Image) => self.image.as_ref().map(|s| &s.texture),
                None => None,
            };
            if let Some(texture) = texture {
                let uv = if layer.mirror {
                    Rect::from_min_max(pos2(1.0, 0.0), pos2(0.0, 1.0))
                } else {
                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0))
                };
                painter.image(texture.id(), area, uv, Color32::WHITE);
            } else {
                painter.rect_filled(
                    area,
                    0,
                    Color32::from_rgba_unmultiplied(
                        layer.color[0],
                        layer.color[1],
                        layer.color[2],
                        layer.color[3],
                    ),
                );
            }
        }
        let visible = layers.iter().any(|l| l.source.is_some());
        if !visible {
            for y in (0..rect.height() as usize).step_by(24) {
                for x in (0..rect.width() as usize).step_by(24) {
                    painter.circle_filled(
                        rect.min + vec2(x as f32, y as f32),
                        0.7,
                        Color32::from_rgb(31, 42, 49),
                    );
                }
            }
            let center = rect.center();
            let brand = Rect::from_center_size(center - vec2(0.0, 62.0), vec2(58.0, 58.0));
            theme::logo(&painter, brand);
            painter.text(
                center + vec2(0.0, -5.0),
                Align2::CENTER_CENTER,
                if self.has_sources() {
                    "Suas fontes estão ocultas"
                } else {
                    "Seu próximo conteúdo começa aqui."
                },
                FontId::new(
                    if rect.width() < 550.0 { 20.0 } else { 25.0 },
                    FontFamily::Name("semibold".into()),
                ),
                TEXT,
            );
            painter.text(
                center + vec2(0.0, 31.0),
                Align2::CENTER_CENTER,
                if self.has_sources() {
                    "Ative o ícone de visibilidade na lista de fontes."
                } else {
                    "Adicione uma tela, câmera ou imagem ao estúdio."
                },
                FontId::proportional(if rect.width() < 550.0 { 12.0 } else { 14.0 }),
                MUTED,
            );
        }
        if self.guides {
            let stroke = Stroke::new(1.0_f32, Color32::from_white_alpha(70));
            for ratio in [1.0 / 3.0, 2.0 / 3.0] {
                painter.line_segment(
                    [
                        pos2(rect.left() + rect.width() * ratio, rect.top()),
                        pos2(rect.left() + rect.width() * ratio, rect.bottom()),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        pos2(rect.left(), rect.top() + rect.height() * ratio),
                        pos2(rect.right(), rect.top() + rect.height() * ratio),
                    ],
                    stroke,
                );
            }
        }
        if response.clicked() || response.drag_started() {
            let pointer = ctx
                .input(|i| i.pointer.press_origin())
                .or(response.interact_pointer_pos());
            if let Some(pointer) = pointer {
                self.selected = layers
                    .iter()
                    .rev()
                    .find(|l| l.source.is_some() && convert(l.rect).contains(pointer))
                    .and_then(|l| l.source);
            }
            if self.selected.is_some() {
                self.inspector_tab = true;
            }
            if response.drag_started() {
                self.canvas_drag = self
                    .selected
                    .map(|s| (s, self.image_settings.x, self.image_settings.y));
            }
        }
        if response.dragged()
            && let Some((source, x, y)) = self.canvas_drag
        {
            if source == Source::Image
                && let Some(layer) = layers.iter().find(|l| l.source == Some(source))
            {
                let delta = response.total_drag_delta().unwrap_or_default();
                let area = convert(layer.rect);
                self.image_settings.x = drag_position(x, delta.x, rect.width() - area.width());
                self.image_settings.y = drag_position(y, delta.y, rect.height() - area.height());
                ctx.set_cursor_icon(CursorIcon::Grabbing);
            }
            if source == Source::Camera
                && let Some(pointer) = response.interact_pointer_pos()
            {
                self.camera_position =
                    match (pointer.x < rect.center().x, pointer.y < rect.center().y) {
                        (true, true) => CameraPosition::TopLeft,
                        (false, true) => CameraPosition::TopRight,
                        (true, false) => CameraPosition::BottomLeft,
                        (false, false) => CameraPosition::BottomRight,
                    };
            }
        }
        if response.drag_stopped() {
            self.canvas_drag = None;
        }
        if let Some(selected) = self.selected
            && let Some(layer) = layers.iter().find(|l| l.source == Some(selected))
        {
            let area = convert(layer.rect).shrink(1.0);
            painter.rect_stroke(area, 0, Stroke::new(1.5_f32, ACCENT), StrokeKind::Inside);
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            painter.rect_filled(rect, 0, Color32::from_black_alpha(200));
            painter.rect_stroke(
                rect.shrink(6.0),
                10,
                Stroke::new(2.0_f32, ACCENT),
                StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Solte sua imagem aqui",
                FontId::proportional(24.0),
                ACCENT,
            );
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(muted(if visible {
                "PRÉVIA · 720p"
            } else {
                "SEU ESTÚDIO, DO SEU JEITO"
            }));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled_ui(self.has_sources() && self.save_receiver.is_none(), |ui| {
                        button(
                            ui,
                            Icon::Save,
                            "Salvar imagem",
                            ButtonKind::Secondary,
                            154.0,
                        )
                    })
                    .inner
                    .on_hover_text("Exportar a composição em PNG · Ctrl+S")
                    .clicked()
                {
                    self.save_snapshot(ctx);
                }
                if self.save_receiver.is_some() {
                    ui.spinner();
                }
            });
        });
        ui.add_space(12.0);
        self.notification(ui);
    }

    fn notification(&mut self, ui: &mut Ui) {
        if self.notice.is_empty() {
            return;
        }
        if self.notice != self.notice_seen {
            self.notice_seen = self.notice.clone();
            self.notice_at = Instant::now();
        }
        if !self.notice_error && self.notice_at.elapsed() > Duration::from_secs(8) {
            return;
        }
        let color = if self.notice_error { AMBER } else { ACCENT };
        Frame::new()
            .fill(color.gamma_multiply(0.08))
            .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.22)))
            .corner_radius(10)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 22.0), Sense::hover());
                    paint_icon(
                        ui.painter(),
                        if self.notice_error {
                            Icon::Help
                        } else {
                            Icon::Check
                        },
                        rect.shrink2(vec2(0.0, 2.0)),
                        color,
                    );
                    ui.vertical(|ui| {
                        ui.set_max_width((ui.available_width() - 36.0).max(120.0));
                        ui.label(RichText::new(&self.notice).size(13.0).color(color));
                        if self.notice == "Sua composição foi salva."
                            && let Some(path) = &self.saved_path
                            && ui.link("Mostrar na pasta").clicked()
                        {
                            reveal_file(path);
                        }
                    });
                    if icon_button(ui, Icon::Close, "Fechar aviso").clicked() {
                        self.notice.clear();
                    }
                });
            });
    }

    fn draw_picker(&mut self, ctx: &Context) {
        let busy = self.pending_desktop.is_some() || self.pending_camera.is_some();
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
            self.cancel_pending();
        }
        if let Some(kind) = refresh {
            self.open_picker(ctx, kind);
        }
        if !close {
            match selected {
                Some(Choice::Desktop(kind, source)) => self.start_desktop(ctx, kind, source),
                Some(Choice::Camera(device)) => self.start_camera(ctx, device),
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
                for (n,heading,body) in [("01","Monte sua composição","Adicione uma tela ou janela, uma imagem e sua câmera. Você pode combinar as três fontes."),("02","Encontre o melhor enquadramento","Selecione uma fonte para ajustar. Arraste a imagem ou a câmera na prévia; o ícone de olho mostra ou oculta cada fonte."),("03","Leve sua imagem para outro aplicativo","Inicie a câmera virtual e escolha Mini Studio RS Cam no aplicativo de chamada. Ou salve a composição como PNG.")] {
                    ui.horizontal_top(|ui|{badge(ui,n,ACCENT);ui.vertical(|ui|{ui.label(title(heading,15.0));ui.label(muted(body));});});ui.add_space(6.0);
                }
                ui.separator();
                ui.horizontal(|ui|{section(ui,"ATALHOS");ui.label(muted("Ctrl+O  Imagem     Ctrl+S  Salvar     Del  Remover     F1  Ajuda"));});
                ui.add_space(4.0);section(ui,"TAMANHO DA INTERFACE");
                ui.horizontal(|ui| {for scale in [0.9,1.0,1.1,1.25]{if ui.selectable_label((self.scale-scale).abs()<0.01,format!("{:.0}%",scale*100.0)).clicked(){self.scale=scale;ctx.set_zoom_factor(scale);}}});
                ui.label(muted("Seus ajustes são salvos automaticamente. Fontes de captura sempre são abertas por você."));
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
        if self.picker.is_some() || self.help_open || self.setup_open || ctx.wants_keyboard_input()
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::O))
            && self.image_receiver.is_none()
        {
            self.open_image(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::S))
            && self.has_sources()
            && self.save_receiver.is_none()
        {
            self.save_snapshot(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete))
            && let Some(source) = self.selected
        {
            self.remove_source(source);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.selected = None;
        }
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()))
            && self.image_receiver.is_none()
        {
            self.load_image(ctx, Some(path));
        }
    }
}

impl Studio {
    fn render(&mut self, ctx: &Context) {
        self.receive_frames(ctx);
        self.shortcuts(ctx);
        self.header(ctx);
        self.footer(ctx);
        let compact = ctx.content_rect().width() < 1180.0;
        self.sources_panel(ctx, compact);
        if !compact {
            SidePanel::right("inspector_panel")
                .exact_width(286.0)
                .resizable(false)
                .frame(panel())
                .show(ctx, |ui| {
                    TopBottomPanel::bottom("output_dock")
                        .frame(Frame::new().inner_margin(Margin {
                            top: 16,
                            ..Margin::ZERO
                        }))
                        .show_inside(ui, |ui| self.output_card(ui, ctx));
                    ScrollArea::vertical()
                        .scroll_bar_visibility(scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                        .id_salt("inspector_scroll")
                        .show(ui, |ui| self.inspector(ui, ctx));
                });
        }
        CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(24))
            .show(ctx, |ui| self.canvas(ui, ctx));
        self.draw_picker(ctx);
        self.dialogs(ctx);
        if let Some(output) = &self.virtual_camera {
            output.update(self.scene());
        }
        if self.preferences_at.elapsed() > Duration::from_secs(2) {
            self.persist_preferences();
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
        self.stop_all();
    }
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

fn source_tile(
    ui: &mut Ui,
    icon: Icon,
    name: &str,
    kind: &str,
    selected: bool,
    visible: bool,
) -> (bool, bool) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 70.0), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::SelectableLabel, true, name));
    let p = ui.painter();
    p.rect_filled(
        rect,
        10,
        if selected {
            ACCENT_DARK
        } else if response.hovered() {
            HOVER
        } else {
            CARD
        },
    );
    p.rect_stroke(
        rect,
        10,
        Stroke::new(
            1.0_f32,
            if selected {
                ACCENT.gamma_multiply(0.55)
            } else {
                LINE
            },
        ),
        StrokeKind::Inside,
    );
    paint_icon(
        p,
        icon,
        Rect::from_center_size(pos2(rect.left() + 24.0, rect.center().y), vec2(20.0, 20.0)),
        if selected { ACCENT } else { MUTED },
    );
    let text_rect = Rect::from_min_max(
        pos2(rect.left() + 44.0, rect.top()),
        pos2(rect.right() - 42.0, rect.bottom()),
    );
    let p = p.with_clip_rect(text_rect);
    p.text(
        pos2(text_rect.left(), rect.top() + 13.0),
        Align2::LEFT_TOP,
        name,
        FontId::proportional(13.0),
        if visible { TEXT } else { MUTED },
    );
    p.text(
        pos2(text_rect.left(), rect.top() + 37.0),
        Align2::LEFT_TOP,
        if visible { kind } else { "Oculta" },
        FontId::proportional(11.0),
        MUTED,
    );
    let eye_rect =
        Rect::from_center_size(pos2(rect.right() - 22.0, rect.center().y), vec2(30.0, 34.0));
    let eye = ui.interact(eye_rect, response.id.with("eye"), Sense::click());
    eye.widget_info(|| {
        WidgetInfo::labeled(
            WidgetType::Button,
            true,
            if visible {
                "Ocultar fonte"
            } else {
                "Mostrar fonte"
            },
        )
    });
    paint_icon(
        ui.painter(),
        if visible { Icon::Eye } else { Icon::Hidden },
        eye_rect.shrink2(vec2(6.0, 8.0)),
        if visible { ACCENT } else { MUTED },
    );
    let toggle = eye
        .clone()
        .on_hover_text(if visible {
            "Ocultar fonte"
        } else {
            "Mostrar fonte"
        })
        .clicked();
    (response.on_hover_text(name).clicked() && !toggle, toggle)
}

fn full_slider(ui: &mut Ui, label: &str, value: &mut u32, range: std::ops::RangeInclusive<u32>) {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 18.0;
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(12.0).color(MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{value}%")).size(12.0));
            });
        });
        ui.spacing_mut().slider_width = ui.available_width();
        ui.add(Slider::new(value, range).show_value(false));
    });
}
fn percent_slider(ui: &mut Ui, label: &str, value: &mut f32) {
    let mut percentage = *value * 100.0;
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 18.0;
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(12.0).color(MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{percentage:.0}%")).size(12.0));
            });
        });
        ui.spacing_mut().slider_width = ui.available_width();
        if ui
            .add(Slider::new(&mut percentage, 0.0..=100.0).show_value(false))
            .changed()
        {
            *value = percentage / 100.0;
        }
    });
}
fn drag_position(start: f32, delta: f32, space: f32) -> f32 {
    if space > 1.0 {
        (start + delta / space).clamp(0.0, 1.0)
    } else {
        start
    }
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
    use crate::{Options, Pixels, image_source};
    use std::sync::mpsc;

    struct Harness {
        ctx: Context,
        app: Studio,
        size: Vec2,
        controls: std::collections::HashMap<String, Rect>,
    }

    impl Harness {
        fn new(size: Vec2) -> Self {
            let ctx = Context::default();
            ctx.enable_accesskit();
            let app = Studio::initialize(
                &ctx,
                "Teste".into(),
                Options {
                    // Evita ler/gravar preferências pessoais durante os testes.
                    screenshot: Some("unused.png".into()),
                    ..Default::default()
                },
            );
            Self {
                ctx,
                app,
                size,
                controls: Default::default(),
            }
        }

        fn frame(&mut self, events: Vec<Event>) {
            let output = self.ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                    events,
                    ..Default::default()
                },
                |ctx| self.app.render(ctx),
            );
            self.controls.clear();
            for (_, node) in output.platform_output.accesskit_update.unwrap().nodes {
                if let (Some(label), Some(bounds)) = (node.label(), node.bounds()) {
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

        fn image_result(&mut self, result: Result<Option<image_source::LoadedImage>, String>) {
            let (sender, receiver) = mpsc::channel();
            sender.send(result).unwrap();
            self.app.image_receiver = Some(receiver);
            self.frame(vec![]);
            self.frame(vec![]);
        }

        fn add_image(&mut self) {
            self.image_result(Ok(Some(image_source::LoadedImage {
                name: "Arte.png".into(),
                pixels: Pixels {
                    width: 2,
                    height: 2,
                    rgba: [255, 80, 0, 255].repeat(4).into(),
                },
            })));
        }
    }

    #[test]
    fn source_visibility_and_delete_work_through_ui() {
        let mut h = Harness::new(vec2(1440.0, 900.0));
        h.add_image();
        assert_eq!(h.app.scene().layers().len(), 2);
        h.click("Ocultar fonte");
        assert_eq!(h.app.scene().layers().len(), 1);
        assert!(h.app.image.is_some());
        h.click("Mostrar fonte");
        assert_eq!(h.app.scene().layers().len(), 2);
        h.frame(vec![Event::Key {
            key: Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        assert!(h.app.image.is_none());
        assert!(h.app.selected.is_none());
    }

    #[test]
    fn compact_inspector_keeps_output_accessible_and_errors_keep_image() {
        let mut h = Harness::new(vec2(1024.0, 720.0));
        h.add_image();
        assert!(h.app.inspector_tab);
        let output = h
            .controls
            .get("Iniciar câmera virtual")
            .or_else(|| h.controls.get("Configurar câmera virtual"))
            .unwrap();
        assert!(output.bottom() < h.size.y && output.left() >= 0.0);
        h.image_result(Err("Arquivo inválido".into()));
        assert_eq!(h.app.image.as_ref().unwrap().loaded.name, "Arte.png");
        assert!(h.app.notice_error);
        h.image_result(Ok(None));
        assert!(h.app.image.is_some());
        assert!(!h.app.notice_error);
        h.click("Fontes");
        assert!(!h.app.inspector_tab);
        h.frame(vec![
            Event::PointerMoved(pos2(100.0, 300.0)),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(0.0, -650.0),
                modifiers: Modifiers::NONE,
            },
        ]);
        for _ in 0..30 {
            h.frame(vec![]);
        }
        h.click("Arte.png");
        assert!(h.app.inspector_tab);
    }

    #[test]
    fn modal_prevents_delete_shortcut_from_removing_source() {
        let mut h = Harness::new(vec2(1024.0, 720.0));
        h.add_image();
        h.app.help_open = true;
        h.frame(vec![Event::Key {
            key: Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        assert!(h.app.image.is_some());
    }

    #[test]
    fn help_opens_by_keyboard_and_closes_by_button() {
        let mut h = Harness::new(vec2(1024.0, 720.0));
        h.frame(vec![]);
        h.frame(vec![Event::Key {
            key: Key::F1,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        assert!(h.app.help_open);
        h.click("Fechar ajuda");
        assert!(!h.app.help_open);
    }

    #[test]
    fn dragging_keeps_source_inside_canvas_and_handles_full_size() {
        assert_eq!(drag_position(0.5, 200.0, 100.0), 1.0);
        assert_eq!(drag_position(0.5, -200.0, 100.0), 0.0);
        assert_eq!(drag_position(0.5, 200.0, 0.0), 0.5);
        assert_eq!(drag_position(0.25, 25.0, 100.0), 0.5);
    }
}
