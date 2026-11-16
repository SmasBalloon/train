//! Caméra de la carte et géométrie des tracés.
//!
//! La carte est dessinée en « sous-pixels » : chaque cellule du terminal en
//! contient 2 en largeur et 4 en hauteur (les points d'un caractère braille),
//! ce qui les rend à peu près carrés.

pub const MIN_SCALE: f64 = 0.5;
pub const MAX_SCALE: f64 = 400.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Point du monde affiché au centre de l'écran.
    pub cx: f64,
    pub cy: f64,
    /// Sous-pixels par unité de carte.
    pub scale: f64,
}

impl Camera {
    pub fn to_px(self, world: [f64; 2], size: (f64, f64)) -> (f64, f64) {
        (
            (world[0] - self.cx) * self.scale + size.0 / 2.0,
            (world[1] - self.cy) * self.scale + size.1 / 2.0,
        )
    }

    pub fn to_world(self, px: (f64, f64), size: (f64, f64)) -> [f64; 2] {
        [
            (px.0 - size.0 / 2.0) / self.scale + self.cx,
            (px.1 - size.1 / 2.0) / self.scale + self.cy,
        ]
    }

    /// Zoome en gardant fixe le point du monde situé sous `anchor`.
    pub fn zoom_at(&mut self, factor: f64, anchor: (f64, f64), size: (f64, f64)) {
        let before = self.to_world(anchor, size);
        self.scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        let after = self.to_world(anchor, size);
        self.cx += before[0] - after[0];
        self.cy += before[1] - after[1];
    }

    pub fn pan_px(&mut self, dx: f64, dy: f64) {
        self.cx += dx / self.scale;
        self.cy += dy / self.scale;
    }

    /// Cadre l'ensemble des points en laissant une marge autour.
    pub fn fit(points: &[[f64; 2]], size: (f64, f64)) -> Camera {
        if points.is_empty() {
            return Camera {
                cx: 0.0,
                cy: 0.0,
                scale: 8.0,
            };
        }
        let (mut min, mut max) = (points[0], points[0]);
        for p in points {
            for i in 0..2 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let width = (max[0] - min[0]).max(1.0);
        let height = (max[1] - min[1]).max(1.0);
        let scale = (size.0 * 0.7 / width)
            .min(size.1 * 0.5 / height)
            .clamp(MIN_SCALE, MAX_SCALE);
        Camera {
            cx: (min[0] + max[0]) / 2.0,
            cy: (min[1] + max[1]) / 2.0,
            scale,
        }
    }
}

/// Ligne brisée paramétrée par la distance parcourue.
#[derive(Debug, Clone)]
pub struct Polyline {
    pts: Vec<[f64; 2]>,
    /// Distance cumulée jusqu'à chaque sommet.
    cum: Vec<f64>,
}

impl Polyline {
    pub fn new(pts: Vec<[f64; 2]>) -> Polyline {
        let mut cum = Vec::with_capacity(pts.len());
        let mut total = 0.0;
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                total += (p[0] - pts[i - 1][0]).hypot(p[1] - pts[i - 1][1]);
            }
            cum.push(total);
        }
        Polyline { pts, cum }
    }

    pub fn points(&self) -> &[[f64; 2]] {
        &self.pts
    }

    pub fn length(&self) -> f64 {
        self.cum.last().copied().unwrap_or(0.0)
    }

    /// Position et direction (vecteur unitaire) à la distance `d` du départ.
    pub fn at(&self, d: f64) -> ([f64; 2], [f64; 2]) {
        match self.pts.len() {
            0 => return ([0.0, 0.0], [1.0, 0.0]),
            1 => return (self.pts[0], [1.0, 0.0]),
            _ => {}
        }
        let d = d.clamp(0.0, self.length());
        // Premier segment non dégénéré qui contient `d`.
        let mut i = 1;
        while i < self.pts.len() - 1 && (self.cum[i] < d || self.cum[i] <= self.cum[i - 1]) {
            i += 1;
        }
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        let seg = self.cum[i] - self.cum[i - 1];
        if seg <= 0.0 {
            return (b, [1.0, 0.0]);
        }
        let t = (d - self.cum[i - 1]) / seg;
        (
            [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
            [(b[0] - a[0]) / seg, (b[1] - a[1]) / seg],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: (f64, f64) = (200.0, 120.0);

    #[test]
    fn world_and_screen_coordinates_round_trip() {
        let cam = Camera {
            cx: 3.0,
            cy: -2.0,
            scale: 7.5,
        };
        let px = cam.to_px([10.0, 4.0], SIZE);
        let back = cam.to_world(px, SIZE);
        assert!((back[0] - 10.0).abs() < 1e-9 && (back[1] - 4.0).abs() < 1e-9);
        assert_eq!(cam.to_px([3.0, -2.0], SIZE), (100.0, 60.0));
    }

    #[test]
    fn zoom_keeps_the_anchor_point_fixed() {
        let mut cam = Camera {
            cx: 0.0,
            cy: 0.0,
            scale: 4.0,
        };
        let anchor = (150.0, 30.0);
        let before = cam.to_world(anchor, SIZE);
        cam.zoom_at(2.5, anchor, SIZE);
        let after = cam.to_world(anchor, SIZE);
        assert_eq!(cam.scale, 10.0);
        assert!((before[0] - after[0]).abs() < 1e-9 && (before[1] - after[1]).abs() < 1e-9);
        cam.zoom_at(1e9, anchor, SIZE);
        assert_eq!(cam.scale, MAX_SCALE);
    }

    #[test]
    fn fit_shows_every_point() {
        let pts = [[0.0, 0.0], [26.0, 8.0], [-6.0, 12.0]];
        let cam = Camera::fit(&pts, SIZE);
        for p in pts {
            let (x, y) = cam.to_px(p, SIZE);
            assert!(
                (0.0..SIZE.0).contains(&x) && (0.0..SIZE.1).contains(&y),
                "{p:?} hors écran"
            );
        }
    }

    #[test]
    fn polyline_walks_along_its_segments() {
        let line = Polyline::new(vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0]]);
        assert_eq!(line.length(), 15.0);
        assert_eq!(line.at(0.0), ([0.0, 0.0], [1.0, 0.0]));
        assert_eq!(line.at(4.0), ([4.0, 0.0], [1.0, 0.0]));
        assert_eq!(line.at(12.0), ([10.0, 2.0], [0.0, 1.0]));
        assert_eq!(line.at(99.0).0, [10.0, 5.0]);
        assert_eq!(line.at(-3.0).0, [0.0, 0.0]);
    }
}
