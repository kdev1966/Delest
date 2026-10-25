//! Alerts and notices, shown in the agent's session and as a system notification.
//! They must stay rare to stay read: an alert at most once a day unless the disk got
//! worse, a notice at most once a week.

use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::index::Totals;
use crate::lang::{self, Lang};
use crate::tr;
use crate::volume::{Level, Space};

const FILE: &str = "alertes.json";
const DAY: i64 = 86_400;
pub const ALERT_EVERY: i64 = DAY;
pub const NOTICE_EVERY: i64 = 7 * DAY;

/// When each kind of message was last shown.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Shown {
    #[serde(default)]
    pub alert_at: Option<i64>,
    #[serde(default)]
    pub notice_at: Option<i64>,
    /// Free space at the last alert, to tell when the disk got worse.
    #[serde(default)]
    pub alert_available: Option<u64>,
}

impl Shown {
    /// A broken file only resets the rate limit: it holds no decision of the developer.
    pub fn load(data_dir: &Path) -> Shown {
        fs::read_to_string(data_dir.join(FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> io::Result<()> {
        fs::create_dir_all(data_dir)?;
        let json = serde_json::to_string(self).map_err(io::Error::other)?;
        fs::write(data_dir.join(FILE), json)
    }
}

/// Whether a message of this level is due now. An alert comes back before its day
/// is over when free space dropped by another tenth since the last one.
pub fn due(level: Level, space: Space, shown: &Shown, now: i64) -> bool {
    let elapsed = |time: Option<i64>, every: i64| time.is_none_or(|time| now - time >= every);
    match level {
        Level::Fine => false,
        Level::Notice => {
            elapsed(shown.notice_at, NOTICE_EVERY) && elapsed(shown.alert_at, ALERT_EVERY)
        }
        Level::Alert => {
            let worse = shown
                .alert_available
                .is_some_and(|before| space.available < before - before / 10);
            elapsed(shown.alert_at, ALERT_EVERY) || worse
        }
    }
}

pub fn record(level: Level, space: Space, shown: &mut Shown, now: i64) {
    match level {
        Level::Fine => {}
        Level::Notice => shown.notice_at = Some(now),
        Level::Alert => {
            shown.alert_at = Some(now);
            shown.alert_available = Some(space.available);
        }
    }
}

/// The message, in the developer's language, with what to do next.
pub fn message(level: Level, space: Space, totals: Totals, lang: Lang) -> String {
    let available = lang::size(space.available, lang);
    let freeable = lang::size(totals.rebuildable + totals.online, lang);
    match level {
        Level::Fine => String::new(),
        Level::Notice => tr!(
            lang,
            "Delest: {freeable} could be freed (build folders, caches). See it with: delest clean --dry-run",
            "Delest : {freeable} peuvent être libérés (dossiers de compilation, caches). Aperçu : delest clean --dry-run"
        ),
        Level::Alert => tr!(
            lang,
            "Delest: only {available} free on the disk; {freeable} could be freed. See it with: delest clean --dry-run",
            "Delest : plus que {available} libres sur le disque ; {freeable} peuvent être libérés. Aperçu : delest clean --dry-run"
        ),
    }
}

/// Turns system notifications off: `DELEST_NOTIFY=0`; the message still reaches the session.
pub const ENV_NOTIFY: &str = "DELEST_NOTIFY";

/// A Windows notification, through PowerShell and the system's own API, with no
/// module to install. The text comes in `DELEST_TEXT`, never inside the script.
/// Windows shows a toast only for a known application id: Delest's own once the app
/// is installed (its Start menu entry carries it), else PowerShell's. The node list
/// is read with `Item()`: PowerShell cannot index it (checked on Windows 11).
const TOAST: &str = r#"$null = [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]
$xml = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)
$texts = $xml.GetElementsByTagName('text')
$null = $texts.Item(0).AppendChild($xml.CreateTextNode('Delest'))
$null = $texts.Item(1).AppendChild($xml.CreateTextNode($env:DELEST_TEXT))
$id = '{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe'
if (Get-StartApps -Name 'Delest' | Where-Object AppID -eq 'dev.kdev1966.delest') { $id = 'dev.kdev1966.delest' }
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($id).Show([Windows.UI.Notifications.ToastNotification]::new($xml))"#;

/// The program that shows system notifications here, if the system has one.
pub fn notifier() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("osascript")
    } else if cfg!(windows) {
        Some("PowerShell")
    } else if cfg!(unix) {
        crate::paths::find_program("notify-send").map(|_| "notify-send")
    } else {
        None
    }
}

/// Shows a system notification; `false` when turned off or when the system has no way to.
pub fn notify(text: &str) -> bool {
    if std::env::var_os(ENV_NOTIFY).is_some_and(|value| value == "0") {
        return false;
    }
    let command = if cfg!(target_os = "macos") {
        let script = format!(
            "display notification \"{}\" with title \"Delest\"",
            text.replace('\\', "\\\\").replace('"', "\\\"")
        );
        Command::new("osascript").args(["-e", &script]).output()
    } else if cfg!(windows) {
        Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", TOAST])
            .env("DELEST_TEXT", text)
            .output()
    } else if cfg!(unix) {
        Command::new("notify-send").args(["Delest", text]).output()
    } else {
        return false;
    };
    command.is_ok_and(|output| output.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn space(available: u64) -> Space {
        Space {
            total: 500 * GB,
            available: available * GB,
        }
    }

    #[test]
    fn an_alert_comes_once_a_day_unless_the_disk_got_worse() {
        let mut shown = Shown::default();
        assert!(due(Level::Alert, space(40), &shown, 0));
        record(Level::Alert, space(40), &mut shown, 0);
        assert!(!due(Level::Alert, space(39), &shown, 3_600));
        assert!(due(Level::Alert, space(35), &shown, 3_600));
        assert!(due(Level::Alert, space(40), &shown, DAY));
    }

    #[test]
    fn a_notice_comes_once_a_week_and_never_right_after_an_alert() {
        let mut shown = Shown::default();
        assert!(due(Level::Notice, space(80), &shown, 0));
        record(Level::Notice, space(80), &mut shown, 0);
        assert!(!due(Level::Notice, space(80), &shown, 6 * DAY));
        assert!(due(Level::Notice, space(80), &shown, 7 * DAY));
        record(Level::Alert, space(40), &mut shown, 7 * DAY);
        assert!(!due(Level::Notice, space(80), &shown, 7 * DAY + 3_600));
        assert!(!due(Level::Fine, space(80), &shown, 30 * DAY));
    }

    #[test]
    fn messages_say_what_to_do() {
        let totals = Totals {
            rebuildable: 3 * GB,
            online: 2 * GB,
            to_review: 0,
        };
        let text = message(Level::Alert, space(15), totals, Lang::Fr);
        assert!(
            text.contains("15 Go")
                && text.contains("5,0 Go")
                && text.contains("delest clean --dry-run"),
            "{text}"
        );
        assert!(message(Level::Fine, space(80), totals, Lang::En).is_empty());
    }
}
