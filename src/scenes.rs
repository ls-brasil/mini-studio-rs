use crate::composition::{CameraPosition, ImageMode, ImageSettings, Transform, camera_rect};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesktopSource {
    // HWNDs só valem nesta sessão; não devem ser restaurados do disco.
    #[serde(skip)]
    pub selected_id: Option<u32>,
    pub window: bool,
    #[serde(default = "capture_cursor_default")]
    pub capture_cursor: bool,
    pub name: String,
    pub detail: String,
}

fn capture_cursor_default() -> bool {
    true
}

impl DesktopSource {
    pub fn resolve(
        &self,
        sources: Vec<crate::native_capture::CaptureSource>,
    ) -> Option<crate::native_capture::CaptureSource> {
        let mut matches = sources.into_iter().filter(|source| {
            source.name == self.name
                && (!self.window
                    || source.detail.split(" · ").next() == self.detail.split(" · ").next())
        });
        if let Some(id) = self.selected_id {
            return matches.find(|source| source.id == id);
        }
        match (matches.next(), matches.next()) {
            (Some(source), None) => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraSource {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NdiSource {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SourceInput {
    Desktop(DesktopSource),
    Camera(CameraSource),
    Ndi(NdiSource),
    Image(PathBuf),
}
impl SourceInput {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Desktop(d) => {
                if d.window {
                    "Janela"
                } else {
                    "Tela"
                }
            }
            Self::Camera(_) => "Câmera",
            Self::Ndi(_) => "NDI",
            Self::Image(_) => "Imagem",
        }
    }
    pub fn name(&self) -> String {
        match self {
            Self::Desktop(d) => d.name.clone(),
            Self::Camera(c) => c.name.clone(),
            Self::Ndi(source) => source.name.clone(),
            Self::Image(path) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceDefinition {
    pub id: u64,
    pub name: String,
    pub input: SourceInput,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSource {
    #[serde(default)]
    pub source_id: u64,
    pub id: u64,
    #[serde(default, skip_serializing)]
    pub name: String,
    #[serde(default = "empty_input", skip_serializing)]
    pub input: SourceInput,
    pub visible: bool,
    pub locked: bool,
    pub transform: Transform,
}
fn empty_input() -> SourceInput {
    SourceInput::Image(PathBuf::new())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedScene {
    pub name: String,
    pub sources: Vec<SavedSource>,
}
impl Default for SavedScene {
    fn default() -> Self {
        Self {
            name: "Cena 1".into(),
            sources: vec![],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneLibrary {
    #[serde(default)]
    pub sources: Vec<SourceDefinition>,
    pub version: u32,
    pub active: usize,
    pub scenes: Vec<SavedScene>,
}
impl Default for SceneLibrary {
    fn default() -> Self {
        Self {
            version: 3,
            sources: vec![],
            active: 0,
            scenes: vec![SavedScene::default()],
        }
    }
}
impl SceneLibrary {
    // Scene entries cache the definition for the editor; source_id is the shared identity.
    // A changed cache commits a definition edit; composition fields remain local.
    pub fn reconcile_sources(&mut self) -> Vec<u64> {
        let mut next = self.sources.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        for scene in &mut self.scenes {
            for item in &mut scene.sources {
                if item.source_id == 0 {
                    item.source_id = next;
                    next += 1;
                    self.sources.push(SourceDefinition {
                        id: item.source_id,
                        name: item.name.clone(),
                        input: item.input.clone(),
                    });
                }
            }
        }
        let mut changed = Vec::new();
        for definition in &mut self.sources {
            if let Some(item) = self.scenes.iter().flat_map(|s| &s.sources).find(|s| {
                s.source_id == definition.id
                    && (s.name != definition.name || s.input != definition.input)
            }) {
                if item.input != definition.input {
                    changed.push(definition.id);
                }
                definition.name = item.name.clone();
                definition.input = item.input.clone();
            }
        }
        let mut invalidated = Vec::new();
        for item in self.scenes.iter_mut().flat_map(|s| &mut s.sources) {
            if let Some(definition) = self.sources.iter().find(|s| s.id == item.source_id) {
                let stale = item.input != definition.input;
                item.name = definition.name.clone();
                item.input = definition.input.clone();
                if stale && changed.contains(&item.source_id) {
                    invalidated.push(item.id);
                }
            }
        }
        invalidated
    }

    pub fn path() -> Result<PathBuf, String> {
        std::env::var_os("LOCALAPPDATA")
            .map(|base| PathBuf::from(base).join("MiniStudioRS").join("scenes.json"))
            .ok_or_else(|| "Não foi possível localizar a pasta de cenas.".into())
    }
    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        Self::decode(&bytes).map(Some)
    }
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let is_v3 = value["version"].as_u64() == Some(3);
        let mut library: Self = match value["version"].as_u64() {
            Some(2) | Some(3) => serde_json::from_value(value).map_err(|e| e.to_string())?,
            Some(1) => {
                let old: LegacyLibrary =
                    serde_json::from_value(value).map_err(|e| e.to_string())?;
                let mut next = 1;
                Self {
                    version: 3,
                    sources: vec![],
                    active: old.active,
                    scenes: old
                        .scenes
                        .into_iter()
                        .map(|scene| scene.migrate(&mut next))
                        .collect(),
                }
            }
            _ => return Err("Versão do arquivo de cenas não reconhecida.".into()),
        };
        if is_v3 {
            let mut ids = std::collections::HashSet::new();
            for definition in &library.sources {
                if definition.id == 0 || definition.id == u64::MAX || !ids.insert(definition.id) {
                    return Err("Identificadores inválidos na biblioteca de fontes.".into());
                }
            }
            for item in library.scenes.iter_mut().flat_map(|s| &mut s.sources) {
                let definition = library
                    .sources
                    .iter()
                    .find(|s| s.id == item.source_id)
                    .ok_or("Referência a uma fonte inexistente na biblioteca.")?;
                item.name = definition.name.clone();
                item.input = definition.input.clone();
            }
        }
        if library.scenes.is_empty() {
            return Err("O arquivo de cenas está vazio.".into());
        }
        library.active = library.active.min(library.scenes.len() - 1);
        let mut ids = std::collections::HashSet::new();
        for (index, scene) in library.scenes.iter_mut().enumerate() {
            scene.name = scene.name.trim().chars().take(80).collect();
            if scene.name.is_empty() {
                scene.name = format!("Cena {}", index + 1);
            }
            for source in &mut scene.sources {
                if source.id == 0 || source.id == u64::MAX || !ids.insert(source.id) {
                    return Err("Identificadores de fonte inválidos no arquivo de cenas.".into());
                }
                source.transform.sanitize();
                source.name = source.name.trim().chars().take(80).collect();
                if source.name.is_empty() {
                    source.name = source.input.name();
                }
            }
        }
        library.reconcile_sources();
        library.version = 3;
        Ok(library)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut normalized = self.clone();
        normalized.reconcile_sources();
        std::fs::create_dir_all(path.parent().ok_or("Pasta de cenas inválida.")?)
            .map_err(|e| e.to_string())?;
        let temp = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(&normalized).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(temp, path).map_err(|e| e.to_string())
    }
    pub fn next_source_id(&self) -> u64 {
        self.scenes
            .iter()
            .flat_map(|s| &s.sources)
            .map(|s| s.id)
            .max()
            .unwrap_or(0)
            + 1
    }
    pub fn unique_name(&self, base: &str) -> String {
        if !self.scenes.iter().any(|scene| scene.name == base) {
            return base.into();
        }
        for n in 2.. {
            let name = format!("{base} ({n})");
            if !self.scenes.iter().any(|scene| scene.name == name) {
                return name;
            }
        }
        unreachable!()
    }
}

#[derive(Deserialize)]
struct LegacyLibrary {
    active: usize,
    scenes: Vec<LegacyScene>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LegacyScene {
    pub name: String,
    pub desktop: Option<DesktopSource>,
    pub camera: Option<CameraSource>,
    pub image_path: Option<PathBuf>,
    pub source_names: [String; 3],
    pub desktop_visible: bool,
    pub camera_visible: bool,
    pub mirror: bool,
    pub camera_size: u32,
    pub camera_position: CameraPosition,
    pub image: ImageSettings,
}

impl Default for LegacyScene {
    fn default() -> Self {
        Self {
            name: "Cena 1".into(),
            desktop: None,
            camera: None,
            image_path: None,
            source_names: Default::default(),
            desktop_visible: true,
            camera_visible: true,
            mirror: false,
            camera_size: 25,
            camera_position: CameraPosition::BottomRight,
            image: ImageSettings::default(),
        }
    }
}

impl LegacyScene {
    fn migrate(self, next: &mut u64) -> SavedScene {
        let mut sources = Vec::new();
        let mut add = |input: SourceInput, index: usize, visible: bool, transform: Transform| {
            let name = if self.source_names[index].trim().is_empty() {
                input.name()
            } else {
                self.source_names[index].clone()
            };
            sources.push(SavedSource {
                source_id: 0,
                id: *next,
                name,
                input,
                visible,
                locked: false,
                transform,
            });
            *next += 1;
        };
        let scale = self.image.size.clamp(5, 100) as f32 / 100.0;
        let image_transform = Transform {
            x: (1.0 - scale) * self.image.x,
            y: (1.0 - scale) * self.image.y,
            width: scale,
            height: scale,
            ..Default::default()
        };
        if self.image.mode == ImageMode::Background
            && let Some(path) = &self.image_path
        {
            add(
                SourceInput::Image(path.clone()),
                2,
                self.image.visible,
                image_transform,
            );
        }
        if let Some(desktop) = &self.desktop {
            add(
                SourceInput::Desktop(desktop.clone()),
                0,
                self.desktop_visible,
                Transform::default(),
            );
        }
        if self.image.mode == ImageMode::Overlay
            && let Some(path) = &self.image_path
        {
            add(
                SourceInput::Image(path.clone()),
                2,
                self.image.visible,
                image_transform,
            );
        }
        if let Some(camera) = self.camera {
            let rect = if self.desktop.is_some() || self.image_path.is_some() {
                camera_rect(
                    self.camera_size,
                    self.camera_position,
                    [0.0, 0.0, 1280.0, 720.0],
                )
            } else {
                [0.0, 0.0, 1280.0, 720.0]
            };
            let mut transform = Transform::from_rect(rect);
            transform.mirror = self.mirror;
            add(
                SourceInput::Camera(camera),
                1,
                self.camera_visible,
                transform,
            );
        }
        SavedScene {
            name: self.name,
            sources,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_library_keeps_layout_local_and_survives_removing_all_uses() {
        let old = br#"{"version":2,"active":0,"scenes":[{"name":"Cena","sources":[{"id":1,"name":"Tela","input":{"Desktop":{"window":false,"name":"Tela","detail":"Monitor"}},"visible":true,"locked":false,"transform":{}}]}]}"#;
        let mut library = SceneLibrary::decode(old).unwrap();
        let mut second = library.scenes[0].clone();
        second.name = "Outra".into();
        second.sources[0].id = 2;
        second.sources[0].transform.width = 0.4;
        second.sources[0].visible = false;
        library.scenes.push(second);
        library.scenes[0].sources[0].name = "Compartilhada".into();
        let SourceInput::Desktop(d) = &mut library.scenes[0].sources[0].input else {
            panic!()
        };
        d.capture_cursor = false;
        assert_eq!(library.reconcile_sources(), vec![2]);
        assert_eq!(library.scenes[1].sources[0].name, "Compartilhada");
        assert_eq!(library.scenes[1].sources[0].transform.width, 0.4);
        assert!(!library.scenes[1].sources[0].visible);
        assert_eq!(library.scenes[0].sources[0].transform.width, 1.0);
        let restored = SceneLibrary::decode(&serde_json::to_vec(&library).unwrap()).unwrap();
        assert_eq!(restored, library);
        for scene in &mut library.scenes {
            scene.sources.clear();
        }
        library.reconcile_sources();
        let restored = SceneLibrary::decode(&serde_json::to_vec(&library).unwrap()).unwrap();
        assert_eq!(restored.sources.len(), 1);
    }

    #[test]
    fn migration_preserves_distinct_sources_and_rejects_broken_references() {
        let old = br#"{"version":2,"active":0,"scenes":[{"name":"Cena","sources":[{"id":1,"name":"A","input":{"Image":"C:/a.png"},"visible":true,"locked":false,"transform":{}},{"id":2,"name":"B","input":{"Image":"C:/a.png"},"visible":false,"locked":true,"transform":{}}]}]}"#;
        let mut library = SceneLibrary::decode(old).unwrap();
        assert_eq!(library.sources.len(), 2);
        assert_ne!(
            library.scenes[0].sources[0].source_id,
            library.scenes[0].sources[1].source_id
        );
        library.sources.clear();
        assert!(SceneLibrary::decode(&serde_json::to_vec(&library).unwrap()).is_err());
    }

    #[test]
    fn ndi_channel_identity_round_trips_with_other_source_types() {
        let input = SourceInput::Ndi(NdiSource {
            name: "ESTÚDIO (Câmera principal)".into(),
        });
        let mut library = SceneLibrary::default();
        library.scenes[0].sources = vec![
            SavedSource {
                source_id: 0,
                id: 1,
                name: "Vídeo da rede".into(),
                input: input.clone(),
                visible: false,
                locked: true,
                transform: Transform {
                    x: 0.25,
                    width: 0.5,
                    mirror: true,
                    ..Default::default()
                },
            },
            SavedSource {
                source_id: 0,
                id: 2,
                name: "Fundo".into(),
                input: SourceInput::Image("C:/fundo.png".into()),
                visible: true,
                locked: false,
                transform: Transform::default(),
            },
        ];
        library.reconcile_sources();
        let restored = SceneLibrary::decode(&serde_json::to_vec(&library).unwrap()).unwrap();
        assert_eq!(restored, library);
        assert_eq!(input.label(), "NDI");
        assert_eq!(input.name(), "ESTÚDIO (Câmera principal)");
    }

    #[test]
    fn cursor_preference_defaults_on_for_old_scenes_and_round_trips_per_source() {
        let old = br#"{"version":2,"active":0,"scenes":[{"name":"Cena","sources":[{"id":1,"name":"Tela","input":{"Desktop":{"window":false,"name":"Tela","detail":"Monitor"}},"visible":true,"locked":false,"transform":{}}]}]}"#;
        let mut library = SceneLibrary::decode(old).unwrap();
        let mut duplicate = library.scenes[0].sources[0].clone();
        duplicate.id = 2;
        duplicate.source_id = 0;
        let SourceInput::Desktop(desktop) = &mut duplicate.input else {
            panic!("Esperava fonte de desktop");
        };
        assert!(desktop.capture_cursor);
        desktop.capture_cursor = false;
        desktop.window = true;
        library.scenes[0].sources.push(duplicate);
        library.reconcile_sources();
        let restored = SceneLibrary::decode(&serde_json::to_vec(&library).unwrap()).unwrap();
        assert_eq!(restored, library);
        for (source, expected) in restored.scenes[0].sources.iter().zip([true, false]) {
            let SourceInput::Desktop(desktop) = &source.input else {
                panic!("Esperava fonte de desktop");
            };
            assert_eq!(desktop.capture_cursor, expected);
        }
    }

    #[test]
    fn old_scenes_migrate_to_ordered_sources_without_losing_names_or_visibility() {
        let old=br#"{"version":1,"active":0,"scenes":[{"name":"Aula","desktop":{"window":true,"name":"Slides","detail":"App"},"image_path":"C:/fundo.png","source_names":["Minha janela","","Fundo"],"desktop_visible":false}]}"#;
        let library = SceneLibrary::decode(old).unwrap();
        assert_eq!(library.version, 3);
        assert_eq!(library.scenes[0].name, "Aula");
        let sources = &library.scenes[0].sources;
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].name, "Fundo");
        assert_eq!(sources[1].name, "Minha janela");
        assert!(!sources[1].visible);
        assert_ne!(sources[0].id, sources[1].id);
    }
    #[test]
    fn multiple_same_type_sources_round_trip_and_corrupt_file_is_preserved() {
        let mut library = SceneLibrary::default();
        for id in 1..=4 {
            library.scenes[0].sources.push(SavedSource {
                source_id: 0,
                id,
                name: format!("Imagem {id}"),
                input: SourceInput::Image(format!("C:/{id}.png").into()),
                visible: id != 2,
                locked: id == 3,
                transform: Transform {
                    x: id as f32 * 0.1,
                    ..Default::default()
                },
            });
        }
        let folder =
            std::env::temp_dir().join(format!("mini-studio-scene-v2-{}", std::process::id()));
        let path = folder.join("scenes.json");
        library.reconcile_sources();
        library.save(&path).unwrap();
        library.scenes[0].sources.reverse();
        library.reconcile_sources();
        library.save(&path).unwrap();
        assert_eq!(SceneLibrary::load(&path).unwrap().unwrap(), library);
        std::fs::write(&path, b"broken").unwrap();
        assert!(SceneLibrary::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }
    #[test]
    fn resized_window_matches_but_ambiguous_windows_do_not() {
        use crate::native_capture::CaptureSource;
        let saved = DesktopSource {
            selected_id: None,
            window: true,
            capture_cursor: true,
            name: "Slides".into(),
            detail: "App · 1280 × 720".into(),
        };
        let source = CaptureSource {
            id: 4,
            name: "Slides".into(),
            detail: "App · 1920 × 1080".into(),
        };
        assert_eq!(saved.resolve(vec![source.clone()]).unwrap().id, 4);
        assert!(saved.resolve(vec![source.clone(), source]).is_none());
    }

    #[test]
    fn explicit_window_selection_survives_duplicate_titles_and_resize() {
        use crate::native_capture::CaptureSource;
        let saved = DesktopSource {
            selected_id: Some(20),
            window: true,
            capture_cursor: true,
            name: "JW Library".into(),
            detail: "Application Frame Host · 894 × 1048".into(),
        };
        let first = CaptureSource {
            id: 10,
            name: saved.name.clone(),
            detail: saved.detail.clone(),
        };
        let second = CaptureSource {
            id: 20,
            detail: "Application Frame Host · 1920 × 1080".into(),
            ..first.clone()
        };
        assert_eq!(
            saved
                .resolve(vec![first.clone(), second.clone()])
                .unwrap()
                .id,
            20
        );
        // Se a selecionada fechar, não capturar silenciosamente a outra janela.
        assert!(saved.resolve(vec![first.clone()]).is_none());
        let restored: DesktopSource =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(restored.selected_id, None);
        assert!(restored.resolve(vec![first, second]).is_none());
    }
}
