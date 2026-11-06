use crate::cli::{DeviceDescription, DeviceSnapshot, LodValueCapability};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardRow {
    pub label: String,
    pub value: String,
    pub detail: String,
    pub active: bool,
    pub x: u16,
    pub y: u16,
    pub separate: bool,
    pub raw_value: String,
    pub persisted: bool,
    pub option_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardView {
    pub device_name: String,
    pub connection: String,
    pub connection_key: String,
    pub device_id: String,
    pub model_key: String,
    pub show_battery: bool,
    pub battery: String,
    pub battery_percentage: Option<u8>,
    pub battery_charging: bool,
    pub show_dpi: bool,
    pub active_dpi: String,
    pub dpi_capabilities: String,
    pub dpi_independent_axes: bool,
    pub dpi_min: u16,
    pub dpi_max: u16,
    pub dpi_step: u16,
    pub dpi_max_stages: usize,
    pub dpi_rows: Vec<DashboardRow>,
    pub show_lod: bool,
    pub active_lod: String,
    pub lod_capabilities: String,
    pub lod_rows: Vec<DashboardRow>,
    pub lod_options: Vec<String>,
    pub show_polling: bool,
    pub show_wireless_polling: bool,
    pub show_wired_polling: bool,
    pub wireless_polling: String,
    pub wired_polling: String,
    pub wireless_polling_hz: u16,
    pub wired_polling_hz: u16,
    pub wireless_polling_primary: bool,
    pub wired_polling_primary: bool,
    pub polling_capabilities: String,
    pub wireless_polling_options: Vec<String>,
    pub wired_polling_options: Vec<String>,
    pub wireless_polling_index: usize,
    pub wired_polling_index: usize,
    pub show_power: bool,
    pub show_low_power_mode: bool,
    pub show_low_power_polling: bool,
    pub show_sleep_timer: bool,
    pub show_auto_low_power: bool,
    pub low_power_mode: String,
    pub low_power_enabled: bool,
    pub low_power_polling: String,
    pub low_power_polling_hz: u16,
    pub low_power_polling_options: Vec<String>,
    pub low_power_polling_index: usize,
    pub sleep_timer: String,
    pub sleep_timer_edit: u32,
    pub sleep_timer_current: u32,
    pub sleep_timer_min: u64,
    pub sleep_timer_max: u64,
    pub sleep_timer_step: u64,
    pub sleep_timer_real_min: u64,
    pub sleep_timer_real_max: u64,
    pub auto_low_power: String,
    pub auto_low_power_enabled: bool,
    pub auto_low_power_threshold: String,
    pub auto_low_power_threshold_edit: u8,
    pub auto_low_power_threshold_current: u8,
    pub auto_low_power_threshold_min: u8,
    pub auto_low_power_threshold_max: u8,
    pub auto_low_power_threshold_step: u8,
    pub show_wireless_stability: bool,
    pub wireless_stability: String,
    pub wireless_stability_enabled: bool,
    pub show_bluetooth_smoothing: bool,
    pub bluetooth_smoothing: String,
    pub bluetooth_smoothing_enabled: bool,
    pub show_scroll_jump: bool,
    pub scroll_jump: String,
    pub scroll_jump_enabled: bool,
    pub scroll_jump_delay: String,
    pub scroll_jump_delay_edit: u16,
    pub scroll_jump_delay_current: u16,
    pub scroll_jump_capabilities: String,
    pub scroll_jump_delay_min: u16,
    pub scroll_jump_delay_max: u16,
    pub scroll_jump_delay_step: u16,
    pub scroll_jump_delay_unit: String,
}

impl DashboardView {
    pub fn new(description: DeviceDescription, snapshot: DeviceSnapshot) -> Self {
        let capabilities = &description.capabilities;
        let active_stage = snapshot.dpi.active_stage;
        let active_dpi = snapshot
            .dpi
            .stages
            .iter()
            .find(|stage| stage.stage == active_stage)
            .map_or_else(
                || "Unavailable".to_owned(),
                |stage| dpi_value(stage.x, stage.y),
            );
        let mut dpi_rows = snapshot
            .dpi
            .stages
            .iter()
            .map(|stage| DashboardRow {
                label: format!("Stage {}", stage.stage),
                value: dpi_value(stage.x, stage.y),
                detail: String::new(),
                active: stage.stage == active_stage,
                x: stage.x,
                y: stage.y,
                separate: stage.x != stage.y,
                raw_value: String::new(),
                persisted: true,
                option_index: 0,
            })
            .collect::<Vec<_>>();
        let lod_rows = snapshot
            .lod
            .stages
            .iter()
            .map(|stage| {
                let metadata = lod_metadata(&capabilities.lod.values, &stage.lod);
                DashboardRow {
                    label: format!("Stage {}", stage.stage),
                    value: metadata.map_or_else(|| stage.lod.clone(), |value| value.label.clone()),
                    detail: metadata
                        .map(|value| format!("{} mm", value.distance_mm))
                        .unwrap_or_default(),
                    active: stage.stage == active_stage,
                    x: 0,
                    y: 0,
                    separate: false,
                    raw_value: stage.lod.clone(),
                    persisted: true,
                    option_index: capabilities
                        .lod
                        .values
                        .iter()
                        .position(|value| value.value == stage.lod)
                        .unwrap_or(0),
                }
            })
            .collect::<Vec<_>>();
        for (dpi_row, lod_row) in dpi_rows.iter_mut().zip(&lod_rows) {
            dpi_row.raw_value.clone_from(&lod_row.raw_value);
            dpi_row.option_index = lod_row.option_index;
            dpi_row.detail.clone_from(&lod_row.detail);
        }
        let active_lod = lod_rows.iter().find(|row| row.active).map_or_else(
            || "Unavailable".to_owned(),
            |row| {
                if row.detail.is_empty() {
                    row.value.clone()
                } else {
                    format!("{} · {}", row.value, row.detail)
                }
            },
        );
        let battery = if capabilities.battery.supported {
            match snapshot.battery.percentage {
                Some(percentage) => {
                    if capabilities.battery.charging_state
                        && snapshot.battery.charging == Some(true)
                    {
                        format!("{percentage}% · Charging")
                    } else {
                        format!("{percentage}%")
                    }
                }
                None => "Battery unavailable".to_owned(),
            }
        } else {
            String::new()
        };
        let connection_key = snapshot.device.connection.clone();
        let scroll_delay = &capabilities.scroll_jump.delay;
        let sleep_timer = &capabilities.power.sleep_timer;
        let sleep_recommended_min = sleep_timer
            .recommended_range
            .as_ref()
            .map_or(sleep_timer.min_minutes, |range| range.min_minutes);
        let sleep_recommended_max = sleep_timer
            .recommended_range
            .as_ref()
            .map_or(sleep_timer.max_minutes, |range| range.max_minutes);
        let sleep_timer_value = snapshot
            .power
            .sleep_timer_minutes
            .unwrap_or_else(|| u32::try_from(sleep_recommended_min).unwrap_or(u32::MAX));
        let sleep_timer_edit = u32::try_from(
            u64::from(sleep_timer_value).clamp(sleep_recommended_min, sleep_recommended_max),
        )
        .unwrap_or(u32::MAX);

        Self {
            device_name: snapshot.device.name,
            connection: connection_label(&connection_key).to_owned(),
            connection_key: connection_key.clone(),
            device_id: snapshot.device.id,
            model_key: snapshot.device.model_key,
            show_battery: capabilities.battery.supported,
            battery,
            battery_percentage: snapshot.battery.percentage,
            battery_charging: snapshot.battery.charging == Some(true),
            show_dpi: capabilities.dpi.supported,
            active_dpi,
            dpi_capabilities: dpi_capability_text(&capabilities.dpi),
            dpi_independent_axes: capabilities.dpi.independent_axes,
            dpi_min: capabilities.dpi.min,
            dpi_max: capabilities.dpi.max,
            dpi_step: capabilities.dpi.step,
            dpi_max_stages: capabilities.dpi.max_stages,
            dpi_rows,
            show_lod: capabilities.lod.supported,
            active_lod,
            lod_capabilities: if capabilities.lod.per_stage {
                "Configured per DPI stage".to_owned()
            } else {
                "Shared across stages".to_owned()
            },
            lod_rows,
            lod_options: capabilities
                .lod
                .values
                .iter()
                .map(|value| format!("{} — {} mm", value.label, value.distance_mm))
                .collect(),
            show_polling: capabilities.polling.supported,
            show_wireless_polling: !capabilities.polling.modes.wireless.rates_hz.is_empty(),
            show_wired_polling: !capabilities.polling.modes.wired.rates_hz.is_empty(),
            wireless_polling: format!("{} Hz", snapshot.polling.wireless_hz),
            wired_polling: format!("{} Hz", snapshot.polling.wired_hz),
            wireless_polling_hz: snapshot.polling.wireless_hz,
            wired_polling_hz: snapshot.polling.wired_hz,
            wireless_polling_primary: connection_key == "2.4_ghz",
            wired_polling_primary: connection_key == "wired",
            polling_capabilities: format!(
                "Wireless: {} Hz · Wired: {} Hz",
                rates_text(&capabilities.polling.modes.wireless.rates_hz),
                rates_text(&capabilities.polling.modes.wired.rates_hz)
            ),
            wireless_polling_options: rate_options(&capabilities.polling.modes.wireless.rates_hz),
            wired_polling_options: rate_options(&capabilities.polling.modes.wired.rates_hz),
            wireless_polling_index: option_index(
                &capabilities.polling.modes.wireless.rates_hz,
                snapshot.polling.wireless_hz,
            ),
            wired_polling_index: option_index(
                &capabilities.polling.modes.wired.rates_hz,
                snapshot.polling.wired_hz,
            ),
            show_power: capabilities.power.supported,
            show_low_power_mode: capabilities.power.low_power_mode,
            show_low_power_polling: !capabilities.power.low_power_polling.rates_hz.is_empty(),
            show_sleep_timer: capabilities.power.sleep_timer.supported,
            show_auto_low_power: capabilities.power.auto_low_power.supported,
            low_power_mode: on_off(snapshot.power.low_power_enabled).to_owned(),
            low_power_enabled: snapshot.power.low_power_enabled,
            low_power_polling: format!("{} Hz", snapshot.power.low_power_polling_hz),
            low_power_polling_hz: snapshot.power.low_power_polling_hz,
            low_power_polling_options: rate_options(&capabilities.power.low_power_polling.rates_hz),
            low_power_polling_index: option_index(
                &capabilities.power.low_power_polling.rates_hz,
                snapshot.power.low_power_polling_hz,
            ),
            sleep_timer: snapshot.power.sleep_timer_minutes.map_or_else(
                || format!("{} ms", snapshot.power.sleep_timer_ms),
                |minutes| format!("{minutes} min"),
            ),
            sleep_timer_edit,
            sleep_timer_current: sleep_timer_value,
            sleep_timer_min: sleep_recommended_min,
            sleep_timer_max: sleep_recommended_max,
            sleep_timer_step: sleep_timer.step_minutes,
            sleep_timer_real_min: sleep_timer.min_minutes,
            sleep_timer_real_max: sleep_timer.max_minutes,
            auto_low_power: on_off(snapshot.power.auto_low_power_enabled).to_owned(),
            auto_low_power_enabled: snapshot.power.auto_low_power_enabled,
            auto_low_power_threshold: format!(
                "{}%",
                snapshot.power.auto_low_power_threshold_percent
            ),
            auto_low_power_threshold_edit: snapshot.power.auto_low_power_threshold_percent,
            auto_low_power_threshold_current: snapshot.power.auto_low_power_threshold_percent,
            auto_low_power_threshold_min: capabilities.power.auto_low_power.threshold.min,
            auto_low_power_threshold_max: capabilities.power.auto_low_power.threshold.max,
            auto_low_power_threshold_step: capabilities.power.auto_low_power.threshold.step,
            show_wireless_stability: capabilities.wireless_stability.supported,
            wireless_stability: on_off(snapshot.wireless_stability.enabled).to_owned(),
            wireless_stability_enabled: snapshot.wireless_stability.enabled,
            show_bluetooth_smoothing: capabilities.bluetooth_smoothing.supported,
            bluetooth_smoothing: on_off(snapshot.bluetooth_smoothing.enabled).to_owned(),
            bluetooth_smoothing_enabled: snapshot.bluetooth_smoothing.enabled,
            show_scroll_jump: capabilities.scroll_jump.supported,
            scroll_jump: on_off(snapshot.scroll_jump.enabled).to_owned(),
            scroll_jump_enabled: snapshot.scroll_jump.enabled,
            scroll_jump_delay: format!("{} ms", snapshot.scroll_jump.delay_ms),
            scroll_jump_delay_edit: snapshot.scroll_jump.delay_ms,
            scroll_jump_delay_current: snapshot.scroll_jump.delay_ms,
            scroll_jump_capabilities: format!(
                "{}–{} {}, step {} {}",
                scroll_delay.min,
                scroll_delay.max,
                scroll_delay.unit,
                scroll_delay.step,
                scroll_delay.unit
            ),
            scroll_jump_delay_min: scroll_delay.min,
            scroll_jump_delay_max: scroll_delay.max,
            scroll_jump_delay_step: scroll_delay.step,
            scroll_jump_delay_unit: scroll_delay.unit.clone(),
        }
    }
}

fn lod_metadata<'a>(
    values: &'a [LodValueCapability],
    current: &str,
) -> Option<&'a LodValueCapability> {
    values.iter().find(|value| value.value == current)
}

