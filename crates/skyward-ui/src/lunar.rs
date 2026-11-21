//! The Chinese lunar calendar, worked out from the new moons and the
//! solar terms rather than tabulated, as the charging screen's date needs
//! it: the month and the day a Gregorian date falls on.

use calendrical_calculations::chinese_based::{Chinese, chinese_based_date_from_fixed};
use calendrical_calculations::iso::fixed_from_iso;

const NAMES: [&str; 12] = ["正月", "二月", "三月", "四月", "五月", "六月", "七月", "八月", "九月", "十月", "冬月", "腊月"];
const NAMES_HANT: [&str; 12] = ["正月", "二月", "三月", "四月", "五月", "六月", "七月", "八月", "九月", "十月", "冬月", "臘月"];
const DAYS: [&str; 30] = [
    "初一", "初二", "初三", "初四", "初五", "初六", "初七", "初八", "初九", "初十", "十一", "十二", "十三", "十四", "十五", "十六", "十七",
    "十八", "十九", "二十", "廿一", "廿二", "廿三", "廿四", "廿五", "廿六", "廿七", "廿八", "廿九", "三十",
];

/// The lunar month and day a Gregorian date falls on: the month from 1 to
/// 12, the day from 1 to 30, and whether the month is the year's leap one.
pub fn lunar(year: i32, month: u32, day: u32) -> Option<(u32, u32, bool)> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let c = chinese_based_date_from_fixed::<Chinese>(fixed_from_iso(year, month as u8, day as u8));
    // The calendar numbers its months through the leap month, which takes
    // the name of the month before it: a month after the leap one is
    // named one lower than its place.
    let (name, leap) = match c.leap_month {
        Some(at) if c.month == at.get() => (c.month - 1, true),
        Some(at) if c.month > at.get() => (c.month - 1, false),
        _ => (c.month, false),
    };
    Some((name as u32, c.day as u32, leap))
}

/// The lunar date written as the charging screen shows it: `十月初七`, the
/// leap month marked with the word for it.
pub fn text(year: i32, month: u32, day: u32, hant: bool) -> Option<String> {
    let (m, d, leap) = lunar(year, month, day)?;
    let names = if hant { NAMES_HANT } else { NAMES };
    let lead = if leap { if hant { "閏" } else { "闰" } } else { "" };
    Some(format!("{lead}{}{}", names[(m - 1) as usize], DAYS[(d - 1) as usize]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_date_reads_as_the_phone_shows_it() {
        assert_eq!(text(2026, 11, 15, false).as_deref(), Some("十月初七"));
    }

    #[test]
    fn a_leap_month_is_marked_and_the_year_turns_at_its_first_day() {
        assert_eq!(text(2025, 7, 25, false).as_deref(), Some("闰六月初一"));
        assert_eq!(text(2024, 2, 10, false).as_deref(), Some("正月初一"));
        assert_eq!(text(1900, 1, 31, false).as_deref(), Some("正月初一"));
    }
}
