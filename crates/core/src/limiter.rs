//! The output limiter in the four units of the vendor apps (BEH-5), the same on both families.
//!
//! Peak voltage and dBu are one quantity; the watts follow from the voltage and the load. So a
//! limiter is a threshold (as Vpk, or as dBu) at a load, and the other figures are derived. A change
//! of the load moves the watts only: the threshold stays where it is.

use labgruppen::limiter as math;

/// A unit the limiter threshold or load is shown and edited in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Vpk,
    Ohm,
    Watt,
    Dbu,
}

impl Unit {
    pub fn label(self) -> &'static str {
        match self {
            Unit::Vpk => "Vpk",
            Unit::Ohm => "Ohm",
            Unit::Watt => "W",
            Unit::Dbu => "dBu",
        }
    }
}

/// The four figures of an output limiter, all consistent: the volts and watts are the ones a threshold
/// of `dbu` gives at `ohm`. A dBV stored by an IPD is turned into dBu before this (see [`math::DBV_TO_DBU`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limiter {
    pub vpk: f64,
    pub ohm: f64,
    pub watt: f64,
    pub dbu: f64,
}

impl Limiter {
    pub fn from_dbu(dbu: f64, ohm: f64) -> Limiter {
        let vpk = math::vpk_from_dbu(dbu);
        Limiter { vpk, ohm, watt: math::watt(vpk, ohm), dbu }
    }

    pub fn from_vpk(vpk: f64, ohm: f64) -> Limiter {
        Limiter { vpk, ohm, watt: math::watt(vpk, ohm), dbu: math::dbu(vpk) }
    }

    pub fn from_watt(watt: f64, ohm: f64) -> Limiter {
        Limiter::from_vpk(math::vpk_from_watt(watt, ohm), ohm)
    }

    /// The same threshold at another load: only the watts change.
    pub fn with_ohm(self, ohm: f64) -> Limiter {
        Limiter { ohm, watt: math::watt(self.vpk, ohm), ..self }
    }

    /// The limiter after the user entered `value` in `unit`. The threshold is edited at the current
    /// load; entering the load keeps the threshold.
    pub fn edit(self, unit: Unit, value: f64) -> Limiter {
        match unit {
            Unit::Vpk => Limiter::from_vpk(value, self.ohm),
            Unit::Dbu => Limiter::from_dbu(value, self.ohm),
            Unit::Watt => Limiter::from_watt(value, self.ohm),
            Unit::Ohm => self.with_ohm(value),
        }
    }

    pub fn in_unit(self, unit: Unit) -> f64 {
        match unit {
            Unit::Vpk => self.vpk,
            Unit::Ohm => self.ohm,
            Unit::Watt => self.watt,
            Unit::Dbu => self.dbu,
        }
    }
}

