use crate::composition::{CameraPosition, ImageSettings};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub mirror: bool,
    pub camera_size: u32,
    pub camera_position: CameraPosition,
    pub image: ImageSettings,
    pub guides: bool,
    pub scale: f32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            mirror: false,
            camera_size: 25,
            camera_position: CameraPosition::BottomRight,
            image: ImageSettings::default(),
            guides: false,
            scale: 1.0,
        }
    }
}

impl Preferences {
    fn sanitize(mut self) -> Self {
        self.camera_size = self.camera_size.clamp(15, 40);
        self.image.size = self.image.size.clamp(5, 100);
        self.image.x = if self.image.x.is_finite() {
            self.image.x.clamp(0.0, 1.0)
        } else {
            0.5
        };
        self.image.y = if self.image.y.is_finite() {
            self.image.y.clamp(0.0, 1.0)
        } else {
            0.5
        };
        self.scale = if self.scale.is_finite() {
            self.scale.clamp(0.9, 1.25)
        } else {
            1.0
        };
        // Uma nova fonte começa visível, sem restaurar dispositivos ou arquivos privados.
        self.image.visible = true;
        self
    }
    fn path() -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA").map(|base| {
            PathBuf::from(base)
                .join("MiniStudioRS")
                .join("preferences.json")
        })
    }
    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .unwrap_or_default()
            .sanitize()
    }
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("Não foi possível localizar a pasta de preferências.")?;
        std::fs::create_dir_all(path.parent().ok_or("Pasta inválida.")?)
            .map_err(|e| e.to_string())?;
        let temp = path.with_extension(format!("{}.tmp", std::process::id()));
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, &path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_and_out_of_bounds_values_are_safe() {
        let p = serde_json::from_str::<Preferences>(r#"{"camera_size":999,"scale":4.0}"#)
            .unwrap()
            .sanitize();
        assert_eq!(p.camera_size, 40);
        assert_eq!(p.scale, 1.25);
        assert_eq!(p.image, ImageSettings::default());
        assert!(serde_json::from_str::<Preferences>("broken").is_err());
    }
    #[test]
    fn settings_round_trip_preserves_layout() {
        let p = Preferences {
            mirror: true,
            camera_position: CameraPosition::TopLeft,
            guides: true,
            ..Preferences::default()
        };
        assert_eq!(
            serde_json::from_slice::<Preferences>(&serde_json::to_vec(&p).unwrap()).unwrap(),
            p
        );
    }
}
