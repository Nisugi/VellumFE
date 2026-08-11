//! Elanthian time — the in-game clock, computed locally.
//!
//! GemStone's clock runs on US Eastern time, so the phase of the in-game
//! day is derivable from the system clock with no wire signal, no `time`
//! command, and no round trip. It is available at login before any game
//! text arrives, and it keeps working while disconnected.
//!
//! **A caveat worth respecting.** gswiki (*Elanthian calendar*) says the
//! clock is Eastern but that "the sunrises and sunsets more closely follow
//! central standard time". So sunrise/sunset are NOT simply Eastern-clock
//! constants, and a formula presenting them as authoritative would be
//! subtly wrong twice a day. The phase boundaries here are therefore
//! *configurable* defaults rather than a claim of exactness — a user who
//! finds night starting too early can move it without a code change.

use serde::{Deserialize, Serialize};

/// A coarse phase of the in-game day.
///
/// The four Elanthian hour-names (gswiki) map onto these: Hour of Lumnis =
/// dawn, Hour of Phoen = noon (Day), Hour of Tonis = dusk, Hour of Ronan =
/// midnight (Night). The `time` verb also reports a phase in prose ("It is
/// currently afternoon").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DayPhase {
    Dawn,
    Day,
    Dusk,
    Night,
}

impl DayPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            DayPhase::Dawn => "dawn",
            DayPhase::Day => "day",
            DayPhase::Dusk => "dusk",
            DayPhase::Night => "night",
        }
    }
}

/// Hour boundaries between phases, on the 24-hour Eastern clock.
///
/// Defaults are deliberately conservative round numbers rather than a
/// sunrise model: see the module note on Eastern-vs-Central. A phase runs
/// from its own hour up to the next one, and `night` wraps midnight.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhaseBoundaries {
    pub dawn: u32,
    pub day: u32,
    pub dusk: u32,
    pub night: u32,
}

impl Default for PhaseBoundaries {
    fn default() -> Self {
        Self {
            dawn: 6,
            day: 8,
            dusk: 18,
            night: 20,
        }
    }
}

/// Whether US Eastern is on daylight time for this UTC instant.
///
/// Rule since 2007: DST runs from the second Sunday of March at 02:00
/// local standard time to the first Sunday of November at 02:00 local
/// daylight time. Computed rather than pulled from a timezone database so
/// this stays dependency-free and unit-testable.
fn eastern_is_dst(utc: chrono::DateTime<chrono::Utc>) -> bool {
    use chrono::{Datelike as _, TimeZone as _, Timelike as _};

    // Evaluate the rule against local STANDARD time (UTC-5).
    let standard = utc - chrono::Duration::hours(5);
    let (year, month, day, hour) = (
        standard.year(),
        standard.month(),
        standard.day(),
        standard.hour(),
    );

    let nth_sunday = |month: u32, nth: u32| -> u32 {
        let first = chrono::Utc
            .with_ymd_and_hms(year, month, 1, 0, 0, 0)
            .single()
            .expect("first of month is a valid date");
        // days_from_sunday: 0 = Sunday
        let offset = first.weekday().num_days_from_sunday();
        let first_sunday = 1 + ((7 - offset) % 7);
        first_sunday + 7 * (nth - 1)
    };

    match month {
        1..=2 | 12 => false,
        4..=10 => true,
        3 => {
            let start = nth_sunday(3, 2);
            day > start || (day == start && hour >= 2)
        }
        11 => {
            let end = nth_sunday(11, 1);
            // Ends at 02:00 daylight time == 01:00 standard time.
            day < end || (day == end && hour < 1)
        }
        _ => false,
    }
}

/// The in-game hour (0-23) for a UTC timestamp.
pub fn elanthian_hour(unix_seconds: i64) -> u32 {
    use chrono::Timelike as _;
    let Some(utc) = chrono::DateTime::from_timestamp(unix_seconds, 0) else {
        return 0;
    };
    let offset = if eastern_is_dst(utc) { 4 } else { 5 };
    (utc - chrono::Duration::hours(offset)).hour()
}

/// The phase of the in-game day for a UTC timestamp.
pub fn phase_at(unix_seconds: i64, bounds: PhaseBoundaries) -> DayPhase {
    phase_for_hour(elanthian_hour(unix_seconds), bounds)
}

