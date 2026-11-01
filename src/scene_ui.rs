use crate::{
    PickerKind, Studio,
    composition::{Source, Transform},
    scenes::{SavedScene, SceneLibrary},
    theme::*,
};
use eframe::egui::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SceneTemplate {
    #[default]
    Empty,
    ScreenCamera,
    WindowCamera,
    SideBySide,
}

impl SceneTemplate {
    pub(crate) const ALL: [Self; 4] = [
        Self::Empty,
        Self::ScreenCamera,
        Self::WindowCamera,
        Self::SideBySide,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Empty => "Cena vazia",
            Self::ScreenCamera => "Tela + câmera no canto",
            Self::WindowCamera => "Janela + câmera no canto",
            Self::SideBySide => "Duas janelas lado a lado",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Empty => "Adicione suas fontes e monte a composição livremente.",
            Self::ScreenCamera | Self::WindowCamera => {
                "Escolha o conteúdo e depois a câmera. O enquadramento fica pronto automaticamente."
            }
            Self::SideBySide => {
                "Escolha a janela da esquerda e depois a da direita. As proporções das imagens são preservadas."
            }
        }
    }

    fn step(self, index: usize) -> Option<(PickerKind, Transform, &'static str)> {
        let full = Transform::default();
        match (self, index) {
            (Self::ScreenCamera, 0) => Some((PickerKind::Screen, full, "Escolha a tela principal")),
            (Self::WindowCamera, 0) => {
                Some((PickerKind::Window, full, "Escolha a janela principal"))
            }
            (Self::ScreenCamera | Self::WindowCamera, 1) => Some((
                PickerKind::Camera,
                Transform {
                    x: 0.70,
                    y: 0.66,
                    width: 0.28,
                    height: 0.30,
                    ..full
                },
                "Escolha a câmera para o canto inferior direito",
            )),
            (Self::SideBySide, 0 | 1) => Some((
                PickerKind::Window,
                Transform {
                    x: if index == 0 { 0.02 } else { 0.51 },
                    y: 0.04,
                    width: 0.47,
                    height: 0.92,
                    ..full
                },
                if index == 0 {
                    "Escolha a janela da esquerda"
                } else {
                    "Escolha a janela da direita"
                },
            )),
            _ => None,
        }
    }
}

pub(crate) struct TemplateSetup {
    template: SceneTemplate,
    step: usize,
}

pub(crate) enum SceneDialog {
    Create {
        name: String,
        template: SceneTemplate,
    },
    Rename {
        name: String,
    },
    Delete,
}
impl Studio {
    pub(crate) fn apply_scene_template(&mut self, ctx: &Context, template: SceneTemplate) {
        if !self.current_scene().sources.is_empty() || self.image_receiver.is_some() {
            return;
        }
        self.cancel_scene_template();
        self.scene_template = Some(TemplateSetup { template, step: 0 });
        self.open_template_step(ctx);
    }

    pub(crate) fn cancel_scene_template(&mut self) {
        self.scene_template = None;
    }

    pub(crate) fn scene_template_progress(&self) -> Option<String> {
        let setup = self.scene_template.as_ref()?;
        let (_, _, instruction) = setup.template.step(setup.step)?;
        Some(format!("Etapa {} de 2 · {instruction}", setup.step + 1))
    }

    fn open_template_step(&mut self, ctx: &Context) {
        let Some(setup) = self.scene_template.take() else {
            return;
        };
        if let Some((kind, _, _)) = setup.template.step(setup.step) {
            self.open_picker(ctx, kind, None);
            self.scene_template = Some(setup);
        }
    }

    pub(crate) fn refresh_picker(&mut self, ctx: &Context, kind: PickerKind) {
        let setup = self.scene_template.take();
        self.open_picker(ctx, kind, self.picker_target);
        self.scene_template = setup;
    }

    pub(crate) fn accept_template_source(&mut self, ctx: &Context, id: Source) {
        let Some(mut setup) = self.scene_template.take() else {
            return;
        };
        if let Some((_, transform, _)) = setup.template.step(setup.step)
            && let Some(source) = self.source_mut(id)
        {
            source.transform = transform;
            setup.step += 1;
            self.scene_template = Some(setup);
            self.persist_scenes(false);
            self.open_template_step(ctx);
        }
    }

