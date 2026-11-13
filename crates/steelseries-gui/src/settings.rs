use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub const DEFAULT_REFRESH_INTERVAL_SECONDS: u64 = 60;
pub const REFRESH_INTERVAL_OPTIONS: [u64; 5] = [15, 30, 60, 120, 300];
const AUTOSTART_FILE_NAME: &str = "steelseries-linux-gui.desktop";
const LEGACY_AUTOSTART_ENTRY: &str = "[Desktop Entry]\nType=Application\nName=SteelSeries Linux\nExec=steelseries-gui --autostart\nTerminal=false\n";
const AUTOSTART_DESKTOP_ENTRY: &str = "[Desktop Entry]\nType=Application\nName=SteelSeries Linux\nExec=steelseries-linux-gui --autostart\nIcon=steelseries-linux-gui\nTerminal=false\n";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayIconTheme {
    Colored,
    #[default]
    Monochrome,
}

impl TrayIconTheme {
    pub const fn index(self) -> i32 {
        match self {
            Self::Colored => 0,
            Self::Monochrome => 1,
        }
    }

    pub const fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Colored),
            1 => Some(Self::Monochrome),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiSettings {
    pub tray_enabled: bool,
    pub tray_icon_theme: TrayIconTheme,
    pub refresh_interval_seconds: u64,
    pub start_at_login: bool,
    pub device_priority: Vec<String>,
    pub known_devices: Vec<KnownDevice>,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            tray_enabled: true,
            tray_icon_theme: TrayIconTheme::Monochrome,
            refresh_interval_seconds: DEFAULT_REFRESH_INTERVAL_SECONDS,
            start_at_login: false,
            device_priority: Vec::new(),
            known_devices: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownDevice {
    pub id: String,
    pub name: String,
    pub model_key: String,
    pub last_connection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_unix_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_type: Option<String>,
}

impl GuiSettings {
    pub fn normalize(&mut self) {
        if !REFRESH_INTERVAL_OPTIONS.contains(&self.refresh_interval_seconds) {
            self.refresh_interval_seconds = DEFAULT_REFRESH_INTERVAL_SECONDS;
        }
        self.known_devices
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.known_devices
            .dedup_by(|left, right| left.id == right.id);
        self.device_priority
            .retain(|id| self.known_devices.iter().any(|device| &device.id == id));
        for device in &self.known_devices {
            if !self.device_priority.contains(&device.id) {
                self.device_priority.push(device.id.clone());
            }
        }
    }

    pub fn load(path: &Path) -> Self {
        let Ok(contents) = fs::read(path) else {
            return Self::default();
        };
        let mut settings = serde_json::from_slice::<Self>(&contents).unwrap_or_default();
        settings.normalize();
        settings
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        fs::write(&temporary, bytes)?;
        fs::rename(temporary, path)
    }
}

pub fn settings_path() -> PathBuf {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(path)
            .join("steelseries-linux")
            .join("gui-settings.json");
    }
    env::var_os("HOME").map_or_else(
        || PathBuf::from(".steelseries-linux-gui-settings.json"),
        |home| {
            PathBuf::from(home)
                .join(".config")
                .join("steelseries-linux")
                .join("gui-settings.json")
        },
    )
}

pub fn autostart_path() -> PathBuf {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return autostart_path_in(Path::new(&path));
    }
    env::var_os("HOME").map_or_else(
        || PathBuf::from(AUTOSTART_FILE_NAME),
        |home| autostart_path_in(&PathBuf::from(home).join(".config")),
    )
}

fn autostart_path_in(config_home: &Path) -> PathBuf {
    config_home.join("autostart").join(AUTOSTART_FILE_NAME)
}

