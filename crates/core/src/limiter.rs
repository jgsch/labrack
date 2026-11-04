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

/// The four figures of an output limiter. `dbu` is the number the amp stores and shows on its front
/// panel. The volts and watts are the vendor app's: on an IPD they correspond to a dBu that is 2.22 dB
/// above the stored one (`offset`), on an IPX to the stored dBu itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limiter {
    pub vpk: f64,
    pub ohm: f64,
    pub watt: f64,
    pub dbu: f64,
    offset: f64,
}

impl Limiter {
    /// A threshold of `dbu` as stored by an amp whose volts are `offset` dB above that dBu.
    pub fn stored(dbu: f64, ohm: f64, offset: f64) -> Limiter {
        let vpk = math::vpk_from_dbu(dbu + offset);
        Limiter { vpk, ohm, watt: math::watt(vpk, ohm), dbu, offset }
    }

    fn at_volts(vpk: f64, ohm: f64, offset: f64) -> Limiter {
        Limiter { vpk, ohm, watt: math::watt(vpk, ohm), dbu: math::dbu(vpk) - offset, offset }
    }

    /// For an amp whose stored dBu is the true one (the IPX).
    pub fn from_vpk(vpk: f64, ohm: f64) -> Limiter {
        Limiter::at_volts(vpk, ohm, 0.0)
    }

    pub fn from_dbu(dbu: f64, ohm: f64) -> Limiter {
        Limiter::stored(dbu, ohm, 0.0)
    }

    pub fn from_watt(watt: f64, ohm: f64) -> Limiter {
        Limiter::from_vpk(math::vpk_from_watt(watt, ohm), ohm)
    }

    /// The same threshold at another load: only the watts change.
    pub fn with_ohm(self, ohm: f64) -> Limiter {
        Limiter { ohm, watt: math::watt(self.vpk, ohm), ..self }
    }

    /// The limiter after the user entered `value` in `unit`. The threshold is edited at the current
    /// load; entering the load keeps the threshold. A dBu entered is the dBu to store.
    pub fn edit(self, unit: Unit, value: f64) -> Limiter {
        match unit {
            Unit::Vpk => Limiter::at_volts(value, self.ohm, self.offset),
            Unit::Dbu => Limiter::stored(value, self.ohm, self.offset),
            Unit::Watt => Limiter::at_volts(math::vpk_from_watt(value, self.ohm), self.ohm, self.offset),
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

    // an IPD stores 2.22 dB less than the volts and watts of the vendor app go with
    #[test]
    fn an_ipd_threshold_is_the_stored_dbu_with_the_vendor_apps_volts() {
        // read with the Windows app on an IPD 2400: stored 29.04 is 40 Vpk, 200 W into 4 ohm
        let l = Limiter::stored(29.04, 4.0, math::IPD_DISPLAY_OFFSET);
        assert_eq!(l.dbu, 29.04);
        close(l.vpk, 40.0, 0.05);
        close(l.watt, 200.0, 0.8);
        // and entering volts or watts stores the lower dBu
        close(l.edit(Unit::Vpk, 60.0).dbu, 34.77 - 2.22, 0.01);
        close(l.edit(Unit::Watt, 450.0).dbu, 34.77 - 2.22, 0.01);
        assert_eq!(l.edit(Unit::Dbu, 25.0).dbu, 25.0);
        assert_eq!(l.edit(Unit::Ohm, 8.0).dbu, 29.04);
        // an IPX stores the dBu of its volts
        close(Limiter::from_vpk(60.0, 4.0).dbu, 34.77, 0.01);
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
