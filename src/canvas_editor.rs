use crate::composition::{HEIGHT, Source, Transform, WIDTH};
use eframe::egui::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    Move,
    Resize(i8, i8),
}
impl Handle {
    pub fn cursor(self) -> CursorIcon {
        match self {
            Self::Move => CursorIcon::Grab,
            Self::Resize(0, _) => CursorIcon::ResizeVertical,
            Self::Resize(_, 0) => CursorIcon::ResizeHorizontal,
            Self::Resize(x, y) if x == y => CursorIcon::ResizeNwSe,
            _ => CursorIcon::ResizeNeSw,
        }
    }
}
pub fn handles(rect: Rect) -> [(Handle, Pos2); 8] {
    [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ]
    .map(|(x, y)| {
        (
            Handle::Resize(x, y),
            rect.center() + rect.size() * vec2(x as f32, y as f32) * 0.5,
        )
    })
}
pub fn hit_handle(rect: Rect, point: Pos2) -> Option<Handle> {
    handles(rect)
        .into_iter()
        .filter(|(_, p)| p.distance(point) <= 9.0)
        .min_by(|(_, a), (_, b)| a.distance(point).total_cmp(&b.distance(point)))
        .map(|(h, _)| h)
}

#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub id: Source,
    pub original: Transform,
    pub start: Transform,
    pub handle: Handle,
    pub crop: bool,
}
impl Drag {
    pub fn apply(self, delta: Vec2, free: bool) -> Transform {
        let mut t = self.start;
        match self.handle {
            Handle::Move => {
                t.x += delta.x;
                t.y += delta.y;
            }
            Handle::Resize(hx, hy) if self.crop => {
                // Trabalha no espaço visível; o recorte é salvo antes do espelhamento.
                let (left, right) = if t.mirror { (2, 0) } else { (0, 2) };
                for (axis, h, d, first, last) in
                    [(0, hx, delta.x, left, right), (1, hy, delta.y, 1, 3)]
                {
                    if h == 0 {
                        continue;
                    }
                    let (pos, size) = if axis == 0 {
                        (&mut t.x, &mut t.width)
                    } else {
                        (&mut t.y, &mut t.height)
                    };
                    let full_size = *size / (1.0 - t.crop[first] - t.crop[last]);
                    let index = if h < 0 { first } else { last };
                    let opposite = if h < 0 { last } else { first };
                    let old = t.crop[index];
                    t.crop[index] =
                        (old - h as f32 * d / full_size).clamp(0.0, 0.99 - t.crop[opposite]);
                    let change = (t.crop[index] - old) * full_size;
                    *size -= change;
                    if h < 0 {
                        *pos += change;
                    }
                }
            }
            Handle::Resize(hx, hy) => {
                let dw = delta.x * hx as f32;
                let dh = delta.y * hy as f32;
                let (w, h) = if free {
                    (
                        (t.width + dw).clamp(0.001, 4.0),
                        (t.height + dh).clamp(0.001, 4.0),
                    )
                } else {
                    let sx = dw / t.width;
                    let sy = dh / t.height;
                    let scale = 1.0
                        + if hx == 0 {
                            sy
                        } else if hy == 0 || sx.abs() > sy.abs() {
                            sx
                        } else {
                            sy
                        };
                    let scale = scale.clamp(
                        (0.001 / t.width).max(0.001 / t.height),
                        (4.0 / t.width).min(4.0 / t.height),
                    );
                    (t.width * scale, t.height * scale)
                };
                t.x += (t.width - w)
                    * if hx < 0 {
                        1.0
                    } else if hx == 0 {
                        0.5
                    } else {
                        0.0
                    };
                t.y += (t.height - h)
                    * if hy < 0 {
                        1.0
                    } else if hy == 0 {
                        0.5
                    } else {
                        0.0
                    };
                t.width = w;
                t.height = h;
            }
        }
        // Only an explicit free resize opts into distortion. Moving, cropping and
        // proportional resizing must keep following changes in capture dimensions.
        t.stretch = self.original.stretch
            || (free && !self.crop && matches!(self.handle, Handle::Resize(_, _)));
        t.sanitize();
        t
    }
}

// Guias em coordenadas normalizadas; tolerância constante em pixels da prévia.
pub fn snap_move(t: &mut Transform, targets: &[[f32; 4]], threshold: Vec2) {
    for axis in 0..2 {
        let (pos, size, tolerance) = if axis == 0 {
            (&mut t.x, t.width, threshold.x)
        } else {
            (&mut t.y, t.height, threshold.y)
        };
        let anchors = [*pos, *pos + size * 0.5, *pos + size];
        let mut best = tolerance;
        let mut correction = 0.0;
        for target in targets {
            for line in [
                target[axis],
                target[axis] + target[axis + 2] * 0.5,
                target[axis] + target[axis + 2],
            ] {
                for anchor in anchors {
                    let d = line - anchor;
                    if d.abs() < best {
                        best = d.abs();
                        correction = d;
                    }
                }
            }
        }
        *pos += correction;
    }
}

