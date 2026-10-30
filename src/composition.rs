use image::{Rgba, RgbaImage, imageops};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const BACKGROUND: Rgba<u8> = Rgba([8, 11, 16, 255]);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CameraPosition {
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
}

pub fn fit_rect(src_width: u32, src_height: u32, area: [f32; 4]) -> [f32; 4] {
    let [x, y, width, height] = area;
    if src_width == 0 || src_height == 0 || width <= 0.0 || height <= 0.0 {
        return [x, y, 0.0, 0.0];
    }
    let scale = (width / src_width as f32).min(height / src_height as f32);
    let fitted_width = src_width as f32 * scale;
    let fitted_height = src_height as f32 * scale;
    [
        x + (width - fitted_width) / 2.0,
        y + (height - fitted_height) / 2.0,
        fitted_width,
        fitted_height,
    ]
}

pub fn camera_rect(percent: u32, position: CameraPosition, area: [f32; 4]) -> [f32; 4] {
    let [x, y, area_width, area_height] = area;
    let width = area_width * percent.clamp(15, 40) as f32 / 100.0;
    let height = width * 9.0 / 16.0;
    let padding = area_width * 24.0 / WIDTH as f32;
    let left = matches!(
        position,
        CameraPosition::BottomLeft | CameraPosition::TopLeft
    );
    let top = matches!(position, CameraPosition::TopLeft | CameraPosition::TopRight);
    [
        if left {
            x + padding
        } else {
            x + area_width - width - padding
        },
        if top {
            y + padding
        } else {
            y + area_height - height - padding
        },
        width,
        height,
    ]
}

fn source_image(width: u32, height: u32, pixels: &[u8]) -> Result<RgbaImage, String> {
    let expected = width
        .checked_mul(height)
        .and_then(|size| size.checked_mul(4));
    if width == 0 || height == 0 || expected.map(|size| size as usize) != Some(pixels.len()) {
        return Err("A fonte retornou um quadro RGBA inválido.".into());
    }
    RgbaImage::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| "Não foi possível preparar a imagem da fonte.".into())
}

fn draw_source(canvas: &mut RgbaImage, image: &RgbaImage, area: [f32; 4]) {
    let [x, y, width, height] = fit_rect(image.width(), image.height(), area);
    let resized = imageops::resize(
        image,
        width.round().max(1.0) as u32,
        height.round().max(1.0) as u32,
        imageops::FilterType::Triangle,
    );
    imageops::overlay(canvas, &resized, x.round() as i64, y.round() as i64);
}

fn camera_background(canvas: &mut RgbaImage, area: [f32; 4]) {
    let [x, y, width, height] = area.map(|value| value.round() as i32);
    for py in (y - 2).max(0)..(y + height + 2).min(HEIGHT as i32) {
        for px in (x - 2).max(0)..(x + width + 2).min(WIDTH as i32) {
            let border = px < x || px >= x + width || py < y || py >= y + height;
            canvas.put_pixel(
                px as u32,
                py as u32,
                if border {
                    Rgba([255, 255, 255, 255])
                } else {
                    BACKGROUND
                },
            );
        }
    }
}

pub fn compose_snapshot(
    base: Option<(u32, u32, &[u8])>,
    camera: Option<(u32, u32, &[u8])>,
    mirror: bool,
    size: u32,
    position: CameraPosition,
) -> Result<RgbaImage, String> {
    if base.is_none() && camera.is_none() {
        return Err("Adicione uma fonte antes de salvar a imagem.".into());
    }
    let mut canvas = RgbaImage::from_pixel(WIDTH, HEIGHT, BACKGROUND);
    let full = [0.0, 0.0, WIDTH as f32, HEIGHT as f32];
    if let Some((width, height, pixels)) = base {
        draw_source(&mut canvas, &source_image(width, height, pixels)?, full);
    }
    if let Some((width, height, pixels)) = camera {
        let mut image = source_image(width, height, pixels)?;
        if mirror {
            imageops::flip_horizontal_in_place(&mut image);
        }
        let area = if base.is_some() {
            camera_rect(size, position, full)
        } else {
            full
        };
        if base.is_some() {
            camera_background(&mut canvas, area);
        }
        draw_source(&mut canvas, &image, area);
    }
    Ok(canvas)
}