    pub(crate) fn initialize_scenes(&mut self) {
        if self.options.screenshot.is_none() && self.options.benchmark.is_none() {
            match SceneLibrary::path().and_then(|path| SceneLibrary::load(&path)) {
                Ok(Some(library)) => self.scenes = library,
                Ok(None) => {}
                Err(error) => {
                    self.scenes_load_error = Some(error.clone());
                    self.show_error(format!("Não foi possível ler as cenas: {error}. O arquivo original será preservado."));
                }
            }
        }
        self.next_source_id = self.scenes.next_source_id();
        self.saved_scenes = self.scenes.clone();
    }
    pub(crate) fn persist_scenes(&mut self, explicit: bool) {
        if self.options.screenshot.is_some() || self.options.benchmark.is_some() {
            return;
        }
        if let Some(error) = &self.scenes_load_error {
            if explicit {
                self.show_error(format!("Não foi possível salvar: {error}. O arquivo scenes.json original foi preservado."));
            }
            return;
        }
        if explicit || self.scenes != self.saved_scenes {
            match SceneLibrary::path().and_then(|path| self.scenes.save(&path)) {
                Ok(()) => {
                    self.saved_scenes = self.scenes.clone();
                    if explicit {
                        self.notice_error = false;
                        self.notice = format!("Cena ‘{}’ salva.", self.current_scene().name);
                    }
                }
                Err(error) => self.show_error(format!("Não foi possível salvar as cenas: {error}")),
            }
        }
    }
    pub(crate) fn switch_scene(&mut self, ctx: &Context, index: usize) {
        if index == self.scenes.active || index >= self.scenes.scenes.len() {
            return;
        }
        self.clear_scene_runtime();
        self.scenes.active = index;
        self.activate_scene(ctx);
        self.persist_scenes(false);
    }
    pub(crate) fn activate_scene(&mut self, ctx: &Context) {
        let ids: Vec<_> = self.current_scene().sources.iter().map(|s| s.id).collect();
        for id in ids {
            if self
                .runtimes
                .get(&id)
                .is_none_or(|r| r.borrow().error.is_some())
            {
                self.activate_source(ctx, id);
            }
        }
    }
    fn create_scene(&mut self, ctx: &Context, name: String) {
        self.scenes.scenes.push(SavedScene {
            name,
            sources: vec![],
        });
        self.switch_scene(ctx, self.scenes.scenes.len() - 1);
    }
    fn duplicate_scene(&mut self, ctx: &Context) {
        let mut scene = self.current_scene().clone();
        scene.name = self.scenes.unique_name(&format!("{} — cópia", scene.name));
        for source in &mut scene.sources {
            source.id = self.allocate_source_id();
        }
        self.scenes.scenes.push(scene);
        self.switch_scene(ctx, self.scenes.scenes.len() - 1);
    }
    fn delete_scene(&mut self, ctx: &Context) {
        if self.scenes.scenes.len() <= 1 {
            return;
        }
        self.scenes.scenes.remove(self.scenes.active);
        self.clear_scene_runtime();
        self.scenes.active = self.scenes.active.min(self.scenes.scenes.len() - 1);
        self.activate_scene(ctx);
        self.persist_scenes(false);
    }
    pub(crate) fn scenes_dock(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.label(title("Cenas", 16.0));
            ui.label(muted(format!("{}", self.scenes.scenes.len())));
        });
        TopBottomPanel::bottom("scene_toolbar")
            .frame(Frame::NONE)
            .show_inside(ui, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if crate::studio_ui::glyph_button(ui, "+", "Nova cena").clicked() {
                        self.scene_dialog = Some(SceneDialog::Create {
                            name: self.scenes.unique_name("Nova cena"),
                            template: SceneTemplate::Empty,
                        });
                    }
                    if ui
                        .add_enabled_ui(self.scenes.scenes.len() > 1, |ui| {
                            crate::studio_ui::small_icon(ui, Icon::Trash, "Excluir cena")
                        })
                        .inner
                        .clicked()
                    {
                        self.scene_dialog = Some(SceneDialog::Delete);
                    }
                    ui.menu_button("•••", |ui| {
                        if ui.button("Renomear cena").clicked() {
                            self.scene_dialog = Some(SceneDialog::Rename {
                                name: self.current_scene().name.clone(),
                            });
                            ui.close();
                        }
                        if ui.button("Duplicar cena").clicked() {
                            self.duplicate_scene(ctx);
                            ui.close();
                        }
                    });
                });
            });
        ScrollArea::vertical().id_salt("scene_list").show(ui, |ui| {
            let mut switch = None;
            let mut rename = false;
            for (index, scene) in self.scenes.scenes.iter().enumerate() {
                let r = ui.add_sized(
                    [ui.available_width(), 28.0],
                    Button::selectable(index == self.scenes.active, &scene.name).truncate(),
                );
                if r.clicked() {
                    switch = Some(index);
                }
                if r.double_clicked() {
                    switch = Some(index);
                    rename = true;
                }
                r.on_hover_text(&scene.name);
            }
            if let Some(index) = switch {
                self.switch_scene(ctx, index);
            }
            if rename {
                self.scene_dialog = Some(SceneDialog::Rename {
                    name: self.current_scene().name.clone(),
                });
            }
        });
    }
    pub(crate) fn scene_dialogs(&mut self, ctx: &Context) {
        let Some(mut dialog) = self.scene_dialog.take() else {
            return;
        };
        let mut accept = false;
        let mut cancel = false;
        let response = Modal::new(Id::new("scene_dialog"))
            .frame(modal_frame())
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(title(
                    match dialog {
                        SceneDialog::Create { .. } => "Nova cena",
                        SceneDialog::Rename { .. } => "Renomear cena",
                        SceneDialog::Delete => "Excluir cena?",
                    },
                    23.0,
                ));
                match &mut dialog {
                    SceneDialog::Create { name, .. } | SceneDialog::Rename { name } => {
                        let label = ui.label(muted("Nome da cena"));
                        ui.add(
                            TextEdit::singleline(name)
                                .id_salt("scene_name")
                                .char_limit(80)
                                .desired_width(f32::INFINITY),
                        )
                        .labelled_by(label.id);
                    }
                    SceneDialog::Delete => {
                        ui.label(format!(
                            "Excluir ‘{}’? Os arquivos das fontes serão mantidos.",
                            self.current_scene().name
                        ));
                    }
                }
                if let SceneDialog::Create { template, .. } = &mut dialog {
                    ui.add_space(8.0);
                    ui.label(muted("Começar com"));
                    for choice in SceneTemplate::ALL {
                        ui.selectable_value(template, choice, choice.label());
                    }
                    ui.add_space(6.0);
                    ui.label(muted(template.description()));
                }
                let valid = match &dialog {
                    SceneDialog::Create { name, .. } | SceneDialog::Rename { name } => {
                        !name.trim().is_empty()
                    }
                    SceneDialog::Delete => true,
                };
                ui.horizontal(|ui| {
                    cancel = ui.button("Cancelar").clicked();
                    let label = match dialog {
                        SceneDialog::Create { .. } => "Criar cena",
                        SceneDialog::Rename { .. } => "Salvar nome",
                        SceneDialog::Delete => "Confirmar exclusão",
                    };
                    accept = ui.add_enabled(valid, Button::new(label)).clicked();
                });
            });
        if accept {
            match dialog {
                SceneDialog::Create { name, template } => {
                    self.create_scene(ctx, name.trim().into());
                    self.apply_scene_template(ctx, template);
                }
                SceneDialog::Rename { name } => {
                    self.current_scene_mut().name = name.trim().into();
                    self.persist_scenes(false);
                }
                SceneDialog::Delete => self.delete_scene(ctx),
            }
        } else if !cancel && !response.should_close() {
            self.scene_dialog = Some(dialog);
        }
    }
}

