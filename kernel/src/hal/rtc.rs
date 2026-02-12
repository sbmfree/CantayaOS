//! PL031 Real-Time Clock (RTC) driver
//!
//! QEMU virt machine exposes a PL031 at 0x0901_0000.
//! We read RTCDR (offset 0x000) to get the current UTC Unix timestamp.

/// PL031 base address on QEMU virt machine
const PL031_BASE: usize = 0x0901_0000;

/// RTCDR — Data Register (read = current Unix timestamp)
const RTCDR: usize = 0x000;

/// Read the PL031 Data Register — returns seconds since 1970-01-01 00:00:00 UTC.
pub fn read_unix_timestamp() -> u64 {
    unsafe {
        core::ptr::read_volatile((PL031_BASE + RTCDR) as *const u32) as u64
    }
}

/// Date-time structure
#[derive(Clone, Copy, Debug)]
pub struct DateTime {
    pub year: u32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub weekday: u8, // 0 = Sunday
}

impl DateTime {
    /// Day-of-week name abbreviation
    pub fn weekday_str(&self) -> &'static str {
        match self.weekday {
            0 => "Sun",
            1 => "Mon",
            2 => "Tue",
            3 => "Wed",
            4 => "Thu",
            5 => "Fri",
            6 => "Sat",
            _ => "???",
        }
    }

    /// Month name abbreviation
    pub fn month_str(&self) -> &'static str {
        match self.month {
            1  => "Jan",
            2  => "Feb",
            3  => "Mar",
            4  => "Apr",
            5  => "May",
            6  => "Jun",
            7  => "Jul",
            8  => "Aug",
            9  => "Sep",
            10 => "Oct",
            11 => "Nov",
            12 => "Dec",
            _  => "???",
        }
    }
}

/// Convert a Unix timestamp to a DateTime (UTC).
pub fn unix_to_datetime(ts: u64) -> DateTime {
    let mut secs = ts;

    let second = (secs % 60) as u8;
    secs /= 60;
    let minute = (secs % 60) as u8;
    secs /= 60;
    let hour = (secs % 24) as u8;
    let mut days = (secs / 24) as u32;

    // Day of week: Jan 1, 1970 was Thursday (4)
    let weekday = ((days + 4) % 7) as u8;

    // Walk through years
    let mut year = 1970u32;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    // Walk through months
    let month_days: [u32; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0u8;
    for (i, &md) in month_days.iter().enumerate() {
        if days < md {
            month = (i + 1) as u8;
            break;
        }
        days -= md;
    }
    if month == 0 {
        month = 12;
    }

    let day = days as u8 + 1;

    DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        weekday,
    }
}

/// Read the current wall-clock date/time.
pub fn now() -> DateTime {
    unix_to_datetime(read_unix_timestamp())
}

fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}
