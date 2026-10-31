use fast_image_resize::{
    PixelType, ResizeAlg, ResizeOptions, Resizer,
    images::{Image, ImageRef},
};
use image::{Rgba, RgbaImage, imageops};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 720;
pub const BACKGROUND: Rgba<u8> = Rgba([8, 11, 16, 255]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CameraPosition {
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageMode {
    Background,
    Overlay,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageSettings {
    pub visible: bool,
    pub mode: ImageMode,
    pub size: u32,
    pub x: f32,
    pub y: f32,
}
impl Default for ImageSettings {
    fn default() -> Self {
        Self {
            visible: true,
            mode: ImageMode::Background,
            size: 100,
            x: 0.5,
            y: 0.5,
        }
    }
}

#[derive(Clone)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub mirror: bool,
    pub opacity: u8,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            mirror: false,
            opacity: 255,
        }
    }
}
impl Transform {
    pub fn sanitize(&mut self) {
        self.x = self.x.clamp(0.0, 1.0);
        self.y = self.y.clamp(0.0, 1.0);
        self.width = self.width.clamp(0.01, 1.0);
        self.height = self.height.clamp(0.01, 1.0);
    }
    pub fn from_rect(rect: [f32; 4]) -> Self {
        Self {
            x: rect[0] / WIDTH as f32,
            y: rect[1] / HEIGHT as f32,
            width: rect[2] / WIDTH as f32,
            height: rect[3] / HEIGHT as f32,
            ..Default::default()
        }
    }
}

pub type Source = u64;

#[derive(Clone)]
pub struct SceneSource {
    pub id: Source,
    pub pixels: Pixels,
    pub transform: Transform,
}

#[derive(Clone, Default)]
pub struct Scene {
    // A primeira fonte é a camada de trás; a lista da interface mostra a ordem inversa.
    pub sources: Vec<SceneSource>,
}

pub struct Layer {
    pub source: Option<Source>,
    pub rect: [f32; 4],
    pub mirror: bool,
    pub opacity: u8,
    pub color: Rgba<u8>,
}

pub fn fit_rect(src_width: u32, src_height: u32, area: [f32; 4]) -> [f32; 4] {
    let [x, y, width, height] = area;
    if src_width == 0 || src_height == 0 || width <= 0.0 || height <= 0.0 {
        return [x, y, 0.0, 0.0];
    }
    let scale = (width / src_width as f32).min(height / src_height as f32);
    let w = src_width as f32 * scale;
    let h = src_height as f32 * scale;
    [x + (width - w) / 2.0, y + (height - h) / 2.0, w, h]
}

pub fn camera_rect(percent: u32, position: CameraPosition, area: [f32; 4]) -> [f32; 4] {
    let [x, y, aw, ah] = area;
    let w = aw * percent.clamp(15, 40) as f32 / 100.0;
    let h = w * 9.0 / 16.0;
    let padding = aw * 24.0 / WIDTH as f32;
    let left = matches!(
        position,
        CameraPosition::BottomLeft | CameraPosition::TopLeft
    );
    let top = matches!(position, CameraPosition::TopLeft | CameraPosition::TopRight);
    [
        if left {
            x + padding
        } else {
            x + aw - w - padding
        },
        if top {
            y + padding
        } else {
            y + ah - h - padding
        },
        w,
        h,
    ]
}

impl Scene {
    pub fn same_content(&self, other: &Self) -> bool {
        self.sources.len() == other.sources.len()
            && self.sources.iter().zip(&other.sources).all(|(a, b)| {
                a.id == b.id
                    && a.transform == b.transform
                    && a.pixels.width == b.pixels.width
                    && a.pixels.height == b.pixels.height
                    && Arc::ptr_eq(&a.pixels.rgba, &b.pixels.rgba)
            })
    }
    pub fn pixels(&self, source: Source) -> Option<&Pixels> {
        self.sources
            .iter()
            .find(|item| item.id == source)
            .map(|item| &item.pixels)
    }
    pub fn layers(&self) -> Vec<Layer> {
        let mut layers = vec![Layer {
            source: None,
            rect: [0.0, 0.0, WIDTH as f32, HEIGHT as f32],
            mirror: false,
            opacity: 255,
            color: BACKGROUND,
        }];
        for source in &self.sources {
            let t = source.transform;
            layers.push(Layer {
                source: Some(source.id),
                rect: fit_rect(
                    source.pixels.width,
                    source.pixels.height,
                    [
                        t.x * WIDTH as f32,
                        t.y * HEIGHT as f32,
                        t.width * WIDTH as f32,
                        t.height * HEIGHT as f32,
                    ],
                )
                .map(f32::round),
                mirror: t.mirror,
                opacity: t.opacity,
                color: Rgba([255; 4]),
            });
        }
        layers
    }