#[cfg(test)]
mod template_tests {
    use super::*;
    use crate::{
        Options,
        scenes::{SavedSource, SourceInput},
    };
    use std::path::PathBuf;

    fn studio() -> (Studio, Context) {
        let ctx = Context::default();
        let app = Studio::initialize(
            &ctx,
            "teste".into(),
            Options {
                screenshot: Some(PathBuf::from("unused.png")),
                ..Default::default()
            },
        );
        (app, ctx)
    }

    #[test]
    fn templates_preserve_aspect_ratio_and_keep_sources_inside_canvas() {
        for template in SceneTemplate::ALL {
            for index in 0..2 {
                if let Some((_, transform, _)) = template.step(index) {
                    assert!(!transform.stretch);
                    assert!(transform.width > 0.0 && transform.height > 0.0);
                    assert!(transform.x >= 0.0 && transform.x + transform.width <= 1.0);
                    assert!(transform.y >= 0.0 && transform.y + transform.height <= 1.0);
                }
            }
            assert!(template.step(2).is_none());
        }
        let (_, left, _) = SceneTemplate::SideBySide.step(0).unwrap();
        let (_, right, _) = SceneTemplate::SideBySide.step(1).unwrap();
        assert!(left.x + left.width < right.x);
        assert!(SceneTemplate::Empty.step(0).is_none());
    }

