use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    cli::{BatteryState, Device},
    settings::{GuiSettings, KnownDevice},
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CachedBattery {
    pub percentage: Option<u8>,
    pub charging: Option<bool>,
}

impl From<BatteryState> for CachedBattery {
    fn from(value: BatteryState) -> Self {
        Self {
            percentage: value.percentage.map(|percentage| percentage.min(100)),
            charging: value.charging,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MonitorState {
    pub active_devices: Vec<Device>,
    pub tray_target: Option<String>,
    pub batteries: HashMap<String, CachedBattery>,
    pub last_refresh_unix_seconds: Option<u64>,
    pub poll_in_progress: bool,
}

impl MonitorState {
    pub fn reconcile_devices(&mut self, settings: &mut GuiSettings, devices: Vec<Device>) -> bool {
        let previous_target = self.tray_target.clone();
        let now = unix_seconds();
        let active_ids = devices
            .iter()
            .map(|device| device.id.as_str())
            .collect::<HashSet<_>>();
        self.batteries
            .retain(|id, _| active_ids.contains(id.as_str()));

        for device in &devices {
            if let Some(known) = settings
                .known_devices
                .iter_mut()
                .find(|known| known.id == device.id)
            {
                known.name.clone_from(&device.name);
                known.model_key.clone_from(&device.model_key);
                known.last_connection.clone_from(&device.connection);
                known.last_seen_unix_seconds = Some(now);
            } else {
                settings.known_devices.push(KnownDevice {
                    id: device.id.clone(),
                    name: device.name.clone(),
                    model_key: device.model_key.clone(),
                    last_connection: device.connection.clone(),
                    last_seen_unix_seconds: Some(now),
                    device_type: None,
                });
            }
        }
        settings.normalize();
        self.active_devices = devices;
        self.last_refresh_unix_seconds = Some(now);
        self.reevaluate_priority(settings);
        self.tray_target != previous_target
    }

    pub fn reevaluate_priority(&mut self, settings: &GuiSettings) {
        let active = self
            .active_devices
            .iter()
            .map(|device| device.id.as_str())
            .collect::<HashSet<_>>();
        self.tray_target = settings
            .device_priority
            .iter()
            .find(|id| active.contains(id.as_str()))
            .cloned();
    }

    pub fn mark_device_not_found(&mut self, settings: &GuiSettings, device_id: &str) {
        self.active_devices.retain(|device| device.id != device_id);
        self.batteries.remove(device_id);
        self.reevaluate_priority(settings);
    }

    pub fn cache_battery(&mut self, device_id: &str, battery: BatteryState) {
        self.batteries.insert(device_id.to_owned(), battery.into());
    }

    pub fn tray_battery(&self) -> Option<&CachedBattery> {
        self.tray_target
            .as_ref()
            .and_then(|id| self.batteries.get(id))
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollSchedule {
    interval_seconds: u64,
    in_flight: bool,
    pending: bool,
}

#[cfg(test)]
impl PollSchedule {
    pub const fn new(interval_seconds: u64) -> Self {
        Self {
            interval_seconds,
            in_flight: false,
            pending: false,
        }
    }

    pub const fn interval_seconds(self) -> u64 {
        self.interval_seconds
    }

    pub fn set_interval(&mut self, interval_seconds: u64) {
        self.interval_seconds = interval_seconds;
    }

    pub fn request(&mut self) -> bool {
        if self.in_flight {
            self.pending = true;
            false
        } else {
            self.in_flight = true;
            true
        }
    }

    pub fn finish(&mut self) -> bool {
        self.in_flight = false;
        if self.pending {
            self.pending = false;
            self.in_flight = true;
            true
        } else {
            false
        }
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str) -> Device {
        Device {
            id: id.to_owned(),
            name: id.to_owned(),
            model_key: "model".to_owned(),
            connection: "wired".to_owned(),
        }
    }

    fn settings() -> GuiSettings {
        GuiSettings {
            device_priority: vec!["A".into(), "B".into(), "C".into()],
            known_devices: ["A", "B", "C"]
                .into_iter()
                .map(|id| KnownDevice {
                    id: id.to_owned(),
                    name: id.to_owned(),
                    model_key: "model".to_owned(),
                    last_connection: "wired".to_owned(),
                    last_seen_unix_seconds: None,
                    device_type: None,
                })
                .collect(),
            ..GuiSettings::default()
        }
    }

    #[test]
    fn priority_always_selects_first_active_device() {
        let mut state = MonitorState::default();
        let mut settings = settings();
        state.reconcile_devices(&mut settings, vec![device("B"), device("C")]);
        assert_eq!(state.tray_target.as_deref(), Some("B"));
        state.reconcile_devices(&mut settings, vec![device("A"), device("B"), device("C")]);
        assert_eq!(state.tray_target.as_deref(), Some("A"));
        state.reconcile_devices(&mut settings, vec![device("B"), device("C")]);
        assert_eq!(state.tray_target.as_deref(), Some("B"));
        state.reconcile_devices(&mut settings, vec![]);
        assert_eq!(state.tray_target, None);
    }

    #[test]
    fn device_not_found_removes_live_battery_and_selects_fallback() {
        let mut settings = settings();
        let mut state = MonitorState::default();
        state.reconcile_devices(&mut settings, vec![device("A"), device("B")]);
        state.cache_battery(
            "A",
            BatteryState {
                percentage: Some(150),
                charging: Some(true),
            },
        );
        assert_eq!(state.tray_battery().unwrap().percentage, Some(100));
        assert_eq!(state.tray_battery().unwrap().charging, Some(true));
        state.mark_device_not_found(&settings, "A");
        assert_eq!(state.tray_target.as_deref(), Some("B"));
        assert!(!state.batteries.contains_key("A"));
    }

    #[test]
    fn newly_active_higher_priority_target_requests_new_battery() {
        let mut settings = settings();
        let mut state = MonitorState::default();
        assert!(state.reconcile_devices(&mut settings, vec![device("B")]));
        assert!(!state.reconcile_devices(&mut settings, vec![device("B")]));
        assert!(state.reconcile_devices(&mut settings, vec![device("A"), device("B")]));
        assert_eq!(state.tray_target.as_deref(), Some("A"));
    }

    #[test]
    fn discovery_preserves_cached_percentage_and_charging_state() {
        let mut settings = settings();
        let mut state = MonitorState::default();
        state.reconcile_devices(&mut settings, vec![device("A")]);
        state.cache_battery(
            "A",
            BatteryState {
                percentage: Some(88),
                charging: Some(true),
            },
        );

        state.reconcile_devices(&mut settings, vec![device("A")]);

        assert_eq!(state.tray_battery().unwrap().percentage, Some(88));
        assert_eq!(state.tray_battery().unwrap().charging, Some(true));
    }

    #[test]
    fn scheduling_is_immediate_serialized_and_interval_driven() {
        let mut schedule = PollSchedule::new(60);
        assert_eq!(schedule.interval_seconds(), 60);
        assert!(schedule.request());
        assert!(!schedule.request());
        assert!(schedule.finish());
        assert!(!schedule.finish());
        schedule.set_interval(15);
        assert_eq!(schedule.interval_seconds(), 15);
        assert_ne!(schedule.interval_seconds(), 5);
    }
}
