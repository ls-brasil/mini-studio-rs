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
pub enum SourceInput {
    Desktop(DesktopSource),
    Camera(CameraSource),
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
            Self::Image(_) => "Imagem",
        }
    }
    pub fn name(&self) -> String {
        match self {
            Self::Desktop(d) => d.name.clone(),
            Self::Camera(c) => c.name.clone(),
            Self::Image(path) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSource {
    pub id: u64,
    pub name: String,
    pub input: SourceInput,
    pub visible: bool,
    pub locked: bool,
    pub transform: Transform,
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
    pub version: u32,
    pub active: usize,
    pub scenes: Vec<SavedScene>,
}
impl Default for SceneLibrary {
    fn default() -> Self {
        Self {
            version: 2,
            active: 0,
            scenes: vec![SavedScene::default()],
        }
    }
}
impl SceneLibrary {
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
        let mut library: Self = match value["version"].as_u64() {
            Some(2) => serde_json::from_value(value).map_err(|e| e.to_string())?,
            Some(1) => {
                let old: LegacyLibrary =
                    serde_json::from_value(value).map_err(|e| e.to_string())?;
                let mut next = 1;
                Self {
                    version: 2,
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
        Ok(library)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        std::fs::create_dir_all(path.parent().ok_or("Pasta de cenas inválida.")?)
            .map_err(|e| e.to_string())?;
        let temp = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
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
    fn cursor_preference_defaults_on_for_old_scenes_and_round_trips_per_source() {
        let old = br#"{"version":2,"active":0,"scenes":[{"name":"Cena","sources":[{"id":1,"name":"Tela","input":{"Desktop":{"window":false,"name":"Tela","detail":"Monitor"}},"visible":true,"locked":false,"transform":{}}]}]}"#;
        let mut library = SceneLibrary::decode(old).unwrap();
        let mut duplicate = library.scenes[0].sources[0].clone();
        duplicate.id = 2;
        let SourceInput::Desktop(desktop) = &mut duplicate.input else {
            panic!("Esperava fonte de desktop");
        };
        assert!(desktop.capture_cursor);
        desktop.capture_cursor = false;
        desktop.window = true;
        library.scenes[0].sources.push(duplicate);
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
        assert_eq!(library.version, 2);
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
        library.save(&path).unwrap();
        library.scenes[0].sources.reverse();
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
