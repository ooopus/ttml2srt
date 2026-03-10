use anyhow::{bail, Result};
use std::fmt;

/// Represents a timestamp with millisecond precision for SRT output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrtTime {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
    pub millis: u32,
}

impl fmt::Display for SrtTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02}:{:02}:{:02},{:03}",
            self.hours, self.minutes, self.seconds, self.millis
        )
    }
}

/// Parse a TTML time expression into an SrtTime.
///
/// Supported formats:
/// - `HH:MM:SS.mmm` / `HH:MM:SS` / `HH:MM:SS.xxx...`
/// - `123.45s` (offset-time in seconds)
pub fn parse_time(s: &str) -> Result<SrtTime> {
    let s = s.trim();

    // Format B: offset in seconds, e.g. "123.45s"
    if let Some(stripped) = s.strip_suffix('s') {
        let secs: f64 = stripped
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid offset-time: {s}"))?;
        return Ok(from_total_millis((secs * 1000.0).round() as u64));
    }

    // Format A: HH:MM:SS[.fraction]
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() != 3 {
        bail!("unsupported time format: {s}");
    }

    let hours: u32 = parts[0]
        .parse()
        .map_err(|_| anyhow::anyhow!("bad hours in: {s}"))?;
    let minutes: u32 = parts[1]
        .parse()
        .map_err(|_| anyhow::anyhow!("bad minutes in: {s}"))?;

    let (seconds, millis) = if let Some((sec_str, frac_str)) = parts[2].split_once('.') {
        let seconds: u32 = sec_str
            .parse()
            .map_err(|_| anyhow::anyhow!("bad seconds in: {s}"))?;

        let millis: u32 = if frac_str.len() <= 3 {
            // Pad with trailing zeros: "5" -> "500", "50" -> "500"
            let padded = format!("{:0<3}", frac_str);
            padded
                .parse()
                .map_err(|_| anyhow::anyhow!("bad fractional seconds in: {s}"))?
        } else {
            // More than 3 digits: parse as float and round
            let full_frac: f64 = format!("0.{frac_str}")
                .parse()
                .map_err(|_| anyhow::anyhow!("bad fractional seconds in: {s}"))?;
            (full_frac * 1000.0).round() as u32
        };

        (seconds, millis.min(999))
    } else {
        let seconds: u32 = parts[2]
            .parse()
            .map_err(|_| anyhow::anyhow!("bad seconds in: {s}"))?;
        (seconds, 0)
    };

    Ok(SrtTime {
        hours,
        minutes,
        seconds,
        millis,
    })
}

fn from_total_millis(ms: u64) -> SrtTime {
    let millis = (ms % 1000) as u32;
    let total_secs = ms / 1000;
    let seconds = (total_secs % 60) as u32;
    let total_mins = total_secs / 60;
    let minutes = (total_mins % 60) as u32;
    let hours = (total_mins / 60) as u32;
    SrtTime {
        hours,
        minutes,
        seconds,
        millis,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hms_millis() {
        let t = parse_time("00:00:03.337").unwrap();
        assert_eq!(t.to_string(), "00:00:03,337");
    }

    #[test]
    fn test_hms_no_millis() {
        let t = parse_time("00:00:03").unwrap();
        assert_eq!(t.to_string(), "00:00:03,000");
    }

    #[test]
    fn test_offset_seconds() {
        let t = parse_time("3.337s").unwrap();
        assert_eq!(t.to_string(), "00:00:03,337");
    }

    #[test]
    fn test_offset_large() {
        let t = parse_time("3661.5s").unwrap();
        assert_eq!(t.to_string(), "01:01:01,500");
    }

    #[test]
    fn test_typical_ttml_time() {
        let t = parse_time("00:07:34.913").unwrap();
        assert_eq!(t.to_string(), "00:07:34,913");
    }

    #[test]
    fn test_short_fraction() {
        let t = parse_time("00:00:01.5").unwrap();
        assert_eq!(t.to_string(), "00:00:01,500");
    }

    #[test]
    fn test_long_fraction() {
        let t = parse_time("00:00:01.5678").unwrap();
        assert_eq!(t.to_string(), "00:00:01,568");
    }
}