fn dpi_value(x: u16, y: u16) -> String {
    if x == y {
        format!("{x} DPI")
    } else {
        format!("X {x} / Y {y} DPI")
    }
}

fn dpi_capability_text(capability: &crate::cli::DpiCapabilities) -> String {
    let axes = if capability.independent_axes {
        " · independent X/Y"
    } else {
        ""
    };
    format!(
        "{}–{} DPI · step {} · up to {} stages{}",
        capability.min, capability.max, capability.step, capability.max_stages, axes
    )
}

fn rates_text(rates: &[u16]) -> String {
    rates
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

fn rate_options(rates: &[u16]) -> Vec<String> {
    rates.iter().map(|rate| format!("{rate} Hz")).collect()
}

fn option_index(rates: &[u16], current: u16) -> usize {
    rates.iter().position(|rate| *rate == current).unwrap_or(0)
}

fn connection_label(connection: &str) -> &str {
    match connection {
        "wired" => "Wired",
        "2.4_ghz" => "2.4 GHz",
        value => value,
    }
}

const fn on_off(enabled: bool) -> &'static str {
    if enabled { "On" } else { "Off" }
}

#[cfg(test)]
mod tests {
    use crate::cli::{
        AutoLowPowerCapabilities, BatteryCapabilities, BatteryState, Capabilities, Device,
        DeviceDescription, DeviceSnapshot, DpiCapabilities, DpiStageState, DpiState, EnabledState,
        LodCapabilities, LodStageState, LodState, LodValueCapability, PollingCapabilities,
        PollingMode, PollingModes, PollingState, PowerCapabilities, PowerState,
        ScrollJumpCapabilities, ScrollJumpState, SleepTimerCapabilities, SupportedCapability,
        UnitRange,
    };

