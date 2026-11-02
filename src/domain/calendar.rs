//! The league's calendar: the NBA dates its games by the Eastern clock, so a game that tips
//! off at 10:30 pm in Los Angeles on the 2nd is a game of the 2nd although it is the 3rd in
//! UTC and in Europe. Every date the application reads as "today" or as a game's date is
//! an Eastern date; the daily sync's own clock time is configured apart.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};

/// The zone the league's schedule reads in.
pub const LEAGUE_ZONE: chrono_tz::Tz = chrono_tz::America::New_York;

/// The league date of an instant.
pub fn league_date(now: DateTime<Utc>) -> NaiveDate {
    now.with_timezone(&LEAGUE_ZONE).date_naive()
}

/// The span of a league date in UTC: from midnight to the last second of the day, Eastern
/// time. Midnight and 23:59:59 never fall in a daylight-saving gap, so the fallbacks are
/// for form.
pub fn day_span(date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let local = |h: u32, m: u32, s: u32| -> DateTime<Utc> {
        let t = date.and_hms_opt(h, m, s).unwrap_or_default();
        LEAGUE_ZONE
            .from_local_datetime(&t)
            .earliest()
            .or_else(|| LEAGUE_ZONE.from_local_datetime(&t).latest())
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_else(|| t.and_utc())
    };
    (local(0, 0, 0), local(23, 59, 59))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn a_late_tip_belongs_to_the_eastern_date() {
        // 7:30 pm Pacific on the 2nd is 10:30 pm Eastern, 3:30 am UTC on the 3rd: a game of
        // the 2nd. Midnight Eastern starts the 3rd.
        assert_eq!(
            league_date(utc("2026-11-03T03:30:00Z")),
            NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()
        );
        assert_eq!(
            league_date(utc("2026-11-03T05:00:00Z")),
            NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()
        );
    }

    #[test]
    fn the_day_span_follows_daylight_saving() {
        let (from, to) = day_span(NaiveDate::from_ymd_opt(2026, 11, 2).unwrap());
        // Eastern Standard Time, five hours behind.
        assert_eq!(from, utc("2026-11-02T05:00:00Z"));
        assert_eq!(to, utc("2026-11-03T04:59:59Z"));
        let (from, _) = day_span(NaiveDate::from_ymd_opt(2026, 4, 10).unwrap());
        assert_eq!(from, utc("2026-04-10T04:00:00Z"));
        // The day the clocks go back is 25 hours long.
        let (from, to) = day_span(NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
        assert_eq!((to - from).num_seconds(), 25 * 3600 - 1);
    }
}
