// Real-Time Clock (RTC) Driver
//
// Reads the current date and time from the CMOS RTC chip.
// The RTC is accessed through I/O ports 0x70 (index) and 0x71 (data).
//
// CMOS RTC Register Map:
//   0x00 — Seconds          0x04 — Hours (0-23 or 1-12 with AM/PM)
//   0x02 — Minutes          0x07 — Day of Month
//   0x06 — Day of Week      0x08 — Month
//   0x09 — Year (0-99)      0x32 — Century (if available)
//   0x0A — Status Register A (update-in-progress bit)
//   0x0B — Status Register B (data format flags)
//
// Values can be in BCD or binary format depending on Status Register B bit 2.
// We handle both formats transparently.
//
// In Windows NT, the RTC is accessed through the HAL via
// HalQueryRealTimeClock / HalSetRealTimeClock.

use super::port::{inb, outb};

/// CMOS I/O ports
const CMOS_INDEX: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

/// CMOS register indices
const RTC_SECONDS: u8 = 0x00;
const RTC_MINUTES: u8 = 0x02;
const RTC_HOURS: u8 = 0x04;
const RTC_DAY: u8 = 0x07;
const RTC_MONTH: u8 = 0x08;
const RTC_YEAR: u8 = 0x09;
const RTC_CENTURY: u8 = 0x32;
const RTC_STATUS_A: u8 = 0x0A;
const RTC_STATUS_B: u8 = 0x0B;

/// Date and time structure
#[derive(Debug, Clone, Copy)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl DateTime {
    /// Format as "YYYY-MM-DD HH:MM:SS"
    pub fn format<'a>(&self, buf: &'a mut [u8; 20]) -> &'a str {
        let digits = |n: u16, b: &mut [u8]| {
            for (i, byte) in b.iter_mut().rev().enumerate() {
                *byte = b'0' + ((n / 10u16.pow(i as u32)) % 10) as u8;
            }
        };
        digits(self.year, &mut buf[0..4]);
        buf[4] = b'-';
        digits(self.month as u16, &mut buf[5..7]);
        buf[7] = b'-';
        digits(self.day as u16, &mut buf[8..10]);
        buf[10] = b' ';
        digits(self.hour as u16, &mut buf[11..13]);
        buf[13] = b':';
        digits(self.minute as u16, &mut buf[14..16]);
        buf[16] = b':';
        digits(self.second as u16, &mut buf[17..19]);
        buf[19] = 0;
        core::str::from_utf8(&buf[..19]).unwrap_or("????-??-?? ??:??:??")
    }
}

/// Read a single CMOS register.
///
/// NMI is preserved by masking bit 7 of the index port.
unsafe fn cmos_read(register: u8) -> u8 {
    // Bit 7 of port 0x70 controls NMI — we keep it clear (NMI enabled)
    outb(CMOS_INDEX, register & 0x7F);
    inb(CMOS_DATA)
}

/// Check if the RTC is currently updating (update-in-progress flag).
/// We should NOT read time registers while this is set.
unsafe fn rtc_updating() -> bool {
    outb(CMOS_INDEX, RTC_STATUS_A & 0x7F);
    inb(CMOS_DATA) & 0x80 != 0
}

/// Convert a BCD-encoded byte to binary.
fn bcd_to_binary(bcd: u8) -> u8 {
    (bcd & 0x0F) + ((bcd >> 4) * 10)
}

/// Read the current date and time from the CMOS RTC.
///
/// Uses double-read technique: read twice and compare to avoid getting
/// inconsistent values during an RTC update cycle.
pub fn read_datetime() -> DateTime {
    unsafe {
        // Wait until RTC is not updating
        while rtc_updating() {
            core::hint::spin_loop();
        }

        let mut second = cmos_read(RTC_SECONDS);
        let mut minute = cmos_read(RTC_MINUTES);
        let mut hour = cmos_read(RTC_HOURS);
        let mut day = cmos_read(RTC_DAY);
        let mut month = cmos_read(RTC_MONTH);
        let mut year = cmos_read(RTC_YEAR);
        let mut century = cmos_read(RTC_CENTURY);

        // Double-read: read again and compare to ensure consistency
        loop {
            while rtc_updating() {
                core::hint::spin_loop();
            }

            let s2 = cmos_read(RTC_SECONDS);
            let m2 = cmos_read(RTC_MINUTES);
            let h2 = cmos_read(RTC_HOURS);
            let d2 = cmos_read(RTC_DAY);
            let mo2 = cmos_read(RTC_MONTH);
            let y2 = cmos_read(RTC_YEAR);
            let c2 = cmos_read(RTC_CENTURY);

            if second == s2 && minute == m2 && hour == h2
                && day == d2 && month == mo2 && year == y2 && century == c2
            {
                break;
            }

            second = s2;
            minute = m2;
            hour = h2;
            day = d2;
            month = mo2;
            year = y2;
            century = c2;
        }

        // Check Status Register B for data format
        let status_b = cmos_read(RTC_STATUS_B);
        let is_binary = status_b & 0x04 != 0;
        let is_24h = status_b & 0x02 != 0;

        if !is_binary {
            // Convert BCD to binary
            second = bcd_to_binary(second);
            minute = bcd_to_binary(minute);
            hour = bcd_to_binary(hour & 0x7F); // mask PM bit
            day = bcd_to_binary(day);
            month = bcd_to_binary(month);
            year = bcd_to_binary(year);
            century = bcd_to_binary(century);
        }

        // Handle 12-hour format
        if !is_24h && (hour & 0x80) != 0 {
            hour = ((hour & 0x7F) + 12) % 24;
        }

        // Build full year
        let full_year = if century > 0 {
            (century as u16) * 100 + year as u16
        } else {
            // Assume 2000s if century register reads 0
            2000 + year as u16
        };

        DateTime {
            year: full_year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }
}
