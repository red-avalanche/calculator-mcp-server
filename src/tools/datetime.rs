// Date/time calculation tools — date arithmetic, date differences, and
// calendar utilities. Uses chrono with no timezone support (naive dates only).
//
// Key convention (PLAN.md D6): months and years count as WHOLE calendar
// months/years only, using clamped anniversaries. For example:
//   Jan 13 + 1 month → Feb 13 (anniversary)
//   Jan 31 + 1 month → Feb 28 (clamped to last day of Feb)
//   Feb 29 + 1 year  → Feb 28 (clamped, non-leap target year)
// Complete months from S to E = largest n where add_months_clamped(S, n) <= E.

use chrono::{Datelike, Months, NaiveDate, NaiveDateTime, TimeDelta, Timelike};
use rmcp::model::CallToolResult;
use serde_json::json;

use crate::tools::{err_json, ok_json};

// ===== Request structs =====

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DateAddRequest {
    /// The starting date. Accepted formats: YYYY-MM-DD,
    /// YYYY-MM-DD HH:MM:SS, or YYYY-MM-DDTHH:MM:SS. No timezone suffixes.
    pub date: String,
    /// Years to add (may be negative). Uses clamped calendar arithmetic
    /// (e.g. Feb 29 + 1 year → Feb 28 in non-leap years).
    #[serde(default)]
    pub years: i64,
    /// Months to add (may be negative). Clamped to last day of target month
    /// (e.g. Jan 31 + 1 month → Feb 28).
    #[serde(default)]
    pub months: i64,
    /// Weeks to add (may be negative).
    #[serde(default)]
    pub weeks: i64,
    /// Days to add (may be negative).
    #[serde(default)]
    pub days: i64,
    /// Hours to add (may be negative).
    #[serde(default)]
    pub hours: i64,
    /// Minutes to add (may be negative).
    #[serde(default)]
    pub minutes: i64,
    /// Seconds to add (may be negative).
    #[serde(default)]
    pub seconds: i64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DateDiffRequest {
    /// The start date. Accepted formats: YYYY-MM-DD,
    /// YYYY-MM-DD HH:MM:SS, or YYYY-MM-DDTHH:MM:SS.
    pub start: String,
    /// The end date.
    pub end: String,
    /// Unit for the result: "auto" (default, gives Y/M/D breakdown),
    /// "years", "months", "weeks", "days", "hours", "minutes", or "seconds".
    /// Years/months count complete clamped anniversaries only.
    #[serde(default = "default_diff_unit")]
    pub unit: String,
}

fn default_diff_unit() -> String {
    "auto".to_string()
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DateInfoRequest {
    /// The date to analyze. Accepted formats: YYYY-MM-DD,
    /// YYYY-MM-DD HH:MM:SS, or YYYY-MM-DDTHH:MM:SS.
    pub date: String,
}

// ===== Parsing helpers =====

/// Parses a date string into a NaiveDateTime. Date-only inputs are treated
/// as midnight. Returns whether the input was date-only (no time component).
fn parse_date(s: &str) -> Result<(NaiveDateTime, bool), String> {
    let trimmed = s.trim();

    // Reject timezone suffixes
    if trimmed.contains('+')
        || (trimmed.contains('T') && trimmed.len() > 10 && trimmed.matches('T').count() > 1)
        || trimmed.contains('Z')
        || trimmed.contains("UTC")
        || trimmed.contains("GMT")
    {
        // Check more carefully: + in a time string could be a timezone offset
        if trimmed.contains('+') && trimmed.len() > 19 {
            return Err(format!(
                "Timezone suffixes are not supported. Input: '{}'. Accepted formats: YYYY-MM-DD, YYYY-MM-DD HH:MM:SS, YYYY-MM-DDTHH:MM:SS",
                s
            ));
        }
        if trimmed.contains('Z') {
            return Err(format!(
                "Timezone suffixes are not supported. Input: '{}'. Accepted formats: YYYY-MM-DD, YYYY-MM-DD HH:MM:SS, YYYY-MM-DDTHH:MM:SS",
                s
            ));
        }
    }

    // Try YYYY-MM-DD HH:MM:SS
    if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
        return Ok((dt, false));
    }
    // Try YYYY-MM-DDTHH:MM:SS
    if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return Ok((dt, false));
    }
    // Try YYYY-MM-DD
    if let Ok(d) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Ok((d.and_hms_opt(0, 0, 0).unwrap(), true));
    }

    Err(format!(
        "Cannot parse '{}' as date. Accepted formats: YYYY-MM-DD, YYYY-MM-DD HH:MM:SS, YYYY-MM-DDTHH:MM:SS",
        s
    ))
}

