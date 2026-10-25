//! What the machine can afford right now: free memory and power. The model runs
//! only when it does not slow the developer down.

use std::process::Command;

/// Memory the reasoning model needs while it runs (Gemma 4 E2B, measured: 3.7 GB).
pub const REASONING_BYTES: u64 = 4_000_000_000;
/// Memory the fallback needs (Qwen3.5 2B, measured: 3.3 GB).
pub const FALLBACK_BYTES: u64 = 3_500_000_000;
/// Below this charge, on battery, no model is started.
pub const MIN_BATTERY_PERCENT: u32 = 30;

/// What PowerShell prints for a CIM query; the class names, unlike the performance
/// counters' names, are the same in every language of Windows.
fn powershell(command: &str) -> Option<String> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", command])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Memory that can be given to a program without swapping: free, inactive and
/// speculative pages on macOS, `MemAvailable` on Linux, available bytes on Windows.
pub fn available_memory() -> Option<u64> {
    if cfg!(target_os = "macos") {
        let output = Command::new("vm_stat").output().ok()?;
        parse_vm_stat(&String::from_utf8_lossy(&output.stdout))
    } else if cfg!(windows) {
        powershell("(Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory).AvailableBytes")?
            .parse()
            .ok()
    } else {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let line = text
            .lines()
            .find(|line| line.starts_with("MemAvailable:"))?;
        let kilobytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kilobytes * 1024)
    }
}

fn parse_vm_stat(text: &str) -> Option<u64> {
    let page: u64 = text
        .lines()
        .next()?
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |name: &str| {
        text.lines()
            .find(|line| line.starts_with(name))
            .and_then(|line| line.split(':').nth(1))
            .and_then(|value| value.trim().trim_end_matches('.').parse::<u64>().ok())
            .unwrap_or(0)
    };
    Some((pages("Pages free") + pages("Pages inactive") + pages("Pages speculative")) * page)
}

/// The battery's charge when the machine runs on it; `None` on mains power or
/// without a battery.
pub fn on_battery() -> Option<u32> {
    if cfg!(target_os = "macos") {
        let output = Command::new("pmset").args(["-g", "batt"]).output().ok()?;
        parse_pmset(&String::from_utf8_lossy(&output.stdout))
    } else if cfg!(windows) {
        parse_win32_battery(&powershell(
            "Get-CimInstance Win32_Battery | ForEach-Object { \"$($_.BatteryStatus) $($_.EstimatedChargeRemaining)\" }",
        )?)
    } else {
        let supplies = std::fs::read_dir("/sys/class/power_supply").ok()?;
        let read = |dir: &std::path::Path, name: &str| {
            std::fs::read_to_string(dir.join(name)).unwrap_or_default()
        };
        supplies.flatten().find_map(|supply| {
            let dir = supply.path();
            parse_sysfs_battery(
                &read(&dir, "type"),
                &read(&dir, "status"),
                &read(&dir, "capacity"),
            )
        })
    }
}

/// A Linux power supply: a battery that is discharging, and its charge.
fn parse_sysfs_battery(kind: &str, status: &str, capacity: &str) -> Option<u32> {
    if kind.trim() != "Battery" || status.trim() != "Discharging" {
        return None;
    }
    capacity.trim().parse().ok()
}

/// `Win32_Battery`, one line per battery, `status charge`: status 1 is on battery.
fn parse_win32_battery(text: &str) -> Option<u32> {
    text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next()? == "1").then(|| fields.next()?.parse().ok())?
    })
}

fn parse_pmset(text: &str) -> Option<u32> {
    if !text.contains("'Battery Power'") {
        return None;
    }
    let percent = text
        .split('%')
        .next()?
        .rsplit(|c: char| !c.is_ascii_digit())
        .next()?;
    percent.parse().ok()
}

/// Which reasoning model fits now: the main one, the fallback, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reasoning {
    Main,
    Fallback,
    None,
}

pub fn reasoning(available: Option<u64>, battery: Option<u32>) -> Reasoning {
    if battery.is_some_and(|percent| percent < MIN_BATTERY_PERCENT) {
        return Reasoning::None;
    }
    match available {
        Some(bytes) if bytes >= REASONING_BYTES => Reasoning::Main,
        Some(bytes) if bytes >= FALLBACK_BYTES => Reasoning::Fallback,
        _ => Reasoning::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_from_vm_stat() {
        let text = "Mach Virtual Memory Statistics: (page size of 4096 bytes)\nPages free:      104776.\nPages active:   1413783.\nPages inactive: 1339469.\nPages speculative: 24868.\n";
        assert_eq!(
            parse_vm_stat(text),
            Some((104_776 + 1_339_469 + 24_868) * 4096)
        );
    }

    #[test]
    fn battery_from_pmset() {
        assert_eq!(
            parse_pmset("Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t100%; charged;"),
            None
        );
        assert_eq!(
            parse_pmset(
                "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t42%; discharging;"
            ),
            Some(42)
        );
    }

    #[test]
    fn battery_on_linux_and_windows() {
        assert_eq!(
            parse_sysfs_battery("Battery\n", "Discharging\n", "57\n"),
            Some(57)
        );
        assert_eq!(parse_sysfs_battery("Battery\n", "Charging\n", "57\n"), None);
        assert_eq!(parse_sysfs_battery("Mains\n", "", ""), None);
        assert_eq!(parse_win32_battery("1 42"), Some(42));
        assert_eq!(parse_win32_battery("2 100"), None);
        assert_eq!(parse_win32_battery(""), None);
    }

    #[test]
    fn the_model_is_picked_by_what_the_machine_affords() {
        assert_eq!(reasoning(Some(6_000_000_000), None), Reasoning::Main);
        assert_eq!(reasoning(Some(3_600_000_000), None), Reasoning::Fallback);
        assert_eq!(reasoning(Some(2_000_000_000), None), Reasoning::None);
        assert_eq!(reasoning(Some(6_000_000_000), Some(20)), Reasoning::None);
    }
}
