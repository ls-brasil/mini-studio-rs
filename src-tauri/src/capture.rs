use std::{
    io::Cursor,
    time::{SystemTime, UNIX_EPOCH},
};

use image::{
    DynamicImage, ImageDecoder, RgbaImage,
    codecs::{jpeg::JpegEncoder, png::PngDecoder},
};
use serde::{Deserialize, Serialize};
use xcap::{Monitor, Window};

const PREVIEW_WIDTH: u32 = 1280;
const PREVIEW_HEIGHT: u32 = 720;
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

fn grab_frame(kind: SourceKind, id: u32) -> Result<RgbaImage, String> {
    match kind {
        SourceKind::Screen => {
            let monitor = Monitor::all()
                .map_err(|e| format!("Não foi possível acessar as telas: {e}"))?
                .into_iter()
                .find(|monitor| monitor.id().ok() == Some(id))
                .ok_or("A tela foi desconectada. Selecione outra fonte.")?;
            monitor
                .capture_image()
                .map_err(|e| format!("Não foi possível capturar a tela: {e}"))
        }
        SourceKind::Window => {
            let window = Window::all()
                .map_err(|e| format!("Não foi possível acessar as janelas: {e}"))?
                .into_iter()
                .find(|window| window.id().ok() == Some(id))
                .ok_or("A janela foi fechada ou está indisponível. Selecione outra fonte.")?;
            if window.pid().ok() == Some(std::process::id()) {
                return Err("Selecione uma janela de outro aplicativo.".into());
            }
            if window
                .is_minimized()
                .map_err(|e| format!("A janela está indisponível: {e}"))?
            {
                return Err(
                    "A janela foi minimizada. Restaure-a e selecione a fonte novamente.".into(),
                );
            }
            window
                .capture_image()
                .map_err(|e| format!("Não foi possível capturar a janela: {e}"))
        }
    }
}

fn encode_preview(image: RgbaImage) -> Result<Vec<u8>, String> {
    if image.width() == 0 || image.height() == 0 {
        return Err("A fonte retornou uma imagem vazia.".into());
    }
    let image = DynamicImage::ImageRgba8(image);
    let preview = if image.width() > PREVIEW_WIDTH || image.height() > PREVIEW_HEIGHT {
        image.thumbnail(PREVIEW_WIDTH, PREVIEW_HEIGHT).to_rgb8()
    } else {
        image.to_rgb8()
    };
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 82)
        .encode_image(&preview)
        .map_err(|e| format!("Não foi possível preparar a prévia: {e}"))?;
    Ok(bytes)
}

#[tauri::command]
pub async fn list_capture_sources(kind: SourceKind) -> Result<Vec<CaptureSource>, String> {
    tauri::async_runtime::spawn_blocking(move || list_sources(kind))
        .await
        .map_err(|e| format!("Falha ao listar as fontes: {e}"))?
}

#[tauri::command]
pub async fn capture_frame(kind: SourceKind, id: u32) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        encode_preview(grab_frame(kind, id)?).map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|e| format!("Falha na captura: {e}"))?
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
    fn preview_preserves_portrait_aspect_and_is_jpeg() {
        let bytes = encode_preview(RgbaImage::new(1080, 1920)).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (405, 720));
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::Jpeg
        );
    }

    #[test]
    fn preview_does_not_upscale_and_rejects_empty_frames() {
        let bytes = encode_preview(RgbaImage::new(320, 180)).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (320, 180));
        assert!(encode_preview(RgbaImage::new(0, 0)).is_err());
    }

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

    #[test]
    #[ignore = "Requer sessão gráfica interativa com um monitor conectado"]
    fn captures_a_real_monitor() {
        let started = std::time::Instant::now();
        let sources = list_sources(SourceKind::Screen).unwrap();
        assert!(!sources.is_empty());
        let frame = grab_frame(SourceKind::Screen, sources[0].id).unwrap();
        assert!(frame.width() > 0 && frame.height() > 0);
        let bytes = encode_preview(frame).unwrap();
        assert!(image::load_from_memory(&bytes).is_ok());
        eprintln!("Captura e validação do monitor: {:?}", started.elapsed());
    }

    #[test]
    #[ignore = "Requer sessão gráfica interativa com uma janela aberta"]
    fn captures_a_real_window() {
        let started = std::time::Instant::now();
        let sources = list_sources(SourceKind::Window).unwrap();
        assert!(!sources.is_empty());
        let frame = grab_frame(SourceKind::Window, sources[0].id).unwrap();
        assert!(frame.width() > 0 && frame.height() > 0);
        let bytes = encode_preview(frame).unwrap();
        assert!(image::load_from_memory(&bytes).is_ok());
        eprintln!("Captura e validação da janela: {:?}", started.elapsed());
    }
}