    use super::*;

    fn dashboard_with(
        battery_supported: bool,
        percentage: Option<u8>,
        independent: bool,
    ) -> DashboardView {
        let device = Device {
            connection: "wired".to_owned(),
            id: "AAA".to_owned(),
            model_key: "test_model".to_owned(),
            name: "Test Mouse".to_owned(),
        };
        let description = DeviceDescription {
            device: device.clone(),
            capabilities: Capabilities {
                dpi: DpiCapabilities {
                    supported: true,
                    min: 50,
                    max: 26_000,
                    step: 50,
                    max_stages: 5,
                    independent_axes: independent,
                },
                lod: LodCapabilities {
                    supported: true,
                    per_stage: true,
                    values: vec![
                        LodValueCapability {
                            value: "low".to_owned(),
                            label: "Near".to_owned(),
                            distance_mm: 1,
                        },
                        LodValueCapability {
                            value: "high".to_owned(),
                            label: "Far".to_owned(),
                            distance_mm: 2,
                        },
                    ],
                },
                polling: PollingCapabilities {
                    supported: true,
                    modes: PollingModes {
                        wireless: PollingMode {
                            rates_hz: vec![125, 1000, 4000],
                        },
                        wired: PollingMode {
                            rates_hz: vec![125, 1000],
                        },
                    },
                },
                battery: BatteryCapabilities {
                    supported: battery_supported,
                    charging_state: true,
                },
                power: PowerCapabilities {
                    supported: true,
                    low_power_mode: true,
                    low_power_polling: PollingMode {
                        rates_hz: vec![125, 250, 500],
                    },
                    sleep_timer: SleepTimerCapabilities {
                        supported: true,
                        min_minutes: 1,
                        max_minutes: 71_582,
                        step_minutes: 1,
                        recommended_range: Some(crate::cli::RecommendedRange {
                            min_minutes: 5,
                            max_minutes: 20,
                        }),
                    },
                    auto_low_power: AutoLowPowerCapabilities {
                        supported: true,
                        threshold: UnitRange {
                            min: 5,
                            max: 25,
                            step: 1,
                            unit: "percent".to_owned(),
                        },
                    },
                },
                wireless_stability: SupportedCapability { supported: true },
                bluetooth_smoothing: SupportedCapability { supported: false },
                scroll_jump: ScrollJumpCapabilities {
                    supported: true,
                    delay: UnitRange {
                        min: 100,
                        max: 1500,
                        step: 100,
                        unit: "ms".to_owned(),
                    },
                },
            },
        };
        let snapshot = DeviceSnapshot {
            device,
            battery: BatteryState {
                percentage,
                charging: percentage.map(|_| true),
            },
            dpi: DpiState {
                active_stage: 2,
                stages: vec![
                    DpiStageState {
                        stage: 1,
                        x: 400,
                        y: 400,
                    },
                    DpiStageState {
                        stage: 2,
                        x: 800,
                        y: 1600,
                    },
                ],
            },
            lod: LodState {
                stages: vec![
                    LodStageState {
                        stage: 1,
                        lod: "low".to_owned(),
                    },
                    LodStageState {
                        stage: 2,
                        lod: "high".to_owned(),
                    },
                ],
            },
            polling: PollingState {
                wireless_hz: 4000,
                wired_hz: 1000,
            },
            power: PowerState {
                low_power_enabled: false,
                low_power_polling_hz: 500,
                sleep_timer_minutes: Some(30),
                sleep_timer_ms: 1_800_000,
                auto_low_power_enabled: true,
                auto_low_power_threshold_percent: 15,
            },
            wireless_stability: EnabledState { enabled: true },
            bluetooth_smoothing: EnabledState { enabled: false },
            scroll_jump: ScrollJumpState {
                enabled: true,
                delay_ms: 500,
            },
        };
        DashboardView::new(description, snapshot)
    }

