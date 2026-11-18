//! Mise en forme des nombres à la française.

/// Entier avec espaces entre les milliers : `12 345`.
pub fn int(value: f64) -> String {
    let n = value.round() as i64;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// Somme d'argent en euros ; les très grands montants sont abrégés : `12,35 M€`.
pub fn money(value: f64) -> String {
    let abs = value.abs();
    let sign = if value < 0.0 { "-" } else { "" };
    if abs >= 1e9 {
        format!("{sign}{} Md€", dec2(abs / 1e9))
    } else if abs >= 1e6 {
        format!("{sign}{} M€", dec2(abs / 1e6))
    } else {
        format!("{} €", int(value.floor()))
    }
}

/// Quantité avec au plus une décimale, sans décimale inutile : `12`, `0,5`, `1 240`.
pub fn quantity(value: f64) -> String {
    if value.abs() >= 100.0 || (value - value.round()).abs() < 0.05 {
        int(value)
    } else {
        dec1(value)
    }
}

/// Une décimale, virgule décimale : `2,7`.
pub fn dec1(value: f64) -> String {
    // `+ 0.0` : un zéro négatif (somme vide) ne doit pas s'afficher « -0,0 ».
    format!("{:.1}", value + 0.0).replace('.', ",")
}

pub fn percent(ratio: f64) -> String {
    format!("{} %", (ratio * 100.0).round() as i64)
}

/// Deux décimales, pour les petits montants : `1,37`.
pub fn dec2(value: f64) -> String {
    format!("{:.2}", value + 0.0).replace('.', ",")
}

/// Durée réelle lisible : `45 s`, `2 min`, `1 min 30`, `3 h 12`.
pub fn duration(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    if s >= 3600 {
        return format!("{} h {:02}", s / 3600, s % 3600 / 60);
    }
    match (s / 60, s % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s:02}"),
    }
}

/// Jauge de `width` caractères remplie à `ratio`.
pub fn gauge(ratio: f64, width: usize, full: char, empty: char) -> String {
    let filled = ((ratio.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    let mut out = String::with_capacity(width * 3);
    out.extend(std::iter::repeat_n(full, filled));
    out.extend(std::iter::repeat_n(empty, width - filled));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_use_french_conventions() {
        assert_eq!(int(0.0), "0");
        assert_eq!(int(999.0), "999");
        assert_eq!(int(1240.0), "1 240");
        assert_eq!(int(1234567.0), "1 234 567");
        assert_eq!(int(-1500.0), "-1 500");
        assert_eq!(money(1240.9), "1 240 €");
        assert_eq!(money(999_999.0), "999 999 €");
        assert_eq!(money(12_345_678.0), "12,35 M€");
        assert_eq!(money(-2.5e9), "-2,50 Md€");
        assert_eq!(money(-5.0), "-5 €");
        assert_eq!(quantity(12.0), "12");
        assert_eq!(quantity(0.46), "0,5");
        assert_eq!(quantity(1240.4), "1 240");
        assert_eq!(dec1(2.74), "2,7");
        assert_eq!(dec1(-0.0), "0,0");
        assert_eq!(dec1(std::iter::empty::<f64>().sum()), "0,0");
        assert_eq!(percent(0.866), "87 %");
    }

    #[test]
    fn durations_and_gauges() {
        assert_eq!(duration(45.0), "45 s");
        assert_eq!(duration(120.0), "2 min");
        assert_eq!(duration(90.0), "1 min 30");
        assert_eq!(duration(3.0 * 3600.0 + 12.0 * 60.0 + 40.0), "3 h 12");
        assert_eq!(gauge(0.5, 4, '#', '-'), "##--");
        assert_eq!(gauge(7.0, 3, '#', '-'), "###");
    }
}
