//! The logic of the EQ tabs, without a window (UX-20, BEH-29, BEH-30): the names of the types, which
//! boxes a type has, the response curve that is drawn, and what ALL OFF and RESET write. The EQ page
//! in [`crate::mixeq`] only draws what these return.

use labgruppen::{Channel, EQ_BANDS, EqField, Param, Value, XoverField, XoverFilter};

/// One band as the amp holds it; `kind` is a portable label of [`labgruppen::EQ_TYPES`].
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub on: bool,
    pub kind: String,
    pub freq: f64,
    pub gain: f64,
    pub q: f64,
}

/// A high-pass or low-pass filter of an output; `kind` is a label of [`labgruppen::XOVER_TYPES`].
#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    pub on: bool,
    pub kind: String,
    pub freq: f64,
}

/// The Q RESET sets, the mockup's.
pub const RESET_Q: f64 = 1.4;

/// The bands of a channel whose values are not read yet: the mockup's layout, all off.
pub fn default_band(band: u8) -> Band {
    const FREQS: [f64; 10] = [32.0, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
    let kind = match band {
        0 => "low_shelf",
        b if b + 1 == EQ_BANDS => "high_shelf",
        _ => "bell",
    };
    Band { on: false, kind: kind.into(), freq: FREQS[usize::from(band) % 10], gain: 0.0, q: RESET_Q }
}

/// The name of an EQ type in the list.
pub fn eq_label(kind: &str) -> String {
    match kind {
        "bell" => "Bell".into(),
        "notch" => "Notch".into(),
        "allpass" => "All-pass".into(),
        "low_shelf" => "Low shelf".into(),
        "high_shelf" => "High shelf".into(),
        "band_pass" => "Band-pass".into(),
        "high_pass" => "High-pass".into(),
        "low_pass" => "Low-pass".into(),
        other => other.into(),
    }
}

/// The name of a filter shape in the list: `linkwitz_riley_24` is "Linkwitz-Riley 24 dB/oct".
pub fn filter_label(kind: &str) -> String {
    let Some((family, slope)) = kind.rsplit_once('_') else { return kind.into() };
    let family = match family {
        "butterworth" => "Butterworth",
        "bessel" => "Bessel",
        "linkwitz_riley" => "Linkwitz-Riley",
        other => other,
    };
    format!("{family} {slope} dB/oct")
}

/// Whether a type has a gain (the shelves and the bell).
pub fn has_gain(kind: &str) -> bool {
    matches!(kind, "bell" | "low_shelf" | "high_shelf")
}

/// Whether a type has a Q (not the shelves nor the high and low pass).
pub fn has_q(kind: &str) -> bool {
    matches!(kind, "bell" | "notch" | "allpass" | "band_pass")
}

/// The graph spans 20 Hz to 20 kHz across 1000 units, and ±15 dB around its middle line.
pub const GRAPH_DB: f64 = 15.0;

/// Where `freq` is across the graph, 0 to 1 (log scale, 3 decades from 20 Hz).
pub fn x_of(freq: f64) -> f64 {
    ((freq / 20.0).log10() / 3.0).clamp(0.0, 1.0)
}

/// Where a gain is down the graph, 0 to 1 (0 dB in the middle, ±15 dB nearly at the edges).
pub fn y_of(db: f64) -> f64 {
    (150.0 - db.clamp(-GRAPH_DB, GRAPH_DB) / GRAPH_DB * 140.0) / 300.0
}

/// The sample rate the curve is computed at, the amps'.
const RATE: f64 = 48_000.0;

/// The magnitude in dB, at `f`, of a biquad of the Audio EQ Cookbook (R. Bristow-Johnson).
fn biquad_db(b: [f64; 3], a: [f64; 3], f: f64) -> f64 {
    let w = std::f64::consts::TAU * f / RATE;
    let mag = |c: [f64; 3]| {
        let re = c[0] + c[1] * w.cos() + c[2] * (2.0 * w).cos();
        let im = c[1] * w.sin() + c[2] * (2.0 * w).sin();
        re * re + im * im
    };
    10.0 * (mag(b) / mag(a)).log10()
}

/// What one band does at `f`, in dB. A band that is off does nothing.
pub fn band_db(band: &Band, f: f64) -> f64 {
    if !band.on || band.freq <= 0.0 {
        return 0.0;
    }
    let w0 = std::f64::consts::TAU * band.freq.min(RATE * 0.49) / RATE;
    let (sin, cos) = w0.sin_cos();
    let q = if has_q(&band.kind) || matches!(band.kind.as_str(), "high_pass" | "low_pass") {
        band.q.max(0.05)
    } else {
        std::f64::consts::FRAC_1_SQRT_2
    };
    let alpha = sin / (2.0 * q);
    let a = 10f64.powf(band.gain / 40.0);
    let (b, den) = match band.kind.as_str() {
        "bell" => {
            ([1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a], [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a])
        }
        "notch" => ([1.0, -2.0 * cos, 1.0], [1.0 + alpha, -2.0 * cos, 1.0 - alpha]),
        "band_pass" => ([alpha, 0.0, -alpha], [1.0 + alpha, -2.0 * cos, 1.0 - alpha]),
        "low_pass" => {
            ([(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0], [1.0 + alpha, -2.0 * cos, 1.0 - alpha])
        }
        "high_pass" => {
            ([(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0], [1.0 + alpha, -2.0 * cos, 1.0 - alpha])
        }
        "low_shelf" | "high_shelf" => {
            let s = 2.0 * a.sqrt() * alpha;
            let (p, m) = (a + 1.0, a - 1.0);
            if band.kind == "low_shelf" {
                (
                    [a * (p - m * cos + s), 2.0 * a * (m - p * cos), a * (p - m * cos - s)],
                    [p + m * cos + s, -2.0 * (m + p * cos), p + m * cos - s],
                )
            } else {
                (
                    [a * (p + m * cos + s), -2.0 * a * (m + p * cos), a * (p + m * cos - s)],
                    [p - m * cos + s, 2.0 * (m - p * cos), p - m * cos - s],
                )
            }
        }
        // an all-pass moves the phase only
        _ => return 0.0,
    };
    biquad_db(b, den, f)
}

/// What a high-pass (`high`) or low-pass filter does at `f`, in dB. Butterworth and Linkwitz-Riley
/// have their closed forms; Bessel is its polynomial, normalised to -3 dB at the frequency.
pub fn filter_db(filter: &Filter, high: bool, f: f64) -> f64 {
    if !filter.on || filter.freq <= 0.0 || f <= 0.0 {
        return 0.0;
    }
    let x = if high { filter.freq / f } else { f / filter.freq };
    let Some((family, slope)) = filter.kind.rsplit_once('_') else { return 0.0 };
    let order = slope.parse::<i32>().unwrap_or(12) / 6;
    match family {
        "linkwitz_riley" => -20.0 * (1.0 + x.powi(order)).log10(),
        "bessel" => bessel_db(order, x),
        _ => -10.0 * (1.0 + x.powi(2 * order)).log10(),
    }
}

/// A Bessel low-pass of `order` at `x` times its -3 dB frequency.
fn bessel_db(order: i32, x: f64) -> f64 {
    // the reverse Bessel polynomials, lowest power first, and the factor that puts -3 dB at x = 1
    let (poly, k): (&[f64], f64) = match order {
        1 => (&[1.0, 1.0], 1.0),
        2 => (&[3.0, 3.0, 1.0], 1.3617),
        3 => (&[15.0, 15.0, 6.0, 1.0], 1.7557),
        _ => (&[105.0, 105.0, 45.0, 10.0, 1.0], 2.1139),
    };
    // the polynomial at s = j * x * k
    let w = x * k;
    let (mut re, mut im) = (0.0, 0.0);
    for (n, c) in poly.iter().enumerate() {
        let p = c * w.powi(n as i32);
        match n % 4 {
            0 => re += p,
            1 => im += p,
            2 => re -= p,
            _ => im -= p,
        }
    }
    -10.0 * ((re * re + im * im) / (poly[0] * poly[0])).log10()
}

/// The curve of a channel: the sum of its bands and filters, as an SVG path over 0..1000 × 0..300,
/// and the same path closed on the 0 dB line for the fill.
pub fn curve(bands: &[Band], filters: &[(Filter, bool)]) -> (String, String) {
    const POINTS: usize = 140;
    let points: Vec<String> = (0..=POINTS)
        .map(|i| {
            let f = 20.0 * 10f64.powf(3.0 * i as f64 / POINTS as f64);
            let db = bands.iter().map(|b| band_db(b, f)).sum::<f64>()
                + filters.iter().map(|(flt, high)| filter_db(flt, *high, f)).sum::<f64>();
            let db = if db.is_finite() { db } else { -GRAPH_DB };
            format!("{:.1} {:.1}", i as f64 / POINTS as f64 * 1000.0, y_of(db) * 300.0)
        })
        .collect();
    let line = format!("M{}", points.join(" L "));
    let fill = format!("{line} L 1000 150 L 0 150 Z");
    (line, fill)
}

/// The distance sound travels in `ms` at 20 °C, as the DELAY row says it.
pub fn air(ms: f64) -> String {
    format!("\u{2248} {:.2} m of air at 20 \u{b0}C", ms * 0.343)
}

/// ALL OFF (UX-20): every band of `ch` switched off.
pub fn all_off(ch: Channel) -> Vec<(Param, Value)> {
    (0..EQ_BANDS).map(|band| (Param::Eq { ch, band, field: EqField::Enabled }, Value::Bool(false))).collect()
}

/// RESET (UX-20): every band off, at 0 dB and a Q of 1.4; type and frequency stay.
pub fn reset(ch: Channel) -> Vec<(Param, Value)> {
    let mut writes = all_off(ch);
    for band in 0..EQ_BANDS {
        writes.push((Param::Eq { ch, band, field: EqField::Gain }, Value::Float(0.0)));
        writes.push((Param::Eq { ch, band, field: EqField::Q }, Value::Float(RESET_Q)));
    }
    writes
}

/// The parameter of a field of a filter of an output.
pub fn filter_param(ch: Channel, high: bool, field: XoverField) -> Param {
    Param::Xover { ch, filter: if high { XoverFilter::Highpass } else { XoverFilter::Lowpass }, field }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band(kind: &str, freq: f64, gain: f64, q: f64) -> Band {
        Band { on: true, kind: kind.into(), freq, gain, q }
    }

    fn close(a: f64, b: f64, by: f64) -> bool {
        (a - b).abs() <= by
    }

    // BEH-29, UX-20: a bell peaks at its gain at its frequency and is flat far from it; off, it is flat
    #[test]
    fn a_bell_peaks_at_its_frequency() {
        let b = band("bell", 1000.0, 6.0, 1.4);
        assert!(close(band_db(&b, 1000.0), 6.0, 0.01), "{}", band_db(&b, 1000.0));
        assert!(band_db(&b, 50.0).abs() < 0.1);
        assert!(close(band_db(&band("bell", 1000.0, -6.0, 1.4), 1000.0), -6.0, 0.01));
        assert_eq!(band_db(&Band { on: false, ..b }, 1000.0), 0.0);
    }

    // BEH-29: the shelves reach their gain on their side and are half way at their frequency
    #[test]
    fn the_shelves_lift_their_side() {
        let low = band("low_shelf", 100.0, 6.0, 1.4);
        assert!(close(band_db(&low, 20.0), 6.0, 0.3), "{}", band_db(&low, 20.0));
        assert!(close(band_db(&low, 100.0), 3.0, 0.1));
        assert!(band_db(&low, 5000.0).abs() < 0.1);
        let high = band("high_shelf", 5000.0, -6.0, 1.4);
        assert!(close(band_db(&high, 19000.0), -6.0, 0.5), "{}", band_db(&high, 19000.0));
        assert!(band_db(&high, 100.0).abs() < 0.1);
    }

    // BEH-29: a notch cuts deep at its frequency, the filters cut their side, an all-pass is flat
    #[test]
    fn the_other_types() {
        assert!(band_db(&band("notch", 1000.0, 0.0, 4.0), 1000.0) < -40.0);
        assert!(band_db(&band("high_pass", 100.0, 0.0, 0.707), 25.0) < -20.0);
        assert!(band_db(&band("low_pass", 1000.0, 0.0, 0.707), 100.0).abs() < 0.1);
        assert!(close(band_db(&band("band_pass", 1000.0, 0.0, 1.0), 1000.0), 0.0, 0.01));
        assert_eq!(band_db(&band("allpass", 1000.0, 0.0, 1.0), 300.0), 0.0);
    }

    // BEH-30: a high-pass is -3 dB at its frequency (-6 dB for Linkwitz-Riley), and steeper with
    // its slope
    #[test]
    fn the_output_filters() {
        let f = |kind: &str| Filter { on: true, kind: kind.into(), freq: 100.0 };
        assert!(close(filter_db(&f("butterworth_24"), true, 100.0), -3.01, 0.01));
        assert!(close(filter_db(&f("linkwitz_riley_24"), true, 100.0), -6.02, 0.01));
        assert!(
            close(filter_db(&f("bessel_12"), true, 100.0), -3.0, 0.05),
            "{}",
            filter_db(&f("bessel_12"), true, 100.0)
        );
        assert!(close(filter_db(&f("bessel_24"), false, 100.0), -3.0, 0.05));
        // an octave below: 24 dB/oct Butterworth is about -24 dB, 12 dB/oct about -12 dB
        assert!(close(filter_db(&f("butterworth_24"), true, 50.0), -24.1, 0.2));
        assert!(close(filter_db(&f("butterworth_12"), true, 50.0), -12.3, 0.2));
        assert!(filter_db(&f("butterworth_24"), true, 1000.0).abs() < 0.01, "flat above");
        assert!(filter_db(&f("butterworth_24"), false, 10.0).abs() < 0.01, "a low-pass is flat below");
        assert_eq!(filter_db(&Filter { on: false, ..f("butterworth_24") }, true, 50.0), 0.0);
    }

    // UX-20: the graph is 20 Hz to 20 kHz, ±15 dB; the curve has its points and its fill is closed
    #[test]
    fn the_curve_spans_the_graph() {
        assert_eq!(x_of(20.0), 0.0);
        assert!(close(x_of(20000.0), 1.0, 1e-12));
        assert!(close(x_of(632.46), 0.5, 1e-4));
        assert_eq!(y_of(0.0), 0.5);
        assert_eq!(y_of(30.0), y_of(15.0), "the curve stays on the graph");
        let (line, fill) = curve(&[], &[]);
        assert!(line.starts_with("M0.0 150.0 L "), "{line}");
        assert_eq!(line.matches(" L ").count(), 140);
        assert!(fill.ends_with("L 1000 150 L 0 150 Z"));
        let (lifted, _) = curve(&[band("bell", 1000.0, 12.0, 1.4)], &[]);
        assert_ne!(lifted, line);
    }

    // UX-20: which boxes a type has, and the names of the lists
    #[test]
    fn types_and_their_boxes() {
        assert!(has_gain("bell") && has_gain("low_shelf") && !has_gain("notch"));
        assert!(has_q("bell") && has_q("notch") && !has_q("low_shelf") && !has_q("high_pass"));
        assert_eq!(eq_label("low_shelf"), "Low shelf");
        assert_eq!(filter_label("linkwitz_riley_24"), "Linkwitz-Riley 24 dB/oct");
        assert_eq!(filter_label("bessel_6"), "Bessel 6 dB/oct");
        for kind in labgruppen::EQ_TYPES {
            assert_ne!(eq_label(kind), *kind, "every type has a name: {kind}");
        }
        assert_eq!(air(10.0), "\u{2248} 3.43 m of air at 20 \u{b0}C");
    }

    // UX-20: ALL OFF switches the ten bands off; RESET also flattens them, keeping type and frequency
    #[test]
    fn all_off_and_reset() {
        let ch = Channel::Output(0);
        let off = all_off(ch);
        assert_eq!(off.len(), 10);
        assert!(
            off.iter()
                .all(|(p, v)| matches!(p, Param::Eq { field: EqField::Enabled, .. })
                    && *v == Value::Bool(false))
        );
        let reset = reset(ch);
        assert_eq!(reset.len(), 30);
        assert!(reset.contains(&(Param::Eq { ch, band: 9, field: EqField::Gain }, Value::Float(0.0))));
        assert!(reset.contains(&(Param::Eq { ch, band: 3, field: EqField::Q }, Value::Float(1.4))));
        assert!(
            !reset.iter().any(|(p, _)| matches!(p, Param::Eq { field: EqField::Freq | EqField::Type, .. }))
        );
    }
}