    #[test]
    fn battery_visibility_and_null_state_follow_capabilities() {
        let available = dashboard_with(true, Some(82), true);
        assert!(available.show_battery);
        assert_eq!(available.battery, "82% · Charging");
        let unavailable = dashboard_with(true, None, true);
        assert_eq!(unavailable.battery, "Battery unavailable");
        let unsupported = dashboard_with(false, None, true);
        assert!(!unsupported.show_battery);
        assert!(unsupported.battery.is_empty());
    }

    #[test]
    fn independent_axes_and_asymmetric_dpi_are_preserved() {
        let independent = dashboard_with(true, Some(82), true);
        assert!(independent.dpi_independent_axes);
        assert_eq!(independent.active_dpi, "X 800 / Y 1600 DPI");
        assert!(independent.dpi_capabilities.contains("independent X/Y"));
        assert!(independent.dpi_rows.iter().all(|row| row.persisted));
        let shared = dashboard_with(true, Some(82), false);
        assert!(!shared.dpi_independent_axes);
        assert!(!shared.dpi_capabilities.contains("independent X/Y"));
    }

    #[test]
    fn lod_labels_and_distances_come_from_metadata() {
        let view = dashboard_with(true, Some(82), true);
        assert_eq!(view.lod_rows[0].value, "Near");
        assert_eq!(view.lod_rows[0].detail, "1 mm");
        assert_eq!(view.active_lod, "Far · 2 mm");
        assert_eq!(view.lod_options, ["Near — 1 mm", "Far — 2 mm"]);
        assert_eq!(view.lod_rows[0].option_index, 0);
        assert_eq!(view.lod_rows[1].option_index, 1);
        assert_eq!(view.dpi_rows[0].raw_value, "low");
        assert_eq!(view.dpi_rows[0].option_index, 0);
        assert_eq!(view.dpi_rows[1].raw_value, "high");
        assert_eq!(view.dpi_rows[1].option_index, 1);
    }