// ===== Date arithmetic helpers =====

/// Adds months to a NaiveDate with clamping (chrono's checked_add_months).
fn add_months_clamped(date: NaiveDate, months: i64) -> Option<NaiveDate> {
    if months >= 0 {
        let m = u32::try_from(months).ok()?;
        date.checked_add_months(Months::new(m))
    } else {
        let m = u32::try_from(-months).ok()?;
        date.checked_sub_months(Months::new(m))
    }
}

/// Counts complete clamped calendar months between two dates.
/// Returns the largest n such that add_months_clamped(start, n) <= end.
/// Sign: positive if end > start, negative if end < start.
fn complete_months_between(start: NaiveDate, end: NaiveDate) -> i64 {
    if start == end {
        return 0;
    }
    let sign = if end > start { 1i64 } else { -1i64 };
    let (s, e) = if sign > 0 { (start, end) } else { (end, start) };

    // Initial estimate: calendar month difference
    let mut months =
        (e.year() as i64 - s.year() as i64) * 12 + (e.month() as i64 - s.month() as i64);

    // Adjust down if the estimate overshoots
    while months > 0 {
        if let Some(d) = add_months_clamped(s, months) {
            if d <= e {
                break;
            }
        }
        months -= 1;
    }

    sign * months.max(0)
}

/// Counts complete clamped calendar years between two dates.
fn complete_years_between(start: NaiveDate, end: NaiveDate) -> i64 {
    if start == end {
        return 0;
    }
    let sign = if end > start { 1i64 } else { -1i64 };
    let (s, e) = if sign > 0 { (start, end) } else { (end, start) };

    let mut years = e.year() as i64 - s.year() as i64;

    while years > 0 {
        if let Some(d) = add_months_clamped(s, years * 12) {
            if d <= e {
                break;
            }
        }
        years -= 1;
    }

    sign * years.max(0)
}

/// Returns the number of days in the given month.
fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    next.unwrap().pred_opt().unwrap().day()
}

/// Returns true if the given year is a leap year.
fn is_leap_year(year: i32) -> bool {
    days_in_month(year, 2) == 29
}

// ===== Pure functions =====

/// Adds a duration to a date. Years and months use clamped calendar
/// arithmetic; weeks/days/hours/minutes/seconds use exact time arithmetic.
#[allow(clippy::too_many_arguments)]
pub fn date_add(
    date: &str,
    years: i64,
    months: i64,
    weeks: i64,
    days: i64,
    hours: i64,
    minutes: i64,
    seconds: i64,
) -> Result<String, String> {
    let (dt, date_only) = parse_date(date)?;

    // Apply years and months (clamped calendar arithmetic)
    let total_months = years * 12 + months;
    let new_date = add_months_clamped(dt.date(), total_months)
        .ok_or_else(|| "Date overflow: result is out of representable range".to_string())?;

    let mut result = new_date
        .and_hms_opt(dt.hour(), dt.minute(), dt.second())
        .ok_or_else(|| "Invalid time component".to_string())?;

    // Apply time duration (weeks, days, hours, minutes, seconds)
    let total_seconds = weeks * 604800 + days * 86400 + hours * 3600 + minutes * 60 + seconds;
    if total_seconds != 0 {
        result = result
            .checked_add_signed(TimeDelta::seconds(total_seconds))
            .ok_or_else(|| {
                "Date/time overflow: result is out of representable range".to_string()
            })?;
    }

    // Output format: date-only if input was date-only and no time units were used
    let has_time_units = hours != 0 || minutes != 0 || seconds != 0;
    if date_only && !has_time_units {
        Ok(result.date().to_string())
    } else {
        Ok(result.to_string())
    }
}