pub fn set_autostart_enabled(path: &Path, enabled: bool) -> io::Result<()> {
    if enabled {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, AUTOSTART_DESKTOP_ENTRY)
    } else {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

/// Migrate only the exact entry generated by our previous version.
pub fn migrate_autostart(path: &Path, enabled: bool) -> io::Result<()> {
    let legacy = path.with_file_name("steelseries-linux.desktop");
    match fs::read_to_string(&legacy) {
        Ok(contents) if contents == LEGACY_AUTOSTART_ENTRY => {
            if enabled {
                set_autostart_enabled(path, true)?;
            }
            fs::remove_file(legacy)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_file(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("steelseries-gui-{name}-{nonce}.json"))
    }

    fn device(id: &str) -> KnownDevice {
        KnownDevice {
            id: id.to_owned(),
            name: format!("Device {id}"),
            model_key: "model".to_owned(),
            last_connection: "wired".to_owned(),
            last_seen_unix_seconds: Some(42),
            device_type: None,
        }
    }

    #[test]
    fn defaults_enable_tray_and_use_sixty_seconds() {
        let settings = GuiSettings::default();
        assert!(settings.tray_enabled);
        assert_eq!(settings.tray_icon_theme, TrayIconTheme::Monochrome);
        assert_eq!(settings.refresh_interval_seconds, 60);
        assert!(!settings.start_at_login);
    }

    #[test]
    fn tray_icon_theme_round_trips_and_old_settings_default_to_monochrome() {
        let path = temporary_file("tray-theme");
        let settings = GuiSettings {
            tray_icon_theme: TrayIconTheme::Colored,
            ..GuiSettings::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(
            GuiSettings::load(&path).tray_icon_theme,
            TrayIconTheme::Colored
        );

        fs::write(
            &path,
            r#"{"tray_enabled":true,"refresh_interval_seconds":60}"#,
        )
        .unwrap();
        assert_eq!(
            GuiSettings::load(&path).tray_icon_theme,
            TrayIconTheme::Monochrome
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn custom_interval_priority_and_known_devices_round_trip() {
        let path = temporary_file("round-trip");
        let settings = GuiSettings {
            refresh_interval_seconds: 120,
            known_devices: vec![device("A"), device("B")],
            device_priority: vec!["B".to_owned(), "A".to_owned()],
            ..GuiSettings::default()
        };
        settings.save(&path).unwrap();
        let loaded = GuiSettings::load(&path);
        assert_eq!(loaded.refresh_interval_seconds, 120);
        assert_eq!(loaded.device_priority, ["B", "A"]);
        assert_eq!(loaded.known_devices.len(), 2);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn invalid_interval_falls_back_without_losing_valid_registry() {
        let path = temporary_file("invalid-interval");
        fs::write(
            &path,
            r#"{"tray_enabled":false,"refresh_interval_seconds":0,"device_priority":["A"],"known_devices":[{"id":"A","name":"Device A","model_key":"model","last_connection":"wired"}]}"#,
        )
        .unwrap();
        let loaded = GuiSettings::load(&path);
        assert!(!loaded.tray_enabled);
        assert_eq!(loaded.refresh_interval_seconds, 60);
        assert_eq!(loaded.device_priority, ["A"]);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_config_does_not_crash() {
        let path = temporary_file("malformed");
        fs::write(&path, b"not-json").unwrap();
        assert_eq!(GuiSettings::load(&path), GuiSettings::default());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn autostart_entry_uses_installed_executable_and_can_be_removed() {
        let config_home = temporary_file("autostart-root");
        let path = autostart_path_in(&config_home);

        set_autostart_enabled(&path, true).unwrap();
        let entry = fs::read_to_string(&path).unwrap();
        assert_eq!(entry, AUTOSTART_DESKTOP_ENTRY);
        assert!(entry.contains("Exec=steelseries-linux-gui --autostart"));
        assert!(!entry.contains("target/debug"));

        set_autostart_enabled(&path, false).unwrap();
        assert!(!path.exists());
        set_autostart_enabled(&path, false).unwrap();
        let _ = fs::remove_dir_all(config_home);
    }

    #[test]
    fn migrates_managed_autostart_without_duplicate_entry() {
        let config_home = temporary_file("autostart-migration");
        let path = autostart_path_in(&config_home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let legacy = path.with_file_name("steelseries-linux.desktop");
        fs::write(&legacy, LEGACY_AUTOSTART_ENTRY).unwrap();
        migrate_autostart(&path, true).unwrap();
        assert!(!legacy.exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), AUTOSTART_DESKTOP_ENTRY);
        fs::write(&legacy, "user customized entry").unwrap();
        migrate_autostart(&path, true).unwrap();
        assert_eq!(
            fs::read_to_string(&legacy).unwrap(),
            "user customized entry"
        );
        fs::remove_dir_all(config_home).unwrap();
    }
}