    #[test]
    fn feature_visibility_is_capability_driven() {
        let view = dashboard_with(true, Some(82), true);
        assert!(view.show_wireless_stability);
        assert!(!view.show_bluetooth_smoothing);
        assert!(view.show_scroll_jump);
        assert!(view.show_power);
    }

    #[test]
    fn polling_and_power_state_are_formatted_without_model_checks() {
        let view = dashboard_with(true, Some(82), true);
        assert_eq!(view.wireless_polling, "4000 Hz");
        assert_eq!(view.wired_polling, "1000 Hz");
        assert!(view.wired_polling_primary);
        assert!(!view.wireless_polling_primary);
        assert_eq!(view.low_power_mode, "Off");
        assert_eq!(view.sleep_timer, "30 min");
        assert_eq!(view.auto_low_power, "On");
        assert_eq!(view.auto_low_power_threshold, "15%");
        assert_eq!(
            view.wireless_polling_options,
            ["125 Hz", "1000 Hz", "4000 Hz"]
        );
        assert_eq!(view.wired_polling_options, ["125 Hz", "1000 Hz"]);
        assert_eq!(view.wireless_polling_index, 2);
        assert_eq!(view.wired_polling_index, 1);
        assert_eq!(
            view.low_power_polling_options,
            ["125 Hz", "250 Hz", "500 Hz"]
        );
        assert_eq!(view.low_power_polling_index, 2);
        assert_eq!(
            (
                view.sleep_timer_min,
                view.sleep_timer_max,
                view.sleep_timer_step
            ),
            (5, 20, 1)
        );
        assert_eq!(
            (view.sleep_timer_real_min, view.sleep_timer_real_max),
            (1, 71_582)
        );
        assert_eq!((view.sleep_timer_edit, view.sleep_timer_current), (20, 30));
        assert_eq!(
            (
                view.auto_low_power_threshold_min,
                view.auto_low_power_threshold_max,
                view.auto_low_power_threshold_step
            ),
            (5, 25, 1)
        );
    }

    #[test]
    fn wireless_and_scroll_jump_state_and_limits_are_retained() {
        let view = dashboard_with(true, Some(82), true);
        assert_eq!(view.wireless_stability, "On");
        assert_eq!(view.bluetooth_smoothing, "Off");
        assert_eq!(view.scroll_jump, "On");
        assert_eq!(view.scroll_jump_delay, "500 ms");
        assert_eq!(
            (
                view.scroll_jump_delay_min,
                view.scroll_jump_delay_max,
                view.scroll_jump_delay_step
            ),
            (100, 1500, 100)
        );
        assert_eq!(view.scroll_jump_delay_unit, "ms");
    }
}