/// Computes the difference between two dates in the specified unit.
pub fn date_diff(start: &str, end: &str, unit: &str) -> Result<serde_json::Value, String> {
    let (start_dt, _) = parse_date(start)?;
    let (end_dt, _) = parse_date(end)?;

    let duration = end_dt.signed_duration_since(start_dt);

    match unit {
        "auto" => {
            let sign = if end_dt >= start_dt { 1i64 } else { -1i64 };
            let (s, e) = if sign > 0 {
                (start_dt.date(), end_dt.date())
            } else {
                (end_dt.date(), start_dt.date())
            };

            // Complete years
            let y = complete_years_between(s, e);
            let after_years = add_months_clamped(s, y * 12).unwrap_or(e);

            // Complete months from after_years
            let m = complete_months_between(after_years, e);
            let after_months = add_months_clamped(after_years, m).unwrap_or(e);

            // Remaining days
            let d = e.signed_duration_since(after_months).num_days();

            let total_days = duration.num_days();
            let total_seconds = duration.num_seconds();

            let mut result = json!({
                "years": y,
                "months": m,
                "days": d,
                "total_days": total_days,
                "sign": sign
            });

            // Include total_seconds for datetime inputs
            if start_dt.time() != NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().time()
                || end_dt.time() != NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().time()
            {
                result["total_seconds"] = json!(total_seconds);
            }

            Ok(result)
        }
        "years" => Ok(json!({"result": complete_years_between(start_dt.date(), end_dt.date()), "unit": "years"})),
        "months" => Ok(json!({"result": complete_months_between(start_dt.date(), end_dt.date()), "unit": "months"})),
        "weeks" => Ok(json!({"result": duration.num_days() / 7, "unit": "weeks"})),
        "days" => Ok(json!({"result": duration.num_days(), "unit": "days"})),
        "hours" => Ok(json!({"result": duration.num_seconds() / 3600, "unit": "hours"})),
        "minutes" => Ok(json!({"result": duration.num_seconds() / 60, "unit": "minutes"})),
        "seconds" => Ok(json!({"result": duration.num_seconds(), "unit": "seconds"})),
        _ => Err(format!(
            "Unknown unit: {} (expected auto, years, months, weeks, days, hours, minutes, or seconds)",
            unit
        )),
    }
}

/// Returns calendar information about a date.
pub fn date_info(date: &str) -> Result<serde_json::Value, String> {
    let (dt, _) = parse_date(date)?;
    let d = dt.date();

    let weekday = match d.weekday() {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    };
    let iso_week = d.iso_week().week();
    let day_of_year = d.ordinal();
    let dim = days_in_month(d.year(), d.month());
    let leap = is_leap_year(d.year());
    let last_day_ordinal = NaiveDate::from_ymd_opt(d.year(), 12, 31).unwrap().ordinal();
    let days_remaining = last_day_ordinal - day_of_year;

    Ok(json!({
        "weekday": weekday,
        "iso_week": iso_week,
        "day_of_year": day_of_year,
        "days_in_month": dim,
        "is_leap_year": leap,
        "days_remaining_in_year": days_remaining
    }))
}

// ===== Tool wrappers =====

