//! Transformación mundo (mm) ↔ pantalla (px lógicos), con rotación de previsualización.

use egui::{Pos2, Vec2};
use kurbo::{Point, Rect};

pub const ZOOM_MIN: f64 = 0.2;
pub const ZOOM_MAX: f64 = 80.0;

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    /// Píxeles por mm.
    pub scale: f64,
    /// Posición en pantalla del origen del mundo (0,0 mm).
    pub origin: Pos2,
    /// Rotación de la previsualización en grados (0, 90, 180, 270), alrededor del origen del mundo.
    pub rotation: u16,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport {
            scale: 2.0,
            origin: Pos2::new(80.0, 80.0),
            rotation: 0,
        }
    }
}

impl Viewport {
    /// (cos, sin) exactos para múltiplos de 90°.
    fn trig(self) -> (f64, f64) {
        match self.rotation % 360 {
            90 => (0.0, 1.0),
            180 => (-1.0, 0.0),
            270 => (0.0, -1.0),
            _ => (1.0, 0.0),
        }
    }

    /// Punto del mundo girado por la previsualización (sigue en mm).
    pub fn rotate_world(self, p: Point) -> Point {
        let (c, s) = self.trig();
        Point::new(p.x * c - p.y * s, p.x * s + p.y * c)
    }

    fn unrotate_world(self, p: Point) -> Point {
        let (c, s) = self.trig();
        Point::new(p.x * c + p.y * s, -p.x * s + p.y * c)
    }

    pub fn to_screen(self, p: Point) -> Pos2 {
        let r = self.rotate_world(p);
        Pos2::new(
            self.origin.x + (r.x * self.scale) as f32,
            self.origin.y + (r.y * self.scale) as f32,
        )
    }

    pub fn to_world(self, p: Pos2) -> Point {
        let r = Point::new(
            (p.x - self.origin.x) as f64 / self.scale,
            (p.y - self.origin.y) as f64 / self.scale,
        );
        self.unrotate_world(r)
    }

    /// Caja en pantalla de un rectángulo del mundo (sigue siendo axial porque la rotación es múltiplo de 90°).
    pub fn rect_to_screen(self, r: Rect) -> egui::Rect {
        egui::Rect::from_two_pos(
            self.to_screen(Point::new(r.x0, r.y0)),
            self.to_screen(Point::new(r.x1, r.y1)),
        )
    }

    /// Zoom manteniendo fijo el punto de pantalla `anchor`.
    pub fn zoom_at(&mut self, factor: f64, anchor: Pos2) {
        let scale = (self.scale * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let k = (scale / self.scale) as f32;
        self.origin = anchor + (self.origin - anchor) * k;
        self.scale = scale;
    }

    pub fn pan(&mut self, delta: Vec2) {
        self.origin += delta;
    }

    /// Ajusta `world` (mm) dentro de `screen` con un margen en px, respetando `rotation`.
    pub fn fit_rotated(world: Rect, screen: egui::Rect, padding: f32, rotation: u16) -> Viewport {
        let probe = Viewport {
            scale: 1.0,
            origin: Pos2::ZERO,
            rotation,
        };
        let a = probe.rotate_world(Point::new(world.x0, world.y0));
        let b = probe.rotate_world(Point::new(world.x1, world.y1));
        let rotated = Rect::from_points(a, b);
        if rotated.width() <= 0.0 || rotated.height() <= 0.0 || screen.width() <= 0.0 {
            return Viewport {
                rotation,
                ..Viewport::default()
            };
        }
        let sx = (screen.width() - 2.0 * padding) as f64 / rotated.width();
        let sy = (screen.height() - 2.0 * padding) as f64 / rotated.height();
        let scale = sx.min(sy).clamp(ZOOM_MIN, ZOOM_MAX);
        let ox = screen.min.x + (screen.width() as f64 - rotated.width() * scale) as f32 / 2.0
            - (rotated.x0 * scale) as f32;
        let oy = screen.min.y + (screen.height() as f64 - rotated.height() * scale) as f32 / 2.0
            - (rotated.y0 * scale) as f32;
        Viewport {
            scale,
            origin: Pos2::new(ox, oy),
            rotation,
        }
    }

    pub fn fit(world: Rect, screen: egui::Rect, padding: f32) -> Viewport {
        Viewport::fit_rotated(world, screen, padding, 0)
    }

    /// Eje del mundo que corre a lo largo de la regla horizontal (`true` = x) y su signo,
    /// y lo mismo para la vertical.
    pub fn ruler_axes(self) -> ((bool, f64), (bool, f64)) {
        match self.rotation % 360 {
            90 => ((false, -1.0), (true, 1.0)),
            180 => ((true, -1.0), (false, -1.0)),
            270 => ((false, 1.0), (true, -1.0)),
            _ => ((true, 1.0), (false, 1.0)),
        }
    }
}
