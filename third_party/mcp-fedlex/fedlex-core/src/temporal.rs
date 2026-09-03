//! Bi-temporale Zeitstempel.
//!
//! mcp-fedlex unterscheidet zwei Zeitachsen. `ValidAsOf` ist die Gültigkeitszeit
//! (welche Fassung einer Norm galt an einem Stichtag), `TransactionTime` ist die
//! Systemzeit (wann die Information erfasst wurde, für rückwirkende Korrekturen).
//! Beide sind dünne, typsichere Hüllen um ein Datum bzw. einen Zeitpunkt, damit
//! die beiden Achsen nicht versehentlich vertauscht werden.

use serde::{Deserialize, Serialize};
use std::fmt;
use time::{Date, Month, OffsetDateTime, UtcOffset, Weekday};

/// Gültigkeitszeit. Der Stichtag, dessen konsolidierte Fassung gemeint ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ValidAsOf(pub Date);

impl ValidAsOf {
    /// Erzeugt eine Gültigkeitszeit aus einem Datum.
    pub fn new(date: Date) -> Self {
        Self(date)
    }

    /// Liefert das zugrunde liegende Datum.
    pub fn date(&self) -> Date {
        self.0
    }
}

impl fmt::Display for ValidAsOf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Date> for ValidAsOf {
    fn from(date: Date) -> Self {
        Self(date)
    }
}

/// Systemzeit. Der Zeitpunkt, zu dem ein Fakt erfasst oder korrigiert wurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TransactionTime(pub OffsetDateTime);

impl TransactionTime {
    /// Erzeugt eine Systemzeit aus einem Zeitpunkt.
    pub fn new(at: OffsetDateTime) -> Self {
        Self(at)
    }

    /// Aktuelle Systemzeit (UTC).
    pub fn now() -> Self {
        Self(OffsetDateTime::now_utc())
    }

    /// Liefert den zugrunde liegenden Zeitpunkt.
    pub fn instant(&self) -> OffsetDateTime {
        self.0
    }
}

impl From<OffsetDateTime> for TransactionTime {
    fn from(at: OffsetDateTime) -> Self {
        Self(at)
    }
}

/// Kalendertag in der Schweiz (Europe/Zurich) zum gegebenen Zeitpunkt.
///
/// Schweizer Bundesrecht tritt um Mitternacht **Schweizer Zeit** in Kraft;
/// der UTC-Kalendertag hinkt dem zwischen 22:00/23:00 UTC und Mitternacht
/// hinterher und wäre als «heute» an Grenztagen juristisch falsch. CET/CEST
/// wird direkt gerechnet statt über eine tz-Datenbank: Die Regel (Sommerzeit
/// vom letzten März-Sonntag 01:00 UTC bis zum letzten Oktober-Sonntag
/// 01:00 UTC) ist seit 1996 gesetzlich fixiert und EU/CH-identisch.
pub fn swiss_date_at(instant: OffsetDateTime) -> Date {
    let utc = instant.to_offset(UtcOffset::UTC);
    let year = utc.year();
    let dst_start = last_sunday_utc_1am(year, Month::March);
    let dst_end = last_sunday_utc_1am(year, Month::October);
    let offset = if utc >= dst_start && utc < dst_end {
        UtcOffset::from_hms(2, 0, 0).expect("CEST ist ein gueltiger Offset")
    } else {
        UtcOffset::from_hms(1, 0, 0).expect("CET ist ein gueltiger Offset")
    };
    utc.to_offset(offset).date()
}

/// Der heutige Kalendertag in der Schweiz (Europe/Zurich).
pub fn swiss_today() -> Date {
    swiss_date_at(OffsetDateTime::now_utc())
}

/// 01:00 UTC am letzten Sonntag des Monats (März/Oktober haben 31 Tage).
fn last_sunday_utc_1am(year: i32, month: Month) -> OffsetDateTime {
    let mut d = Date::from_calendar_date(year, month, 31).expect("Maerz und Oktober haben 31 Tage");
    while d.weekday() != Weekday::Sunday {
        d = d.previous_day().expect("Monat enthaelt einen Sonntag");
    }
    d.with_hms(1, 0, 0)
        .expect("01:00 ist eine gueltige Uhrzeit")
        .assume_utc()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    #[test]
    fn valid_as_of_orders_chronologically() {
        let early = ValidAsOf::new(date!(1999 - 01 - 01));
        let late = ValidAsOf::new(date!(2020 - 06 - 15));
        assert!(early < late);
    }

    #[test]
    fn valid_as_of_displays_iso() {
        let v = ValidAsOf::new(date!(2002 - 12 - 31));
        assert_eq!(v.to_string(), "2002-12-31");
    }

    #[test]
    fn transaction_time_roundtrips_through_serde() {
        let tt = TransactionTime::new(datetime!(2026-06-01 10:30 UTC));
        let json = serde_json::to_string(&tt).unwrap();
        let back: TransactionTime = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tt);
    }

    #[test]
    fn transaction_time_now_is_monotonic_enough() {
        let a = TransactionTime::now();
        let b = TransactionTime::now();
        assert!(b.instant() >= a.instant());
    }
}