pub fn date_add_tool(req: DateAddRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match date_add(
        &req.date,
        req.years,
        req.months,
        req.weeks,
        req.days,
        req.hours,
        req.minutes,
        req.seconds,
    ) {
        Ok(result) => Ok(ok_json(json!({"result": result}))),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn date_diff_tool(req: DateDiffRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match date_diff(&req.start, &req.end, &req.unit) {
        Ok(result) => Ok(ok_json(result)),
        Err(e) => Ok(err_json(&e)),
    }
}

pub fn date_info_tool(req: DateInfoRequest) -> Result<CallToolResult, rmcp::ErrorData> {
    match date_info(&req.date) {
        Ok(result) => Ok(ok_json(result)),
        Err(e) => Ok(err_json(&e)),
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    // --- date_add ---

    #[test]
    fn test_date_add_days() {
        assert_eq!(
            date_add("2024-01-15", 0, 0, 0, 1, 0, 0, 0).unwrap(),
            "2024-01-16"
        );
    }

    #[test]
    fn test_date_add_months_clamped() {
        // Jan 31 + 1 month → Feb 28 (2024 is leap year, so Feb 29)
        assert_eq!(
            date_add("2024-01-31", 0, 1, 0, 0, 0, 0, 0).unwrap(),
            "2024-02-29"
        );
        // Jan 31 + 1 month → Feb 28 (2023 is not leap year)
        assert_eq!(
            date_add("2023-01-31", 0, 1, 0, 0, 0, 0, 0).unwrap(),
            "2023-02-28"
        );
    }

    #[test]
    fn test_date_add_years_clamped() {
        // Feb 29 + 1 year → Feb 28 (2025 is not leap year)
        assert_eq!(
            date_add("2024-02-29", 1, 0, 0, 0, 0, 0, 0).unwrap(),
            "2025-02-28"
        );
    }

    #[test]
    fn test_date_add_negative() {
        assert_eq!(
            date_add("2024-03-15", 0, -1, 0, 0, 0, 0, 0).unwrap(),
            "2024-02-15"
        );
    }

    #[test]
    fn test_date_add_with_time() {
        assert_eq!(
            date_add("2024-01-15 10:30:00", 0, 0, 0, 0, 1, 0, 0).unwrap(),
            "2024-01-15 11:30:00"
        );
    }

    #[test]
    fn test_date_add_time_to_date_only() {
        // Date-only input + hours → datetime output
        assert_eq!(
            date_add("2024-01-15", 0, 0, 0, 0, 1, 0, 0).unwrap(),
            "2024-01-15 01:00:00"
        );
    }

    #[test]
    fn test_date_add_invalid_format() {
        assert!(date_add("not-a-date", 0, 0, 0, 0, 0, 0, 0).is_err());
    }

    // --- date_diff ---

    #[test]
    fn test_date_diff_auto() {
        let r = date_diff("2024-01-13", "2024-02-28", "auto").unwrap();
        assert_eq!(r["years"], 0);
        assert_eq!(r["months"], 1);
        assert_eq!(r["days"], 15);
        assert_eq!(r["sign"], 1);
    }

    #[test]
    fn test_date_diff_auto_jan31_feb27() {
        // Jan 31 → Feb 27: 0 complete months (anniversary Feb 28 not reached)
        let r = date_diff("2024-01-31", "2024-02-27", "auto").unwrap();
        assert_eq!(r["months"], 0);
    }

    #[test]
    fn test_date_diff_auto_jan31_feb28() {
        // Jan 31 → Feb 28: 1 complete month (clamped anniversary is Feb 29 in leap year)
        let r = date_diff("2024-01-31", "2024-02-28", "auto").unwrap();
        // 2024 is leap, so Jan 31 + 1 month = Feb 29. Feb 29 > Feb 28, so 0 months.
        assert_eq!(r["months"], 0);
    }

    #[test]
    fn test_date_diff_auto_jan31_feb29_leap() {
        // Jan 31 → Feb 29 (leap year): 1 complete month
        let r = date_diff("2024-01-31", "2024-02-29", "auto").unwrap();
        assert_eq!(r["months"], 1);
    }

    #[test]
    fn test_date_diff_years() {
        let r = date_diff("2020-06-15", "2024-06-15", "years").unwrap();
        assert_eq!(r["result"], 4);
    }

    #[test]
    fn test_date_diff_months() {
        let r = date_diff("2024-01-15", "2024-04-15", "months").unwrap();
        assert_eq!(r["result"], 3);
    }

    #[test]
    fn test_date_diff_days() {
        let r = date_diff("2024-01-01", "2024-01-31", "days").unwrap();
        assert_eq!(r["result"], 30);
    }

    #[test]
    fn test_date_diff_negative() {
        let r = date_diff("2024-03-15", "2024-01-15", "days").unwrap();
        assert_eq!(r["result"], -60);
    }

    #[test]
    fn test_date_diff_seconds() {
        let r = date_diff("2024-01-01 00:00:00", "2024-01-01 00:01:00", "seconds").unwrap();
        assert_eq!(r["result"], 60);
    }

    #[test]
    fn test_date_diff_invalid_unit() {
        assert!(date_diff("2024-01-01", "2024-01-02", "centuries").is_err());
    }

    // --- date_info ---

    #[test]
    fn test_date_info() {
        let r = date_info("2024-02-29").unwrap();
        assert_eq!(r["weekday"], "Thursday");
        assert_eq!(r["iso_week"], 9);
        assert_eq!(r["day_of_year"], 60);
        assert_eq!(r["days_in_month"], 29);
        assert_eq!(r["is_leap_year"], true);
    }

    #[test]
    fn test_date_info_non_leap() {
        let r = date_info("2023-02-15").unwrap();
        assert_eq!(r["days_in_month"], 28);
        assert_eq!(r["is_leap_year"], false);
    }

    #[test]
    fn test_date_info_days_remaining() {
        let r = date_info("2024-12-31").unwrap();
        assert_eq!(r["days_remaining_in_year"], 0);

        let r = date_info("2024-01-01").unwrap();
        assert_eq!(r["days_remaining_in_year"], 365);
    }
}