    pub fn compose(&self) -> Result<RgbaImage, String> {
        let mut canvas = RgbaImage::from_pixel(WIDTH, HEIGHT, BACKGROUND);
        let mut resizer = Resizer::new();
        for layer in self.layers().into_iter().skip(1) {
            let [x, y, w, h] = layer.rect.map(|n| n as u32);
            if let Some(source) = layer.source {
                let p = self.pixels(source).ok_or("Fonte ausente.")?;
                if p.width == 0
                    || p.height == 0
                    || (p.width as usize)
                        .checked_mul(p.height as usize)
                        .and_then(|n| n.checked_mul(4))
                        != Some(p.rgba.len())
                {
                    return Err("A fonte retornou um quadro RGBA inválido.".into());
                }
                let src = ImageRef::new(p.width, p.height, &p.rgba, PixelType::U8x4)
                    .map_err(|e| e.to_string())?;
                let mut dst = Image::new(w.max(1), h.max(1), PixelType::U8x4);
                resizer
                    .resize(
                        &src,
                        &mut dst,
                        &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(
                            fast_image_resize::FilterType::Bilinear,
                        )),
                    )
                    .map_err(|e| e.to_string())?;
                let mut image = RgbaImage::from_raw(w.max(1), h.max(1), dst.into_vec())
                    .ok_or("Quadro inválido.")?;
                if layer.mirror {
                    imageops::flip_horizontal_in_place(&mut image);
                }
                // Mistura inteira sobre fundo opaco evita perda de alfa por arredondamento.
                for (sx, sy, pixel) in image.enumerate_pixels() {
                    if x + sx >= WIDTH || y + sy >= HEIGHT {
                        continue;
                    }
                    let dst = canvas.get_pixel_mut(x + sx, y + sy);
                    let alpha = (u32::from(pixel[3]) * u32::from(layer.opacity) + 127) / 255;
                    for channel in 0..3 {
                        dst[channel] = ((u32::from(pixel[channel]) * alpha
                            + u32::from(dst[channel]) * (255 - alpha)
                            + 127)
                            / 255) as u8;
                    }
                    dst[3] = 255;
                }
            } else {
                for py in y..(y + h).min(HEIGHT) {
                    for px in x..(x + w).min(WIDTH) {
                        canvas.put_pixel(px, py, layer.color);
                    }
                }
            }
        }
        Ok(canvas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn solid(id: u64, color: [u8; 4]) -> SceneSource {
        SceneSource {
            id,
            pixels: Pixels {
                width: 16,
                height: 9,
                rgba: color.repeat(144).into(),
            },
            transform: Transform::default(),
        }
    }
    #[test]
    fn repeated_source_types_follow_order_alpha_and_opacity() {
        let mut scene = Scene {
            sources: vec![
                solid(1, [255, 0, 0, 255]),
                solid(2, [0, 255, 0, 128]),
                solid(3, [0, 0, 255, 255]),
            ],
        };
        scene.sources[2].transform.opacity = 128;
        let image = scene.compose().unwrap();
        assert_eq!(image.get_pixel(640, 360).0, [63, 64, 128, 255]);
        let before = scene.clone();
        scene.sources.swap(0, 2);
        assert!(!scene.same_content(&before));
        assert_eq!(
            scene.compose().unwrap().get_pixel(640, 360).0,
            [255, 0, 0, 255]
        );
        scene.sources.clear();
        assert_eq!(scene.compose().unwrap().get_pixel(640, 360), &BACKGROUND);
    }
    #[test]
    fn preview_geometry_matches_export_and_mirrors_each_source() {
        let mut source = SceneSource {
            id: 7,
            pixels: Pixels {
                width: 2,
                height: 1,
                rgba: vec![255, 0, 0, 255, 0, 0, 255, 255].into(),
            },
            transform: Transform {
                x: 0.5,
                y: 0.5,
                width: 0.5,
                height: 0.5,
                mirror: true,
                ..Default::default()
            },
        };
        let scene = Scene {
            sources: vec![source.clone()],
        };
        let layer = scene.layers().pop().unwrap();
        assert_eq!(layer.rect, [640.0, 380.0, 640.0, 320.0]);
        let image = scene.compose().unwrap();
        assert_eq!(image.get_pixel(650, 400).0, [0, 0, 255, 255]);
        assert_eq!(image.get_pixel(1270, 400).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(10, 10), &BACKGROUND);
        source.pixels.rgba = vec![0; 3].into();
        assert!(
            Scene {
                sources: vec![source]
            }
            .compose()
            .is_err()
        );
    }
}