    #[test]
    fn camera_template_fits_four_by_three_without_distortion() {
        use crate::composition::{Pixels, Scene, SceneSource};
        let (_, transform, _) = SceneTemplate::ScreenCamera.step(1).unwrap();
        let scene = Scene {
            sources: vec![SceneSource {
                id: 1,
                pixels: Pixels {
                    width: 640,
                    height: 480,
                    rgba: std::sync::Arc::from([]),
                },
                transform,
            }],
        };
        let layer = &scene.layers()[1];
        assert!((layer.rect[2] / layer.rect[3] - 4.0 / 3.0).abs() < 0.01);
        // As coordenadas da camada são arredondadas ao pixel mais próximo.
        assert!(layer.rect[0] + 0.5 >= 1280.0 * 0.70);
        assert!(layer.rect[1] + 0.5 >= 720.0 * 0.66);
        assert!(layer.rect[0] + layer.rect[2] <= 1280.0);
        assert!(layer.rect[1] + layer.rect[3] <= 720.0);
    }

    #[test]
    fn final_choice_gets_template_layout_and_finishes_setup() {
        let (mut app, ctx) = studio();
        app.current_scene_mut().sources.push(SavedSource {
            id: 1,
            name: "Câmera escolhida".into(),
            input: SourceInput::Image(PathBuf::from("unused.png")),
            visible: true,
            locked: false,
            transform: Transform::default(),
        });
        app.scene_template = Some(TemplateSetup {
            template: SceneTemplate::ScreenCamera,
            step: 1,
        });
        assert!(
            app.scene_template_progress()
                .unwrap()
                .contains("Etapa 2 de 2")
        );
        app.accept_template_source(&ctx, 1);
        assert!(app.scene_template.is_none());
        assert!(app.picker.is_none());
        assert_eq!(
            app.source(1).unwrap().transform,
            SceneTemplate::ScreenCamera.step(1).unwrap().1
        );
    }

    #[test]
    fn cancel_and_scene_reset_clear_pending_layout_without_removing_sources() {
        let (mut app, _) = studio();
        app.current_scene_mut().sources.push(SavedSource {
            id: 1,
            name: "Já escolhida".into(),
            input: SourceInput::Image(PathBuf::from("unused.png")),
            visible: true,
            locked: false,
            transform: Transform::default(),
        });
        app.scene_template = Some(TemplateSetup {
            template: SceneTemplate::SideBySide,
            step: 1,
        });
        app.cancel_scene_template();
        assert!(app.scene_template.is_none());
        assert_eq!(app.current_scene().sources.len(), 1);
        app.scene_template = Some(TemplateSetup {
            template: SceneTemplate::ScreenCamera,
            step: 1,
        });
        app.clear_scene_runtime();
        assert!(app.scene_template.is_none());
        assert_eq!(app.current_scene().sources.len(), 1);
    }
}
