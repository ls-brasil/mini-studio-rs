use fast_image_resize::{
    PixelType, ResizeAlg, ResizeOptions, Resizer,
    images::{Image, ImageRef},
};
use image::{Rgba, RgbaImage, imageops};
use std::sync::Arc;

pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 720;
pub const BACKGROUND: Rgba<u8> = Rgba([8, 11, 16, 255]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraPosition {
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageMode {
    Background,
    Overlay,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone)]
pub struct Scene {
    pub desktop: Option<Pixels>,
    pub camera: Option<Pixels>,
    pub image: Option<Pixels>,
    pub image_settings: ImageSettings,
    pub mirror: bool,
    pub camera_size: u32,
    pub camera_position: CameraPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Desktop,
    Image,
    Camera,
}

pub struct Layer {
    pub source: Option<Source>,
    pub rect: [f32; 4],
    pub mirror: bool,
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
        fn same(a: &Option<Pixels>, b: &Option<Pixels>) -> bool {
            match (a, b) {
                (Some(a), Some(b)) => {
                    a.width == b.width && a.height == b.height && Arc::ptr_eq(&a.rgba, &b.rgba)
                }
                (None, None) => true,
                _ => false,
            }
        }
        same(&self.desktop, &other.desktop)
            && same(&self.camera, &other.camera)
            && same(&self.image, &other.image)
            && self.image_settings == other.image_settings
            && self.mirror == other.mirror
            && self.camera_size == other.camera_size
            && self.camera_position == other.camera_position
    }
    pub fn pixels(&self, source: Source) -> Option<&Pixels> {
        match source {
            Source::Desktop => self.desktop.as_ref(),
            Source::Camera => self.camera.as_ref(),
            Source::Image => self.image.as_ref(),
        }
    }

    // Coordenadas da saída são quantizadas uma vez, antes de escalar a prévia.
    pub fn layers(&self) -> Vec<Layer> {
        let full = [0.0, 0.0, WIDTH as f32, HEIGHT as f32];
        let mut layers = vec![Layer {
            source: None,
            rect: full,
            mirror: false,
            color: BACKGROUND,
        }];
        let visible_image = self.image.as_ref().filter(|_| self.image_settings.visible);
        let image_layer = || {
            visible_image.map(|pixels| {
                let s = self.image_settings;
                let scale = s.size.clamp(5, 100) as f32 / 100.0;
                let [_, _, w, h] = fit_rect(
                    pixels.width,
                    pixels.height,
                    [0.0, 0.0, full[2] * scale, full[3] * scale],
                );
                Layer {
                    source: Some(Source::Image),
                    rect: [
                        (full[2] - w) * s.x.clamp(0.0, 1.0),
                        (full[3] - h) * s.y.clamp(0.0, 1.0),
                        w,
                        h,
                    ]
                    .map(f32::round),
                    mirror: false,
                    color: Rgba([255; 4]),
                }
            })
        };
        if self.image_settings.mode == ImageMode::Background {
            layers.extend(image_layer());
        }
        if let Some(p) = &self.desktop {
            layers.push(Layer {
                source: Some(Source::Desktop),
                rect: fit_rect(p.width, p.height, full).map(f32::round),
                mirror: false,
                color: Rgba([255; 4]),
            });
        }
        if self.image_settings.mode == ImageMode::Overlay {
            layers.extend(image_layer());
        }
        if let Some(p) = &self.camera {
            let pip = self.desktop.is_some() || visible_image.is_some();
            let area = if pip {
                camera_rect(self.camera_size, self.camera_position, full).map(f32::round)
            } else {
                full
            };
            if pip {
                layers.push(Layer {
                    source: None,
                    rect: [area[0] - 2.0, area[1] - 2.0, area[2] + 4.0, area[3] + 4.0],
                    mirror: false,
                    color: Rgba([255; 4]),
                });
                layers.push(Layer {
                    source: None,
                    rect: area,
                    mirror: false,
                    color: BACKGROUND,
                });
            }
            layers.push(Layer {
                source: Some(Source::Camera),
                rect: fit_rect(p.width, p.height, area).map(f32::round),
                mirror: self.mirror,
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
                    let alpha = u32::from(pixel[3]);
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
    fn solid(color: [u8; 4]) -> Pixels {
        Pixels {
            width: 1,
            height: 1,
            rgba: Arc::from(color),
        }
    }
    fn empty() -> Scene {
        Scene {
            desktop: None,
            camera: None,
            image: None,
            image_settings: ImageSettings::default(),
            mirror: false,
            camera_size: 25,
            camera_position: CameraPosition::BottomRight,
        }
    }
    #[test]
    fn empty_output_is_opaque_background() {
        let result = empty().compose().unwrap();
        assert_eq!(result.dimensions(), (WIDTH, HEIGHT));
        assert!(result.pixels().all(|p| *p == BACKGROUND));
    }
    #[test]
    fn image_order_visibility_and_alpha() {
        let mut scene = empty();
        scene.desktop = Some(solid([0, 0, 255, 255]));
        scene.image = Some(solid([255, 0, 0, 128]));
        assert_eq!(
            scene.compose().unwrap().get_pixel(640, 360).0,
            [0, 0, 255, 255]
        );
        scene.image_settings.mode = ImageMode::Overlay;
        let p = scene.compose().unwrap().get_pixel(640, 360).0;
        assert!(
            (127..=128).contains(&p[0]) && (126..=128).contains(&p[2]) && p[3] == 255,
            "pixel: {p:?}"
        );
        scene.image_settings.visible = false;
        assert_eq!(
            scene.compose().unwrap().get_pixel(640, 360).0,
            [0, 0, 255, 255]
        );
    }
    #[test]
    fn camera_mirror_and_image_position_share_layout() {
        let mut scene = empty();
        scene.camera = Some(Pixels {
            width: 2,
            height: 1,
            rgba: Arc::from([255, 0, 0, 255, 0, 0, 255, 255]),
        });
        scene.mirror = true;
        assert_eq!(
            scene.compose().unwrap().get_pixel(100, 360).0,
            [0, 0, 255, 255]
        );
        scene.image = Some(solid([255; 4]));
        scene.image_settings.size = 25;
        scene.image_settings.x = 1.0;
        scene.image_settings.y = 0.0;
        let layers = scene.layers();
        let image = layers
            .iter()
            .find(|l| l.source == Some(Source::Image))
            .unwrap();
        assert_eq!(image.rect, [1100.0, 0.0, 180.0, 180.0]);
        assert_eq!(layers.last().unwrap().source, Some(Source::Camera));
    }
}
