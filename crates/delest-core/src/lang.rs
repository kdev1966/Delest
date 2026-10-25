//! The language of the text shown to people.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;

/// Overrides the detected language: `DELEST_LANG=fr`.
pub const ENV_LANG: &str = "DELEST_LANG";
/// Delest's own override, then the POSIX precedence for messages.
const VARIABLES: [&str; 5] = [ENV_LANG, "LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"];
/// The language the developer chose in the settings, in Delest's data folder.
const CHOICE: &str = "langue";

/// The text in the user's language: `tr!(lang, "Done: {count}.", "Terminé : {count}.")`.
/// Both literals may use the same inline arguments.
#[macro_export]
macro_rules! tr {
    ($lang:expr, $en:literal, $fr:literal) => {
        match $lang {
            $crate::lang::Lang::En => format!($en),
            $crate::lang::Lang::Fr => format!($fr),
        }
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    /// `fr_FR.UTF-8` and `fr-TN` give French; `de:fr` gives the first supported one;
    /// `C` or an unsupported language gives `None`.
    pub fn parse(value: &str) -> Option<Lang> {
        value.split(':').find_map(|part| {
            let code = part.trim().split(['_', '.', '@', '-']).next()?;
            match code.to_lowercase().as_str() {
                "en" => Some(Lang::En),
                "fr" => Some(Lang::Fr),
                _ => None,
            }
        })
    }

    /// The language set in the environment, if any.
    pub fn from_vars(var: impl Fn(&str) -> Option<OsString>) -> Option<Lang> {
        VARIABLES
            .iter()
            .filter_map(|name| var(name))
            .find_map(|value| Lang::parse(&value.to_string_lossy()))
    }

    /// `DELEST_LANG`, else the language chosen in the settings, else the system's.
    pub fn detect() -> Lang {
        env::var_os(ENV_LANG)
            .and_then(|value| Lang::parse(&value.to_string_lossy()))
            .or_else(|| chosen(&crate::paths::data_dir()))
            .unwrap_or_else(Lang::system)
    }

    /// The environment's language, else the system's preferred one, else English.
    /// macOS terminals often leave `LANG` unset, hence the system lookup.
    pub fn system() -> Lang {
        Lang::from_vars(|name| env::var_os(name))
            .or_else(|| sys_locale::get_locales().find_map(|locale| Lang::parse(&locale)))
            .unwrap_or_default()
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        }
    }
}

/// The language chosen in the settings; `None` follows the system.
pub fn chosen(data_dir: &Path) -> Option<Lang> {
    let text = fs::read_to_string(data_dir.join(CHOICE)).ok()?;
    Lang::parse(text.trim())
}

/// Keeps the developer's choice; `None` goes back to the system's language.
pub fn choose(data_dir: &Path, lang: Option<Lang>) -> io::Result<()> {
    let file = data_dir.join(CHOICE);
    match lang {
        Some(lang) => {
            fs::create_dir_all(data_dir)?;
            fs::write(file, lang.code())
        }
        None => match fs::remove_file(file) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        },
    }
}

/// How long ago, in days: "today", "1 day ago", "3 days ago".
pub fn ago(days: u64, lang: Lang) -> String {
    match days {
        0 => tr!(lang, "today", "aujourd'hui"),
        1 => tr!(lang, "1 day ago", "il y a 1 jour"),
        _ => tr!(lang, "{days} days ago", "il y a {days} jours"),
    }
}

/// How long ago, from seconds: "just now", "12 min ago", "3 h ago", then in days.
pub fn since(seconds: i64, lang: Lang) -> String {
    match seconds {
        s if s < 60 => tr!(lang, "just now", "à l'instant"),
        s if s < 3_600 => {
            let minutes = s / 60;
            tr!(lang, "{minutes} min ago", "il y a {minutes} min")
        }
        s if s < 86_400 => {
            let hours = s / 3_600;
            tr!(lang, "{hours} h ago", "il y a {hours} h")
        }
        s => ago((s / 86_400) as u64, lang),
    }
}

/// A size in decimal units, as the Finder and most disk tools show it.
pub fn size(bytes: u64, lang: Lang) -> String {
    let (value, unit_en, unit_fr) = match bytes {
        b if b >= 1_000_000_000 => (b as f64 / 1e9, "GB", "Go"),
        b if b >= 1_000_000 => (b as f64 / 1e6, "MB", "Mo"),
        b => (b as f64 / 1e3, "kB", "ko"),
    };
    let decimals = if value < 10.0 && bytes >= 1_000_000_000 {
        1
    } else {
        0
    };
    let number = format!("{value:.decimals$}");
    match lang {
        Lang::En => format!("{number} {unit_en}"),
        Lang::Fr => format!("{} {unit_fr}", number.replace('.', ",")),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn a_recent_time_is_said_in_minutes_or_hours() {
        assert_eq!(since(20, Lang::Fr), "à l'instant");
        assert_eq!(since(12 * 60, Lang::Fr), "il y a 12 min");
        assert_eq!(since(3 * 3_600 + 5, Lang::En), "3 h ago");
        assert_eq!(since(86_400, Lang::Fr), "il y a 1 jour");
    }

    #[test]
    fn the_chosen_language_is_kept_until_the_system_is_chosen_again() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("delest");
        assert_eq!(chosen(&data), None);
        choose(&data, Some(Lang::Fr)).unwrap();
        assert_eq!(chosen(&data), Some(Lang::Fr));
        choose(&data, None).unwrap();
        assert_eq!(chosen(&data), None);
        choose(&data, None).unwrap();
    }

    #[test]
    fn parse_locales() {
        assert_eq!(Lang::parse("fr_FR.UTF-8"), Some(Lang::Fr));
        assert_eq!(Lang::parse("fr-TN"), Some(Lang::Fr));
        assert_eq!(Lang::parse("de:fr"), Some(Lang::Fr));
        assert_eq!(Lang::parse("en_US"), Some(Lang::En));
        assert_eq!(Lang::parse("C"), None);
        assert_eq!(Lang::parse(""), None);
    }

    #[test]
    fn the_override_comes_first_and_unsupported_values_are_skipped() {
        let mut vars: HashMap<&str, OsString> = HashMap::new();
        let lookup =
            |vars: &HashMap<&str, OsString>| Lang::from_vars(|name| vars.get(name).cloned());
        assert_eq!(lookup(&vars), None);
        vars.insert("LC_ALL", "C".into());
        vars.insert("LANG", "fr_FR.UTF-8".into());
        assert_eq!(lookup(&vars), Some(Lang::Fr));
        vars.insert(ENV_LANG, "en".into());
        assert_eq!(lookup(&vars), Some(Lang::En));
    }

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(size(14_070_000_000, Lang::Fr), "14 Go");
        assert_eq!(size(3_540_000_000, Lang::Fr), "3,5 Go");
        assert_eq!(size(3_540_000_000, Lang::En), "3.5 GB");
        assert_eq!(size(196_000_000, Lang::En), "196 MB");
        assert_eq!(size(4_000, Lang::Fr), "4 ko");
    }
}
