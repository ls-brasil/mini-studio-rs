use crate::{
    Studio,
    scenes::{SavedScene, SceneLibrary},
    theme::*,
};
use eframe::egui::*;

pub enum SceneDialog {
    Create { name: String },
    Rename { name: String },
    Delete,
}
impl Studio {
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
        let response=Modal::new(Id::new("scene_dialog")).frame(modal_frame()).show(ctx,|ui|{
            ui.set_width(380.0);
            ui.label(title(match dialog {SceneDialog::Create{..}=>"Nova cena",SceneDialog::Rename{..}=>"Renomear cena",SceneDialog::Delete=>"Excluir cena?"},23.0));
            match &mut dialog {
                SceneDialog::Create{name}|SceneDialog::Rename{name}=>{
                    let label=ui.label(muted("Nome da cena"));
                    ui.add(TextEdit::singleline(name).id_salt("scene_name").char_limit(80).desired_width(f32::INFINITY)).labelled_by(label.id);
                }
                SceneDialog::Delete=>{ui.label(format!("Excluir ‘{}’? Os arquivos das fontes serão mantidos.",self.current_scene().name));}
            }
            if matches!(dialog,SceneDialog::Create{..}){ui.label(muted("Depois, use + em Fontes para montar a composição com telas, janelas, câmeras e imagens."));}
            let valid=match &dialog {SceneDialog::Create{name}|SceneDialog::Rename{name}=>!name.trim().is_empty(),SceneDialog::Delete=>true};
            ui.horizontal(|ui|{
                cancel=ui.button("Cancelar").clicked();
                let label=match dialog {SceneDialog::Create{..}=>"Criar cena",SceneDialog::Rename{..}=>"Salvar nome",SceneDialog::Delete=>"Confirmar exclusão"};
                accept=ui.add_enabled(valid,Button::new(label)).clicked();
            });
        });
        if accept {
            match dialog {
                SceneDialog::Create { name } => self.create_scene(ctx, name.trim().into()),
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