/// Phase for an already-resolved in-game hour. Split out so the boundary
/// logic is testable without a clock.
pub fn phase_for_hour(hour: u32, bounds: PhaseBoundaries) -> DayPhase {
    let hour = hour % 24;
    // Walk the boundaries in clock order; `night` owns everything from its
    // own hour through midnight and on to dawn.
    if hour >= bounds.night || hour < bounds.dawn {
        DayPhase::Night
    } else if hour >= bounds.dusk {
        DayPhase::Dusk
    } else if hour >= bounds.day {
        DayPhase::Day
    } else {
        DayPhase::Dawn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32) -> DayPhase {
        phase_for_hour(hour, PhaseBoundaries::default())
    }

    /// Each default phase owns the hours between its boundary and the next.
    #[test]
    fn default_boundaries_partition_the_day() {
        assert_eq!(at(6), DayPhase::Dawn);
        assert_eq!(at(7), DayPhase::Dawn);
        assert_eq!(at(8), DayPhase::Day);
        assert_eq!(at(17), DayPhase::Day);
        assert_eq!(at(18), DayPhase::Dusk);
        assert_eq!(at(19), DayPhase::Dusk);
        assert_eq!(at(20), DayPhase::Night);
        assert_eq!(at(23), DayPhase::Night);
    }

    /// Night wraps midnight — the case a naive range check gets wrong.
    #[test]
    fn night_wraps_past_midnight() {
        assert_eq!(at(0), DayPhase::Night);
        assert_eq!(at(3), DayPhase::Night);
        assert_eq!(at(5), DayPhase::Night);
        assert_eq!(at(6), DayPhase::Dawn, "dawn begins the new day");
    }

    /// Every hour resolves to exactly one phase; none falls through.
    #[test]
    fn every_hour_has_a_phase() {
        for hour in 0..24 {
            let _ = at(hour);
        }
        // And out-of-range input wraps rather than panicking.
        assert_eq!(at(24), at(0));
        assert_eq!(at(49), at(1));
    }

    /// Boundaries are configurable: a user who thinks night starts later
    /// can move it without a code change.
    #[test]
    fn boundaries_are_configurable() {
        let late = PhaseBoundaries {
            dawn: 5,
            day: 7,
            dusk: 20,
            night: 22,
        };
        assert_eq!(phase_for_hour(21, late), DayPhase::Dusk);
        assert_eq!(phase_for_hour(21, PhaseBoundaries::default()), DayPhase::Night);
        assert_eq!(phase_for_hour(5, late), DayPhase::Dawn);
    }

    /// US Eastern DST: second Sunday of March through first Sunday of
    /// November. 2026: March 8, November 1.
    #[test]
    fn eastern_dst_window_is_correct_for_2026() {
        use chrono::TimeZone as _;
        let utc = |m, d, h| chrono::Utc.with_ymd_and_hms(2026, m, d, h, 0, 0).unwrap();

        // Deep winter and high summer are unambiguous.
        assert!(!eastern_is_dst(utc(1, 15, 12)), "January is standard time");
        assert!(eastern_is_dst(utc(7, 15, 12)), "July is daylight time");

        // Around the March transition (2nd Sunday = the 8th).
        assert!(!eastern_is_dst(utc(3, 1, 12)), "before the switch");
        assert!(eastern_is_dst(utc(3, 20, 12)), "after the switch");

        // Around the November transition (1st Sunday = the 1st).
        assert!(!eastern_is_dst(utc(11, 20, 12)), "after falling back");
    }

    /// Cross-check against an independently derived value: 2026-08-10
    /// 20:44 UTC is 16:44 Eastern (EDT, UTC-4), so the hour is 16 and the
    /// phase is Day under the defaults.
    #[test]
    fn matches_an_independently_derived_wall_clock() {
        let unix = 1_786_394_661_i64; // 2026-08-10 20:44 UTC
        assert_eq!(elanthian_hour(unix), 16);
        assert_eq!(phase_at(unix, PhaseBoundaries::default()), DayPhase::Day);
    }

    /// The hour actually shifts with DST — a fixed offset would be an hour
    /// wrong for two-thirds of the year.
    #[test]
    fn hour_follows_daylight_saving() {
        use chrono::TimeZone as _;
        // 17:00 UTC is 12:00 EST (winter) and 13:00 EDT (summer).
        let winter = chrono::Utc
            .with_ymd_and_hms(2026, 1, 15, 17, 0, 0)
            .unwrap()
            .timestamp();
        let summer = chrono::Utc
            .with_ymd_and_hms(2026, 7, 15, 17, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(elanthian_hour(winter), 12);
        assert_eq!(elanthian_hour(summer), 13);
    }
}