/// A dBu that is a float rounding away from an edge of the range is the edge.
pub(crate) fn snap(dbu: f64, (min, max): (f64, f64)) -> f64 {
    const TOLERANCE: f64 = 1e-9;
    if (dbu - min).abs() < TOLERANCE {
        min
    } else if (dbu - max).abs() < TOLERANCE {
        max
    } else {
        dbu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tolerance: f64) {
        assert!((a - b).abs() <= tolerance, "{a} is not within {tolerance} of {b}");
    }

    // BEH-5: the figures of docs/ipx-protocol.md
    #[test]
    fn the_four_figures_follow_from_any_one_and_the_load() {
        let from_vpk = Limiter::from_vpk(69.0, 4.0);
        close(from_vpk.watt, 595.125, 1e-9);
        close(from_vpk.dbu, 35.9851, 1e-4);

        let from_watt = Limiter::from_watt(450.0, 4.0);
        close(from_watt.vpk, 60.0, 1e-9);
        close(from_watt.dbu, 34.7712, 1e-4);

        for l in [from_vpk, from_watt, Limiter::from_dbu(30.0, 8.0)] {
            let again = Limiter::from_dbu(l.dbu, l.ohm);
            close(again.vpk, l.vpk, 1e-9);
            close(again.watt, l.watt, 1e-9);
            let again = Limiter::from_watt(l.watt, l.ohm);
            close(again.dbu, l.dbu, 1e-9);
        }
    }

    // BEH-5: dBu = 10 * log10(W * ohm / 0.6), whatever the load
    #[test]
    fn dbu_is_watts_times_ohm_over_point_six() {
        for ohm in [2.0, 4.0, 8.0, 16.0] {
            let l = Limiter::from_watt(300.0, ohm);
            close(l.dbu, 10.0 * (300.0 * ohm / 0.6f64).log10(), 1e-9);
        }
    }

    // BEH-5: the load scales the watts and nothing else
    #[test]
    fn a_load_edit_keeps_the_threshold() {
        let l = Limiter::from_dbu(33.0, 4.0);
        let m = l.edit(Unit::Ohm, 8.0);
        assert_eq!((m.vpk, m.dbu, m.ohm), (l.vpk, l.dbu, 8.0));
        close(m.watt, l.watt / 2.0, 1e-9);
        assert_eq!(l.with_ohm(4.0), l);
    }

    // a threshold edit keeps the load
    #[test]
    fn a_threshold_edit_keeps_the_load_and_sets_the_unit_asked_for() {
        let l = Limiter::from_dbu(33.0, 8.0);
        for (unit, value) in [(Unit::Vpk, 40.0), (Unit::Watt, 100.0), (Unit::Dbu, 25.0)] {
            let m = l.edit(unit, value);
            assert_eq!(m.ohm, 8.0);
            close(m.in_unit(unit), value, 1e-9);
        }
    }

    // BEH-5: an IPD stores dBV; the app shows dBu and the volts and watts of that dBu
    #[test]
    fn a_stored_dbv_is_shown_as_dbu_with_its_volts_and_watts() {
        // read with the Windows app on an IPD 2400 (4 ohm): the dBu it shows, and the volts and watts
        for (dbv, dbu, vpk, watt) in [(29.04, 31.26, 40.0, 200.0), (32.56, 34.78, 60.0, 450.0)] {
            let l = Limiter::from_dbu(dbv + math::DBV_TO_DBU, 4.0);
            close(l.dbu, dbu, 0.02);
            close(l.vpk, vpk, 0.1);
            close(l.watt, watt, 1.0);
        }
        // the ceilings of the library, in dBV, are the volts the vendor app allows
        for (dbv, vpk) in [(33.8, 69.0), (36.78, 98.0), (39.83, 138.7)] {
            close(Limiter::from_dbu(dbv + math::DBV_TO_DBU, 4.0).vpk, vpk, 0.5);
        }
        // the other way: 40 Vpk is 31.26 dBu, and 29.04 dBV
        let l = Limiter::from_vpk(40.0, 4.0);
        close(l.dbu, 31.26, 0.02);
        close(l.dbu - math::DBV_TO_DBU, 29.04, 0.02);
    }

    #[test]
    fn snap_puts_a_rounding_error_back_on_the_edge() {
        let range = (16.78, 39.83);
        assert_eq!(snap(39.83 + 1e-12, range), 39.83);
        assert_eq!(snap(16.78 - 1e-12, range), 16.78);
        assert_eq!(snap(39.84, range), 39.84);
        assert_eq!(snap(30.0, range), 30.0);
        // an edge reached through another unit
        let hi = math::dbu(math::vpk_from_dbu(39.83));
        assert_eq!(snap(hi, range), 39.83);
    }

    #[test]
    fn labels() {
        assert_eq!(
            [Unit::Vpk, Unit::Ohm, Unit::Watt, Unit::Dbu].map(Unit::label),
            ["Vpk", "Ohm", "W", "dBu"]
        );
    }
}
