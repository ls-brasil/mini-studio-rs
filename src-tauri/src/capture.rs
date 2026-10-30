use std::{
    io::Cursor,
    time::{SystemTime, UNIX_EPOCH},
};

use image::{DynamicImage, ImageDecoder, codecs::png::PngDecoder};
use serde::{Deserialize, Serialize};
use xcap::{Monitor, Window};

const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Screen,
    Window,
}

#[derive(Serialize)]
pub struct CaptureSource {
    id: u32,
    name: String,
    detail: String,
}

fn list_sources(kind: SourceKind) -> Result<Vec<CaptureSource>, String> {
    match kind {
        SourceKind::Screen => {
            let monitors =
                Monitor::all().map_err(|e| format!("Não foi possível listar as telas: {e}"))?;
            Ok(monitors
                .iter()
                .enumerate()
                .filter_map(|(index, monitor)| {
                    let width = monitor.width().ok()?;
                    let height = monitor.height().ok()?;
                    Some(CaptureSource {
                        id: monitor.id().ok()?,
                        name: format!(
                            "Tela {} — {}",
                            index + 1,
                            monitor.friendly_name().unwrap_or_else(|_| "Monitor".into())
                        ),
                        detail: format!(
                            "{width} × {height}{}",
                            if monitor.is_primary().unwrap_or(false) {
                                " · principal"
                            } else {
                                ""
                            }
                        ),
                    })
                })
                .collect())
        }
        SourceKind::Window => {
            let windows =
                Window::all().map_err(|e| format!("Não foi possível listar as janelas: {e}"))?;
            Ok(windows
                .iter()
                .filter_map(|window| {
                    let name = window.title().ok()?;
                    if name.trim().is_empty()
                        || window.pid().ok()? == std::process::id()
                        || window.is_minimized().unwrap_or(true)
                    {
                        return None;
                    }
                    let width = window.width().ok()?;
                    let height = window.height().ok()?;
                    if width == 0 || height == 0 {
                        return None;
                    }
                    Some(CaptureSource {
                        id: window.id().ok()?,
                        name,
                        detail: format!(
                            "{} · {width} × {height}",
                            window.app_name().unwrap_or_else(|_| "Aplicativo".into())
                        ),
                    })
                })
                .collect())
        }
    }
}

#[tauri::command]
pub async fn list_capture_sources(kind: SourceKind) -> Result<Vec<CaptureSource>, String> {
    tauri::async_runtime::spawn_blocking(move || list_sources(kind))
        .await
        .map_err(|e| format!("Falha ao listar as fontes: {e}"))?
}
fn validate_snapshot(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err("A imagem deve ser um PNG de até 16 MB.".into());
    }
    let decoder =
        PngDecoder::new(Cursor::new(bytes)).map_err(|_| "A imagem não é um PNG válido.")?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > 3840
        || height > 2160
        || decoder.total_bytes() > 32 * 1024 * 1024
    {
        return Err("A imagem excede o limite de 3840 × 2160 pixels.".into());
    }
    DynamicImage::from_decoder(decoder).map_err(|_| "O PNG está incompleto ou corrompido.")?;
    Ok(())
}

#[tauri::command]
pub async fn save_snapshot(bytes: Vec<u8>) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        validate_snapshot(&bytes)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let Some(mut path) = rfd::FileDialog::new()
            .set_title("Salvar imagem da composição")
            .set_file_name(format!("mini-studio-{stamp}.png"))
            .add_filter("Imagem PNG", &["png"])
            .save_file()
        else {
            return Ok(None);
        };
        if path.extension().is_none() {
            path.set_extension("png");
        }
        std::fs::write(&path, bytes)
            .map_err(|e| format!("Não foi possível salvar a imagem: {e}"))?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| format!("Falha ao salvar a imagem: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_accepts_png_and_rejects_invalid_payloads() {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(1280, 720)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        assert!(validate_snapshot(bytes.get_ref()).is_ok());
        assert!(validate_snapshot(b"not an image").is_err());
        assert!(validate_snapshot(&bytes.get_ref()[..40]).is_err());
        assert!(validate_snapshot(&vec![0; MAX_SNAPSHOT_BYTES + 1]).is_err());
    }
}
