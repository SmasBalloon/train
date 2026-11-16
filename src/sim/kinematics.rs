//! Marche d'un train entre deux gares : accélération, palier, freinage.
//!
//! Le profil est calculé une fois au départ et se lit ensuite en fonction du
//! temps écoulé : la position ne dépend pas du pas de simulation.

use serde::{Deserialize, Serialize};

/// Profil de vitesse en trapèze (ou en triangle si la distance est trop
/// courte pour atteindre la vitesse visée).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub distance_km: f64,
    /// Vitesse de pointe réellement atteinte, en km/h.
    pub peak_kmh: f64,
    /// Accélération et décélération, en m/s².
    pub accel: f64,
    pub decel: f64,
}

impl Profile {
    /// Profil pour parcourir `distance_km` sans dépasser `max_kmh`.
    pub fn new(distance_km: f64, max_kmh: f64, accel: f64, decel: f64) -> Profile {
        let distance = distance_km.max(0.0) * 1000.0;
        let (accel, decel) = (accel.max(1e-4), decel.max(1e-4));
        let mut v = max_kmh.max(0.1) / 3.6;
        // Distance trop courte : on freine avant d'avoir atteint la vitesse visée.
        let reachable = (2.0 * distance * accel * decel / (accel + decel)).sqrt();
        if reachable < v {
            v = reachable.max(1e-3);
        }
        Profile {
            distance_km: distance / 1000.0,
            peak_kmh: v * 3.6,
            accel,
            decel,
        }
    }

    /// Durées des trois phases, en secondes : accélération, palier, freinage.
    fn phases(&self) -> (f64, f64, f64) {
        let v = self.peak_kmh / 3.6;
        let (t_acc, t_dec) = (v / self.accel, v / self.decel);
        let cruise_m =
            self.distance_km * 1000.0 - v * v / (2.0 * self.accel) - v * v / (2.0 * self.decel);
        (t_acc, (cruise_m / v).max(0.0), t_dec)
    }

    pub fn duration_s(&self) -> f64 {
        let (a, c, d) = self.phases();
        a + c + d
    }

    /// Distance parcourue (km) et vitesse (km/h) après `t` secondes.
    pub fn at(&self, t: f64) -> (f64, f64) {
        let v = self.peak_kmh / 3.6;
        let (t_acc, t_cruise, t_dec) = self.phases();
        let t = t.max(0.0);
        let (metres, speed) = if t < t_acc {
            (self.accel * t * t / 2.0, self.accel * t)
        } else if t < t_acc + t_cruise {
            (v * v / (2.0 * self.accel) + v * (t - t_acc), v)
        } else if t < t_acc + t_cruise + t_dec {
            let tau = t - t_acc - t_cruise;
            let before = v * v / (2.0 * self.accel) + v * t_cruise;
            (
                before + v * tau - self.decel * tau * tau / 2.0,
                v - self.decel * tau,
            )
        } else {
            (self.distance_km * 1000.0, 0.0)
        };
        (
            (metres / 1000.0).min(self.distance_km),
            speed.max(0.0) * 3.6,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trapezoid_matches_hand_computation() {
        // 10 km, 72 km/h (20 m/s), 0,5 m/s² à l'accélération, 1 m/s² au freinage.
        let p = Profile::new(10.0, 72.0, 0.5, 1.0);
        assert_eq!(p.peak_kmh, 72.0);
        // 40 s sur 400 m, puis 9400 m à 20 m/s (470 s), puis 20 s sur 200 m.
        assert!((p.duration_s() - 530.0).abs() < 1e-9);
        let near = |got: (f64, f64), want: (f64, f64)| {
            (got.0 - want.0).abs() < 1e-9 && (got.1 - want.1).abs() < 1e-9
        };
        assert!(near(p.at(0.0), (0.0, 0.0)));
        assert!(near(p.at(20.0), (0.1, 36.0)));
        assert!(near(p.at(40.0), (0.4, 72.0)));
        assert!(near(p.at(275.0), (5.1, 72.0)));
        assert!(near(p.at(520.0), (9.95, 36.0)));
        assert!(near(p.at(530.0), (10.0, 0.0)));
        assert!(near(p.at(9999.0), (10.0, 0.0)));
    }

    #[test]
    fn short_hop_never_reaches_top_speed() {
        // 1 km à 0,5 m/s² dans les deux sens : pointe à √(1000 × 0,5) ≈ 22,4 m/s.
        let p = Profile::new(1.0, 300.0, 0.5, 0.5);
        assert!((p.peak_kmh - 500f64.sqrt() * 3.6).abs() < 1e-9);
        assert!((p.duration_s() - 2.0 * 500f64.sqrt() / 0.5).abs() < 1e-9);
        let (half, speed) = p.at(p.duration_s() / 2.0);
        assert!((half - 0.5).abs() < 1e-9 && (speed - p.peak_kmh).abs() < 1e-9);
    }

    #[test]
    fn better_brakes_and_more_power_shorten_the_trip() {
        let base = Profile::new(19.0, 120.0, 0.1, 0.12).duration_s();
        assert!(Profile::new(19.0, 120.0, 0.1, 0.5).duration_s() < base);
        assert!(Profile::new(19.0, 120.0, 0.3, 0.12).duration_s() < base);
        assert!(Profile::new(19.0, 60.0, 0.1, 0.12).duration_s() > base);
    }

    #[test]
    fn position_never_goes_backwards_or_past_the_end() {
        let p = Profile::new(19.0, 145.0, 0.08, 0.4);
        let mut last = 0.0;
        for i in 0..=2000 {
            let (km, speed) = p.at(i as f64 * p.duration_s() / 1900.0);
            assert!(km >= last - 1e-12 && km <= 19.0 && speed >= 0.0 && speed <= p.peak_kmh + 1e-9);
            last = km;
        }
        assert_eq!(last, 19.0);
    }

    #[test]
    fn degenerate_inputs_do_not_produce_nan() {
        for p in [
            Profile::new(0.0, 100.0, 0.5, 0.5),
            Profile::new(5.0, 0.0, 0.0, 0.0),
            Profile::new(-3.0, -1.0, -1.0, -1.0),
        ] {
            assert!(
                p.duration_s().is_finite() && p.at(10.0).0.is_finite() && p.at(10.0).1.is_finite(),
                "{p:?}"
            );
        }
    }
}
