use std::{fmt, time::Duration};

const MILLISECOND: Duration = Duration::from_millis(1);
const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(60 * 60);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const YEAR: Duration = Duration::from_secs(365 * 24 * 60 * 60);

const UNITS: &[(Duration, &str, &str)] = &[
    (YEAR, "year", "y"),
    (WEEK, "week", "w"),
    (DAY, "day", "d"),
    (HOUR, "hour", "h"),
    (MINUTE, "minute", "m"),
    (SECOND, "second", "s"),
    (MILLISECOND, "millisecond", "ms"),
];

#[derive(Debug)]
pub struct HumanDuration(pub Duration);

impl fmt::Display for HumanDuration {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        let secs = self.0.as_secs();
        let nanos = self.0.subsec_nanos();

        if secs == 0 && nanos == 0 {
            return if f.alternate() {
                write!(f, "0s")
            } else {
                write!(f, "0 seconds")
            };
        }

        if f.alternate() {
            let total_secs = self.0.as_secs_f64();

            if total_secs < 0.1 && total_secs > 0.0 {
                let ms = (total_secs * 1000.0).round() as u64;
                if ms > 0 {
                    return write!(f, "{}ms", ms);
                }
            }

            if total_secs < 60.0 {
                if total_secs == 0.0 {
                    return write!(f, "0s");
                }
                // < 1m: 0.1s, 1.5s, 35s
                let s = format!("{:.1}", total_secs);
                return write!(f, "{}{}", s.trim_end_matches(".0"), "s");
            }

            // >= 1m: 1h1m, 1m40s
            let mut remaining_secs = total_secs.round() as u64;
            let mut units_printed = 0;

            for &(unit, _, alt) in UNITS {
                let unit_secs = unit.as_secs();
                if unit_secs >= 60 && remaining_secs >= unit_secs {
                    let val = remaining_secs / unit_secs;
                    write!(f, "{}{}", val, alt)?;
                    remaining_secs %= unit_secs;
                    units_printed += 1;
                    if units_printed >= 2 {
                        break;
                    }
                } else if unit_secs == 1 && units_printed == 1 && remaining_secs > 0 {
                    write!(f, "{}{}", remaining_secs, alt)?;
                    break;
                }
            }
            Ok(())
        } else {
            for &(unit, name, _) in UNITS {
                if self.0 >= unit {
                    let val = (self.0.as_secs_f64() / unit.as_secs_f64()).round() as u64;
                    if val == 1 {
                        return write!(f, "1 {}", name);
                    }
                    return write!(f, "{} {}s", val, name);
                }
            }
            let ms = nanos / 1_000_000;
            write!(
                f,
                "{} millisecond{}",
                ms,
                if ms == 1 {
                    ""
                } else {
                    "s"
                }
            )
        }
    }
}
