use crate::composition::Pixels;
use image::{ImageDecoder, ImageFormat, ImageReader, Limits};
use std::{
    fs::File,
    io::{BufReader, Cursor, Read},
    path::Path,
};

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PIXELS: u64 = 16_000_000;
const MAX_SIDE: u32 = 8192;

pub struct LoadedImage {
    pub path: std::path::PathBuf,
    pub name: String,
    pub pixels: Pixels,
}

pub fn load(path: &Path) -> Result<LoadedImage, String> {
    let file = File::open(path).map_err(|e| format!("Não foi possível abrir a imagem: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE_BYTES {
        return Err("A imagem excede o limite de arquivo de 32 MiB.".into());
    }
    // Leitura limitada mesmo se o arquivo crescer depois da consulta de tamanho.
    let mut bytes = Vec::new();
    BufReader::new(file)
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("A imagem excede o limite de arquivo de 32 MiB.".into());
    }
    let pixels = decode(&bytes)?;
    Ok(LoadedImage {
        path: path.canonicalize().map_err(|e| e.to_string())?,
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        pixels,
    })
}

fn decode(bytes: &[u8]) -> Result<Pixels, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if !matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)) {
        return Err("Formato não aceito. Escolha um arquivo PNG ou JPEG.".into());
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| format!("Imagem inválida ou acima dos limites: {e}"))?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("A imagem deve ter até 16 milhões de pixels e 8192 pixels por lado.".into());
    }
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("Não foi possível decodificar a imagem: {e}"))?;
    image.apply_orientation(orientation);
    // Reduz uma única vez, com alfa pré-multiplicado durante a filtragem.
    let rgba = image.to_rgba8();
    let [_, _, w, h] =
        crate::composition::fit_rect(rgba.width(), rgba.height(), [0.0, 0.0, 1280.0, 720.0]);
    let (width, height) = (w.round().max(1.0) as u32, h.round().max(1.0) as u32);
    if rgba.width() <= 1280 && rgba.height() <= 720 {
        return Ok(Pixels {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw().into(),
        });
    }
    use fast_image_resize::{
        PixelType, ResizeOptions, Resizer,
        images::{Image, ImageRef},
    };
    let source = ImageRef::new(rgba.width(), rgba.height(), rgba.as_raw(), PixelType::U8x4)
        .map_err(|e| e.to_string())?;
    let mut target = Image::new(width, height, PixelType::U8x4);
    Resizer::new()
        .resize(&source, &mut target, &ResizeOptions::new())
        .map_err(|e| e.to_string())?;
    Ok(Pixels {
        width,
        height,
        rgba: target.into_vec().into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_alpha_and_jpeg_are_loaded() {
        for format in [ImageFormat::Png, ImageFormat::Jpeg] {
            let mut encoded = Cursor::new(Vec::new());
            let original = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                3,
                5,
                image::Rgba([20, 40, 60, 128]),
            ));
            let original = if format == ImageFormat::Jpeg {
                image::DynamicImage::ImageRgb8(original.to_rgb8())
            } else {
                original
            };
            original.write_to(&mut encoded, format).unwrap();
            let p = decode(encoded.get_ref()).unwrap();
            assert_eq!((p.width, p.height), (3, 5));
            assert_eq!(
                p.rgba[3],
                if format == ImageFormat::Png { 128 } else { 255 }
            );
        }
    }
    #[test]
    fn invalid_and_oversized_images_are_rejected() {
        assert!(decode(b"not an image").is_err());
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(8193, 1)
            .write_to(&mut encoded, ImageFormat::Png)
            .unwrap();
        assert!(decode(encoded.get_ref()).is_err());
    }
}
