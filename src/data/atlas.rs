//! Fond de carte : contours des pays, frontières et fleuves réels, issus
//! de Natural Earth (domaine public) et projetés dans le repère de la carte.
//!
//! Repère de la carte : x vers l'est, y vers le sud, une unité vaut 10 km, et
//! Darlington (berceau de la première ligne) est à l'origine. La projection est
//! équirectangulaire, calée sur le 50e parallèle : simple, et fidèle à quelques
//! pour cent près sur l'Europe.

use anyhow::{Context, Result, bail};

/// Origine de la carte : Darlington.
const ORIGIN: [f64; 2] = [-1.5534, 54.5235];
/// Parallèle où les distances est-ouest sont exactes.
const REFERENCE_LAT: f64 = 50.0;
const KM_PER_DEGREE: f64 = 111.32;
const KM_PER_UNIT: f64 = 10.0;

/// Point de la carte pour une longitude et une latitude (en degrés).
pub fn project([lon, lat]: [f64; 2]) -> [f64; 2] {
    let k = KM_PER_DEGREE / KM_PER_UNIT;
    [
        (lon - ORIGIN[0]) * REFERENCE_LAT.to_radians().cos() * k,
        (ORIGIN[1] - lat) * k,
    ]
}

/// Vrai pour des coordonnées géographiques plausibles.
pub fn valid_lonlat([lon, lat]: [f64; 2]) -> bool {
    (-180.0..=180.0).contains(&lon) && (-85.0..=85.0).contains(&lat)
}

/// Contour (anneau fermé) d'un pays, ou d'une de ses îles.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// Code du pays chez Natural Earth (ADM0_A3 : GBR, FRA…).
    pub code: String,
    pub ring: Vec<[f64; 2]>,
    /// Boîte englobante : x min, y min, x max, y max.
    pub bbox: [f64; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct River {
    pub name: String,
    pub path: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Atlas {
    pub regions: Vec<Region>,
    pub borders: Vec<Vec<[f64; 2]>>,
    pub rivers: Vec<River>,
}

fn bbox(points: &[[f64; 2]]) -> [f64; 4] {
    points.iter().fold(
        [f64::MAX, f64::MAX, f64::MIN, f64::MIN],
        |[x0, y0, x1, y1], p| [x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1])],
    )
}

impl Atlas {
    /// Lit le format de `data/world.txt` : une ligne par tracé, « type, étiquette,
    /// points longitude,latitude », séparés par des tabulations.
    pub fn parse(text: &str, file: &str) -> Result<Atlas> {
        let mut atlas = Atlas::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let at = || format!("{file}, ligne {}", n + 1);
            let mut fields = line.splitn(3, '\t');
            let (Some(kind), Some(tag), Some(coords)) =
                (fields.next(), fields.next(), fields.next())
            else {
                bail!(
                    "{} : il faut trois champs séparés par des tabulations",
                    at()
                );
            };
            let mut points = Vec::new();
            for pair in coords.split_whitespace() {
                let (lon, lat) = pair
                    .split_once(',')
                    .with_context(|| format!("{} : point « {pair} » illisible", at()))?;
                let lonlat = [
                    lon.parse()
                        .with_context(|| format!("{} : « {pair} »", at()))?,
                    lat.parse()
                        .with_context(|| format!("{} : « {pair} »", at()))?,
                ];
                if !valid_lonlat(lonlat) {
                    bail!("{} : coordonnées hors du globe « {pair} »", at());
                }
                points.push(project(lonlat));
            }
            let needed = if kind == "C" { 3 } else { 2 };
            if points.len() < needed {
                bail!("{} : pas assez de points", at());
            }
            match kind {
                "C" => atlas.regions.push(Region {
                    code: tag.to_string(),
                    bbox: bbox(&points),
                    ring: points,
                }),
                "B" => atlas.borders.push(points),
                "R" => atlas.rivers.push(River {
                    name: tag.to_string(),
                    path: points,
                }),
                other => bail!("{} : type de tracé « {other} » inconnu (C, B ou R)", at()),
            }
        }
        Ok(atlas)
    }

    /// Codes des pays présents dans le fond de carte.
    pub fn has_code(&self, code: &str) -> bool {
        self.regions.iter().any(|r| r.code == code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_keeps_darlington_at_the_origin_and_ten_km_per_unit() {
        let origin = project(ORIGIN);
        assert!(origin[0].abs() < 1e-9 && origin[1].abs() < 1e-9);
        // Un degré de latitude plus au sud : 11,1 unités vers le bas.
        let south = project([ORIGIN[0], ORIGIN[1] - 1.0]);
        assert!((south[1] - 11.132).abs() < 1e-9);
        // Londres est au sud-sud-est de Darlington, Paris plus au sud encore.
        let london = project([-0.1276, 51.5074]);
        let paris = project([2.3522, 48.8566]);
        assert!(london[0] > 0.0 && london[1] > 30.0);
        assert!(paris[1] > london[1] && paris[0] > london[0]);
        // Londres-Paris : environ 340 km à vol d'oiseau.
        let km = (paris[0] - london[0]).hypot(paris[1] - london[1]) * KM_PER_UNIT;
        assert!((320.0..360.0).contains(&km), "{km}");
    }

    #[test]
    fn the_world_file_parses_and_errors_are_located() {
        let atlas = Atlas::parse(
            "# commentaire\nC\tGBR\t0,50 1,50 1,51\nB\t-\t1,50 1,51\nR\tSeine\t2,49 3,48\n",
            "w",
        )
        .unwrap();
        assert_eq!(
            (atlas.regions.len(), atlas.borders.len(), atlas.rivers.len()),
            (1, 1, 1)
        );
        assert!(atlas.has_code("GBR") && !atlas.has_code("FRA"));
        assert_eq!(atlas.rivers[0].name, "Seine");
        let b = atlas.regions[0].bbox;
        assert!(b[0] < b[2] && b[1] < b[3]);

        for (bad, needle) in [
            ("C\tGBR\t0,50 1,50", "pas assez de points"),
            ("X\t-\t0,50 1,50", "inconnu"),
            ("B\t-\t0,500 1,50", "hors du globe"),
            ("B\t-\t0;50 1,50", "illisible"),
            ("B 0,50", "trois champs"),
        ] {
            let err = Atlas::parse(bad, "w").unwrap_err().to_string();
            assert!(
                err.contains(needle) && err.contains("ligne 1"),
                "{bad} : {err}"
            );
        }
    }
}