pub fn paint_selection(
    painter: &Painter,
    area: Rect,
    canvas: Rect,
    transform: Transform,
    locked: bool,
) {
    let color = if locked {
        Color32::GRAY
    } else if transform.crop.iter().any(|c| *c > 0.0) {
        Color32::from_rgb(65, 215, 145)
    } else {
        Color32::from_rgb(255, 92, 105)
    };
    painter.rect_stroke(area, 0, Stroke::new(1.5_f32, color), StrokeKind::Inside);
    if locked {
        return;
    }
    for (_, point) in handles(area) {
        painter.rect_filled(Rect::from_center_size(point, vec2(7.0, 7.0)), 0, color);
    }
    let center = area.center();
    let gaps = [
        (
            pos2(canvas.left(), center.y),
            area.left_center(),
            (area.left() - canvas.left()) / canvas.width() * WIDTH as f32,
        ),
        (
            area.right_center(),
            pos2(canvas.right(), center.y),
            (canvas.right() - area.right()) / canvas.width() * WIDTH as f32,
        ),
        (
            pos2(center.x, canvas.top()),
            area.center_top(),
            (area.top() - canvas.top()) / canvas.height() * HEIGHT as f32,
        ),
        (
            area.center_bottom(),
            pos2(center.x, canvas.bottom()),
            (canvas.bottom() - area.bottom()) / canvas.height() * HEIGHT as f32,
        ),
    ];
    for (a, b, pixels) in gaps {
        if pixels <= 1.0 || a.distance(b) < 38.0 || !canvas.contains(a) || !canvas.contains(b) {
            continue;
        }
        painter.line_segment([a, b], Stroke::new(1.0_f32, color.gamma_multiply(0.65)));
        let center = a.lerp(b, 0.5);
        let text = format!("{pixels:.0} px");
        let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), Color32::WHITE);
        let label = Rect::from_center_size(center, galley.size() + vec2(8.0, 4.0));
        painter.rect_filled(label, 3, Color32::from_black_alpha(220));
        painter.galley(label.min + vec2(4.0, 2.0), galley, Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn drag(handle: Handle, crop: bool) -> Drag {
        let start = Transform {
            x: 0.2,
            y: 0.2,
            width: 0.4,
            height: 0.4,
            stretch: true,
            ..Default::default()
        };
        Drag {
            id: 1,
            original: start,
            start,
            handle,
            crop,
        }
    }
    #[test]
    fn normal_edits_preserve_fit_and_only_free_resize_enables_stretch() {
        for handle in [Handle::Move, Handle::Resize(1, 1)] {
            for crop in [false, true] {
                let mut d = drag(handle, crop);
                d.original.stretch = false;
                assert!(!d.apply(vec2(0.1, 0.05), false).stretch);
                assert_eq!(
                    d.apply(vec2(0.1, 0.05), true).stretch,
                    !crop && matches!(handle, Handle::Resize(_, _))
                );
            }
        }
    }
    #[test]
    fn every_handle_keeps_opposite_anchor_and_aspect() {
        for (handle, _) in handles(Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0))) {
            let Handle::Resize(x, y) = handle else {
                unreachable!()
            };
            let d = drag(handle, false);
            let t = d.apply(vec2(x as f32 * 0.1, y as f32 * 0.1), false);
            assert!((t.width - 0.5).abs() < 0.0001 && (t.height - 0.5).abs() < 0.0001);
            let anchor = |p: f32, s: f32, h: i8| p + s * (1.0 - h as f32) / 2.0;
            assert!((anchor(t.x, t.width, x) - anchor(d.start.x, d.start.width, x)).abs() < 0.0001);
            assert!(
                (anchor(t.y, t.height, y) - anchor(d.start.y, d.start.height, y)).abs() < 0.0001
            );
        }
    }
    #[test]
    fn crop_and_uncrop_keep_scale_and_handle_mirror() {
        for mirror in [false, true] {
            let mut d = drag(Handle::Resize(-1, 0), true);
            d.start.mirror = mirror;
            let t = d.apply(vec2(0.1, 0.0), false);
            assert!((t.x - 0.3).abs() < 0.0001);
            assert!((t.crop[if mirror { 2 } else { 0 }] - 0.25).abs() < 0.0001);
            d.start = t;
            let restored = d.apply(vec2(-0.1, 0.0), false);
            assert!(restored.crop.iter().all(|c| c.abs() < 0.0001));
            assert!((restored.width - 0.4).abs() < 0.0001);
        }
    }
    #[test]
    fn stretch_offscreen_and_snapping() {
        let d = drag(Handle::Resize(1, 1), false);
        let t = d.apply(vec2(0.2, 0.1), true);
        assert!((t.width - 0.6).abs() < 0.0001 && (t.height - 0.5).abs() < 0.0001);
        let d = drag(Handle::Move, false);
        assert!(d.apply(vec2(-0.4, 0.0), false).x < 0.0);
        let mut t = Transform {
            x: 0.004,
            y: 0.297,
            width: 0.4,
            height: 0.4,
            ..Default::default()
        };
        snap_move(&mut t, &[[0.0, 0.0, 1.0, 1.0]], vec2(0.01, 0.01));
        assert_eq!(t.x, 0.0);
        assert!((t.y - 0.3).abs() < 0.0001);
    }
}
