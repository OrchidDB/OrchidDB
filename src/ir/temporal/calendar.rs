//! Proleptic Gregorian dates across Cypher's signed nine-digit year range.
//!
//! Gregorian leap years repeat every 400 years (146097 days). Using a
//! representative cycle lets chrono validate month/day and weekday rules
//! without restricting the stored year to chrono's narrower range.
use super::{Result, TemporalValue};
use chrono::{Datelike, NaiveDate, NaiveTime};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CalendarDate {
    pub(crate) year: i32,
    pub(crate) month: u32,
    pub(crate) day: u32,
}

impl CalendarDate {
    pub fn new(year: i32, month: u32, day: u32) -> Result<Self> {
        if !(-999_999_999..=999_999_999).contains(&year)
            || NaiveDate::from_ymd_opt(2000 + year.rem_euclid(400), month, day).is_none()
        {
            return Err("Date outside the Gregorian calendar range".into());
        }
        Ok(Self { year, month, day })
    }
    pub fn from_chrono(date: NaiveDate) -> Self {
        Self {
            year: date.year(),
            month: date.month(),
            day: date.day(),
        }
    }
    pub fn chrono(self) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(self.year, self.month, self.day)
    }
    pub fn representative(self) -> NaiveDate {
        NaiveDate::from_ymd_opt(2000 + self.year.rem_euclid(400), self.month, self.day).unwrap()
    }
    pub fn epoch_days(self) -> i64 {
        let date = self.representative();
        i64::from(date.num_days_from_ce()) - 719163
            + i64::from(self.year - date.year()) / 400 * 146097
    }
    pub fn from_epoch_days(days: i64) -> Result<Self> {
        let anchor = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let relative = days
            .checked_sub(i64::from(anchor.num_days_from_ce()) - 719163)
            .ok_or("Date overflow")?;
        let cycle = relative.div_euclid(146097);
        let date = anchor + chrono::Duration::days(relative.rem_euclid(146097));
        let year = i64::from(date.year()) + cycle * 400;
        Self::new(
            i32::try_from(year).map_err(|_| "Year overflow")?,
            date.month(),
            date.day(),
        )
    }
    pub fn add_months(self, months: i64) -> Result<Self> {
        let total = (i64::from(self.year) * 12 + i64::from(self.month) - 1)
            .checked_add(months)
            .ok_or("Month overflow")?;
        let year = i32::try_from(total.div_euclid(12)).map_err(|_| "Year overflow")?;
        let month = total.rem_euclid(12) as u32 + 1;
        for day in (1..=self.day).rev() {
            if let Ok(date) = Self::new(year, month, day) {
                return Ok(date);
            }
        }
        Err("Year outside calendar range".into())
    }
    pub fn add_days(self, days: i64) -> Result<Self> {
        Self::from_epoch_days(self.epoch_days().checked_add(days).ok_or("Day overflow")?)
    }
    pub fn date_value(self) -> TemporalValue {
        self.chrono()
            .map(TemporalValue::Date)
            .unwrap_or(TemporalValue::WideDate(self))
    }
    pub fn datetime_value(self, time: NaiveTime) -> TemporalValue {
        self.chrono()
            .map(|date| TemporalValue::LocalDateTime(date.and_time(time)))
            .unwrap_or(TemporalValue::WideLocalDateTime(self, time))
    }
}

impl fmt::Display for CalendarDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.year < 0 {
            write!(f, "-{:04}", self.year.unsigned_abs())?;
        } else if self.year > 9999 {
            write!(f, "+{}", self.year)?;
        } else {
            write!(f, "{:04}", self.year)?;
        }
        write!(f, "-{:02}-{:02}", self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cycles_roundtrip_and_match_chrono() {
        for year in [
            -999999999, -400000, -400, -1, 0, 1, 1900, 2000, 2400, 400000, 999999999,
        ] {
            for (month, day) in [(1, 1), (2, 28), (3, 1), (12, 31)] {
                let date = CalendarDate::new(year, month, day).unwrap();
                assert_eq!(
                    CalendarDate::from_epoch_days(date.epoch_days()).unwrap(),
                    date
                );
                if let Some(chrono) = date.chrono() {
                    assert_eq!(
                        date.epoch_days(),
                        i64::from(chrono.num_days_from_ce()) - 719163
                    );
                }
            }
        }
        assert!(CalendarDate::new(100000000, 2, 29).is_ok());
        assert!(CalendarDate::new(100000100, 2, 29).is_err());
    }
}
