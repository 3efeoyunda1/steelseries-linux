use std::{collections::HashMap, time::Duration};

use crate::{cli::BatteryState, settings::GuiSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warning {
    Low,
    Critical,
}

#[derive(Default)]
pub struct BatteryNotifications {
    // Keep history across disconnects: reconnecting below an already reported
    // threshold does not count as battery recovery.
    devices: HashMap<String, [bool; 2]>,
}

impl BatteryNotifications {
    pub fn observe(
        &mut self,
        id: &str,
        battery: &BatteryState,
        settings: &GuiSettings,
    ) -> Vec<Warning> {
        let Some(percentage) = battery.percentage.filter(|value| *value <= 100) else {
            return Vec::new();
        };
        let sent = self.devices.entry(id.to_owned()).or_default();
        let mut warnings = Vec::new();
        for (index, (threshold, warning)) in [
            (settings.low_battery_threshold, Warning::Low),
            (settings.critical_battery_threshold, Warning::Critical),
        ]
        .into_iter()
        .enumerate()
        {
            if percentage > threshold {
                sent[index] = false;
            } else if settings.battery_notifications
                && battery.charging == Some(false)
                && !sent[index]
            {
                sent[index] = true;
                warnings.push(warning);
            }
        }
        warnings
    }
}

pub fn send(name: &str, percentage: u8, warning: Warning) -> zbus::Result<()> {
    let connection = zbus::blocking::connection::Builder::session()?
        .method_timeout(Duration::from_secs(2))
        .build()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )?;
    let (label, urgency) = match warning {
        Warning::Low => ("Low battery", 1u8),
        Warning::Critical => ("Critical battery", 2u8),
    };
    let hints = HashMap::from([("urgency", zbus::zvariant::Value::from(urgency))]);
    let _: u32 = proxy.call(
        "Notify",
        &(
            "SteelSeries Linux",
            0u32,
            "steelseries-linux-gui",
            format!("{label}: {name} ({percentage}%)"),
            "",
            Vec::<&str>::new(),
            hints,
            -1i32,
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn battery(percentage: Option<u8>, charging: Option<bool>) -> BatteryState {
        BatteryState {
            percentage,
            charging,
        }
    }

    #[test]
    fn thresholds_are_independent_and_rearm_after_recovery_per_device() {
        let mut state = BatteryNotifications::default();
        let settings = GuiSettings::default();
        let mut observe =
            |id, value| state.observe(id, &battery(Some(value), Some(false)), &settings);
        assert_eq!(observe("A", 25), [Warning::Low]);
        assert!(observe("A", 19).is_empty());
        assert_eq!(observe("A", 10), [Warning::Critical]);
        assert!(observe("A", 5).is_empty());
        assert_eq!(observe("B", 5), [Warning::Low, Warning::Critical]);
        assert!(observe("A", 15).is_empty());
        assert_eq!(observe("A", 10), [Warning::Critical]);
        assert!(observe("A", 26).is_empty());
        assert_eq!(observe("A", 25), [Warning::Low]);
    }

    #[test]
    fn custom_thresholds_and_recovery_while_charging_are_observed() {
        let mut state = BatteryNotifications::default();
        let settings = GuiSettings {
            low_battery_threshold: 30,
            critical_battery_threshold: 15,
            ..GuiSettings::default()
        };
        assert_eq!(
            state.observe("A", &battery(Some(30), Some(false)), &settings),
            [Warning::Low]
        );
        assert_eq!(
            state.observe("A", &battery(Some(15), Some(false)), &settings),
            [Warning::Critical]
        );
        assert!(
            state
                .observe("A", &battery(Some(80), Some(true)), &settings)
                .is_empty()
        );
        assert_eq!(
            state.observe("A", &battery(Some(10), Some(false)), &settings),
            [Warning::Low, Warning::Critical]
        );
    }

    #[test]
    fn invalid_unknown_charging_and_disabled_samples_do_not_trigger() {
        let mut state = BatteryNotifications::default();
        let mut settings = GuiSettings::default();
        for sample in [
            battery(None, Some(false)),
            battery(Some(101), Some(false)),
            battery(Some(5), Some(true)),
            battery(Some(5), None),
        ] {
            assert!(state.observe("A", &sample, &settings).is_empty());
        }
        settings.battery_notifications = false;
        assert!(
            state
                .observe("A", &battery(Some(5), Some(false)), &settings)
                .is_empty()
        );
        settings.battery_notifications = true;
        assert_eq!(
            state.observe("A", &battery(Some(5), Some(false)), &settings),
            [Warning::Low, Warning::Critical]
        );
    }
}
