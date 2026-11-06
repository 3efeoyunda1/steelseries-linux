use serde::Serialize;
use serde_json::{Value, json};
use steelseries_core::{
    Aerox3WirelessGen2, BatteryStatus, ConnectionType, DeviceCapabilities, DpiConfig, Error,
    LiftOffDistance, PhysicalDevice, PollingConfig, PowerConfig, ScrollJumpConfig,
    WirelessFeatures,
};

pub const SCHEMA_VERSION: u8 = 1;

#[derive(Debug)]
pub struct CommandOutput {
    command: &'static str,
    human: String,
    data: Value,
}

impl CommandOutput {
    pub fn new<T>(command: &'static str, human: impl Into<String>, data: T) -> Self
    where
        T: Serialize,
    {
        Self {
            command,
            human: human.into(),
            data: serde_json::to_value(data).expect("typed CLI response must serialize"),
        }
    }

    pub fn write(self, json: bool) {
        if json {
            println!("{}", self.json_document());
        } else {
            println!("{}", self.human);
        }
    }

    #[cfg(test)]
    pub fn human(&self) -> &str {
        &self.human
    }

    pub fn json_document(&self) -> String {
        serde_json::to_string(&SuccessEnvelope {
            schema_version: SCHEMA_VERSION,
            ok: true,
            command: self.command,
            data: &self.data,
        })
        .expect("typed CLI response envelope must serialize")
    }
}

#[derive(Serialize)]
struct SuccessEnvelope<'a> {
    schema_version: u8,
    ok: bool,
    command: &'a str,
    data: &'a Value,
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    schema_version: u8,
    ok: bool,
    command: &'a str,
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'static str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

pub fn error_document(command: &str, error: &Error) -> String {
    let message = error.to_string();
    let (field, details) = error_metadata(error);
    error_document_with_code(command, error_code(error), &message, field, details)
}

pub fn invalid_argument_document(command: &str, message: &str) -> String {
    error_document_with_code(command, "invalid_argument", message, None, None)
}

fn error_document_with_code(
    command: &str,
    code: &'static str,
    message: &str,
    field: Option<String>,
    details: Option<Value>,
) -> String {
    serde_json::to_string(&ErrorEnvelope {
        schema_version: SCHEMA_VERSION,
        ok: false,
        command,
        error: ErrorBody {
            code,
            message,
            field,
            details,
        },
    })
    .expect("typed CLI error envelope must serialize")
}

fn error_code(error: &Error) -> &'static str {
    match error {
        Error::InvalidDpiStageCount(_)
        | Error::InvalidDpiValue { .. }
        | Error::InvalidDpiActiveIndex { .. }
        | Error::InvalidStageId(_)
        | Error::StageNotConfigured { .. }
        | Error::UnsupportedPollingRate(_)
        | Error::WiredPollingTooHigh { .. }
        | Error::UnsupportedLowPowerPollingRate(_)
        | Error::InvalidAutoLowPowerThreshold(_)
        | Error::SleepTimerZero
        | Error::SleepTimerTooLarge
        | Error::InvalidScrollJumpDelay(_) => "invalid_argument",
        Error::DeviceNotConnected
        | Error::InterfaceNotFound
        | Error::RequestedDeviceNotFound { .. }
        | Error::ReceiverLinkUnavailable => "device_not_found",
        Error::ReadTimeout { .. } => "timeout",
        Error::HidInitialization(_)
        | Error::Open(_)
        | Error::OpenPermissionDenied { .. }
        | Error::Write { .. }
        | Error::ShortWrite { .. }
        | Error::Read { .. } => "device_io",
        Error::MalformedResponse { .. }
        | Error::UnexpectedCommand { .. }
        | Error::UnknownLiftOffDistance(_)
        | Error::UnknownPollingCode(_)
        | Error::UnknownLowPowerPollingCode(_)
        | Error::InvalidPowerBoolean { .. }
        | Error::InvalidSleepTimerMilliseconds(_)
        | Error::InvalidWirelessFeatureBoolean { .. }
        | Error::InvalidScrollJumpEnabled(_)
        | Error::InvalidBatteryChargingState(_)
        | Error::InvalidBatteryPercentage(_)
        | Error::EmptyDeviceIdentity
        | Error::UnterminatedDeviceIdentity
        | Error::InvalidDeviceIdentity
        | Error::InvalidReceiverLinkState(_) => "protocol_error",
        Error::MultipleDevices { .. } => "command_failed",
    }
}

fn error_metadata(error: &Error) -> (Option<String>, Option<Value>) {
    let capabilities = Aerox3WirelessGen2::capabilities();
    match error {
        Error::InvalidDpiStageCount(value) => (
            Some("stages".to_owned()),
            Some(json!({
                "value": value,
                "min_items": 1,
                "max_items": capabilities.dpi.max_stages,
            })),
        ),
        Error::InvalidDpiValue { axis, value, stage } => {
            let axis_name = axis.to_ascii_lowercase();
            let field = stage.map_or_else(
                || format!("dpi.{axis_name}"),
                |stage| format!("stages[{}].{axis_name}", stage - 1),
            );
            (
                Some(field),
                Some(json!({
                    "axis": axis_name,
                    "stage": stage,
                    "value": value,
                    "min": capabilities.dpi.min,
                    "max": capabilities.dpi.max,
                    "step": capabilities.dpi.step,
                })),
            )
        }
        Error::UnsupportedPollingRate(value) => (
            Some("wireless_hz".to_owned()),
            Some(json!({
                "value": value,
                "allowed": capabilities.polling.wireless.iter().map(|rate| rate.hz()).collect::<Vec<_>>(),
            })),
        ),
        Error::WiredPollingTooHigh { value } => (
            Some("wired_hz".to_owned()),
            Some(json!({
                "value": value,
                "allowed": capabilities.polling.wired.iter().map(|rate| rate.hz()).collect::<Vec<_>>(),
            })),
        ),
        Error::UnsupportedLowPowerPollingRate(value) => (
            Some("low_power_polling_hz".to_owned()),
            Some(json!({
                "value": value,
                "allowed": capabilities.power.low_power_polling.iter().map(|rate| rate.hz()).collect::<Vec<_>>(),
            })),
        ),
        Error::InvalidAutoLowPowerThreshold(value) => (
            Some("auto_low_power_threshold_percent".to_owned()),
            Some(json!({
                "value": value,
                "min": capabilities.power.auto_low_power_threshold.min,
                "max": capabilities.power.auto_low_power_threshold.max,
                "step": capabilities.power.auto_low_power_threshold.step,
            })),
        ),
        Error::SleepTimerZero | Error::SleepTimerTooLarge => (
            Some("sleep_timer_minutes".to_owned()),
            Some(json!({
                "min": capabilities.power.sleep_timer.min,
                "max": capabilities.power.sleep_timer.max,
                "step": capabilities.power.sleep_timer.step,
            })),
        ),
        Error::InvalidScrollJumpDelay(value) => (
            Some("delay_ms".to_owned()),
            Some(json!({
                "value": value,
                "min": capabilities.scroll_jump.delay.min,
                "max": capabilities.scroll_jump.delay.max,
                "step": capabilities.scroll_jump.delay.step,
            })),
        ),
        _ => (None, None),
    }
}

#[derive(Debug, Serialize)]
pub struct HelpData<'a> {
    pub help: &'a str,
}

#[derive(Debug, Serialize)]
pub struct DevicesData {
    pub devices: Vec<DeviceData>,
}

#[derive(Debug, Serialize)]
pub struct DeviceData {
    pub id: String,
    pub name: &'static str,
    pub model_key: &'static str,
    pub connection: &'static str,
}

impl DevicesData {
    pub fn from_devices(devices: &[PhysicalDevice]) -> Self {
        Self {
            devices: devices.iter().map(DeviceData::from_device).collect(),
        }
    }
}

impl DeviceData {
    pub fn from_device(device: &PhysicalDevice) -> Self {
        Self {
            id: device.identity.to_string(),
            name: match device.model {
                steelseries_core::DeviceModel::Aerox3WirelessGen2 => "Aerox 3 Wireless Gen 2",
            },
            model_key: device.model.key(),
            connection: match device.active_connection {
                ConnectionType::Wired => "wired",
                ConnectionType::Wireless2_4Ghz => "2.4_ghz",
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DeviceDescribeData {
    pub device: DeviceData,
    pub capabilities: CapabilitiesData,
}

impl DeviceDescribeData {
    pub fn new(device: &PhysicalDevice, capabilities: &DeviceCapabilities) -> Self {
        Self {
            device: DeviceData::from_device(device),
            capabilities: CapabilitiesData::from(capabilities),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CapabilitiesData {
    pub dpi: DpiCapabilitiesData,
    pub lod: LodCapabilitiesData,
    pub polling: PollingCapabilitiesData,
    pub battery: BatteryCapabilitiesData,
    pub power: PowerCapabilitiesData,
    pub wireless_stability: SupportedData,
    pub bluetooth_smoothing: SupportedData,
    pub scroll_jump: ScrollJumpCapabilitiesData,
}

impl From<&DeviceCapabilities> for CapabilitiesData {
    fn from(capabilities: &DeviceCapabilities) -> Self {
        Self {
            dpi: DpiCapabilitiesData {
                supported: capabilities.dpi.supported,
                min: capabilities.dpi.min,
                max: capabilities.dpi.max,
                step: capabilities.dpi.step,
                max_stages: capabilities.dpi.max_stages,
                independent_axes: capabilities.dpi.independent_axes,
            },
            lod: LodCapabilitiesData {
                supported: capabilities.lod.supported,
                per_stage: capabilities.lod.per_stage,
                values: capabilities
                    .lod
                    .values
                    .iter()
                    .map(|value| LodValueData {
                        value: lod_name(value.value),
                        label: value.label,
                        distance_mm: value.distance_mm,
                    })
                    .collect(),
            },
            polling: PollingCapabilitiesData {
                supported: capabilities.polling.supported,
                modes: PollingModesData {
                    wireless: PollingModeData {
                        rates_hz: capabilities
                            .polling
                            .wireless
                            .iter()
                            .map(|rate| rate.hz())
                            .collect(),
                    },
                    wired: PollingModeData {
                        rates_hz: capabilities
                            .polling
                            .wired
                            .iter()
                            .map(|rate| rate.hz())
                            .collect(),
                    },
                },
            },
            battery: BatteryCapabilitiesData {
                supported: capabilities.battery.supported,
                charging_state: capabilities.battery.charging_state,
            },
            power: PowerCapabilitiesData {
                supported: capabilities.power.supported,
                low_power_mode: capabilities.power.low_power_mode,
                low_power_polling: PollingModeData {
                    rates_hz: capabilities
                        .power
                        .low_power_polling
                        .iter()
                        .map(|rate| rate.hz())
                        .collect(),
                },
                sleep_timer: SleepTimerCapabilitiesData {
                    supported: true,
                    min: capabilities.power.sleep_timer.min,
                    max: capabilities.power.sleep_timer.max,
                    step: capabilities.power.sleep_timer.step,
                    recommended_range: capabilities.power.sleep_timer.recommended_range.map(
                        |range| RecommendedRangeData {
                            min: range.min,
                            max: range.max,
                        },
                    ),
                },
                auto_low_power: AutoLowPowerCapabilitiesData {
                    supported: capabilities.power.auto_low_power_supported,
                    threshold: PercentRangeData {
                        min: capabilities.power.auto_low_power_threshold.min,
                        max: capabilities.power.auto_low_power_threshold.max,
                        step: capabilities.power.auto_low_power_threshold.step,
                        unit: "percent",
                    },
                },
            },
            wireless_stability: SupportedData {
                supported: capabilities.wireless_features.wireless_stability.supported,
            },
            bluetooth_smoothing: SupportedData {
                supported: capabilities.wireless_features.bluetooth_smoothing.supported,
            },
            scroll_jump: ScrollJumpCapabilitiesData {
                supported: capabilities.scroll_jump.supported,
                delay: UnitRangeData {
                    min: capabilities.scroll_jump.delay.min,
                    max: capabilities.scroll_jump.delay.max,
                    step: capabilities.scroll_jump.delay.step,
                    unit: "ms",
                },
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DpiCapabilitiesData {
    pub supported: bool,
    pub min: u16,
    pub max: u16,
    pub step: u16,
    pub max_stages: usize,
    pub independent_axes: bool,
}

#[derive(Debug, Serialize)]
pub struct LodCapabilitiesData {
    pub supported: bool,
    pub per_stage: bool,
    pub values: Vec<LodValueData>,
}

#[derive(Debug, Serialize)]
pub struct LodValueData {
    pub value: &'static str,
    pub label: &'static str,
    pub distance_mm: u8,
}

#[derive(Debug, Serialize)]
pub struct PollingCapabilitiesData {
    pub supported: bool,
    pub modes: PollingModesData,
}

#[derive(Debug, Serialize)]
pub struct PollingModesData {
    pub wireless: PollingModeData,
    pub wired: PollingModeData,
}

#[derive(Debug, Serialize)]
pub struct PollingModeData {
    pub rates_hz: Vec<u16>,
}

#[derive(Debug, Serialize)]
pub struct BatteryCapabilitiesData {
    pub supported: bool,
    pub charging_state: bool,
}

#[derive(Debug, Serialize)]
pub struct PowerCapabilitiesData {
    pub supported: bool,
    pub low_power_mode: bool,
    pub low_power_polling: PollingModeData,
    pub sleep_timer: SleepTimerCapabilitiesData,
    pub auto_low_power: AutoLowPowerCapabilitiesData,
}

#[derive(Debug, Serialize)]
pub struct SleepTimerCapabilitiesData {
    pub supported: bool,
    #[serde(rename = "min_minutes")]
    pub min: u64,
    #[serde(rename = "max_minutes")]
    pub max: u64,
    #[serde(rename = "step_minutes")]
    pub step: u64,
    pub recommended_range: Option<RecommendedRangeData>,
}

#[derive(Debug, Serialize)]
pub struct RecommendedRangeData {
    #[serde(rename = "min_minutes")]
    pub min: u64,
    #[serde(rename = "max_minutes")]
    pub max: u64,
}

#[derive(Debug, Serialize)]
pub struct AutoLowPowerCapabilitiesData {
    pub supported: bool,
    pub threshold: PercentRangeData,
}

#[derive(Debug, Serialize)]
pub struct PercentRangeData {
    pub min: u8,
    pub max: u8,
    pub step: u8,
    pub unit: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SupportedData {
    pub supported: bool,
}

#[derive(Debug, Serialize)]
pub struct ScrollJumpCapabilitiesData {
    pub supported: bool,
    pub delay: UnitRangeData<u16>,
}

#[derive(Debug, Serialize)]
pub struct UnitRangeData<T> {
    pub min: T,
    pub max: T,
    pub step: T,
    pub unit: &'static str,
}

#[derive(Debug, Serialize)]
pub struct DeviceSnapshotData {
    pub device: DeviceData,
    pub battery: BatteryData,
    pub dpi: DpiData,
    pub lod: LodData,
    pub polling: PollingData,
    pub power: PowerData,
    pub wireless_stability: EnabledData,
    pub bluetooth_smoothing: EnabledData,
    pub scroll_jump: ScrollJumpData,
}

impl DeviceSnapshotData {
    pub fn new(
        device: &PhysicalDevice,
        battery: BatteryStatus,
        dpi: &DpiConfig,
        polling: PollingConfig,
        power: &PowerConfig,
        wireless_features: &WirelessFeatures,
        scroll_jump: &ScrollJumpConfig,
    ) -> Self {
        Self {
            device: DeviceData::from_device(device),
            battery: BatteryData::from(battery),
            dpi: DpiData::from_config(dpi),
            lod: LodData::from_config(dpi),
            polling: PollingData {
                wireless_hz: polling.wireless.hz(),
                wired_hz: polling.wired.hz(),
            },
            power: PowerData::from(power),
            wireless_stability: EnabledData {
                enabled: wireless_features.wireless_stability_enabled,
            },
            bluetooth_smoothing: EnabledData {
                enabled: wireless_features.bluetooth_smoothing_enabled,
            },
            scroll_jump: ScrollJumpData {
                enabled: scroll_jump.enabled,
                delay_ms: scroll_jump.delay_ms,
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DpiData {
    pub active_stage: usize,
    pub stages: Vec<DpiStageData>,
}

#[derive(Debug, Serialize)]
pub struct DpiStageData {
    pub stage: usize,
    pub x: u16,
    pub y: u16,
}

impl DpiData {
    pub fn from_config(config: &DpiConfig) -> Self {
        Self {
            active_stage: config.active + 1,
            stages: config
                .stages
                .iter()
                .enumerate()
                .map(|(index, stage)| DpiStageData {
                    stage: index + 1,
                    x: stage.x,
                    y: stage.y,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ActiveStageData {
    pub active_stage: usize,
}

#[derive(Debug, Serialize)]
pub struct LodData {
    pub stages: Vec<LodStageData>,
}

#[derive(Debug, Serialize)]
pub struct LodStageData {
    pub stage: usize,
    pub lod: &'static str,
}

impl LodData {
    pub fn from_config(config: &DpiConfig) -> Self {
        Self {
            stages: config
                .stages
                .iter()
                .enumerate()
                .map(|(index, stage)| LodStageData {
                    stage: index + 1,
                    lod: lod_name(stage.lod),
                })
                .collect(),
        }
    }
}

pub const fn lod_name(lod: LiftOffDistance) -> &'static str {
    match lod {
        LiftOffDistance::Low => "low",
        LiftOffDistance::High => "high",
    }
}

#[derive(Debug, Serialize)]
pub struct LodSetData {
    pub stage: usize,
    pub lod: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PollingData {
    pub wireless_hz: u16,
    pub wired_hz: u16,
}

#[derive(Debug, Serialize)]
pub struct PollingSetData {
    pub connection: &'static str,
    pub hz: u16,
}

#[derive(Debug, Serialize)]
pub struct BatteryData {
    pub percentage: Option<u8>,
    pub charging: Option<bool>,
}

impl From<BatteryStatus> for BatteryData {
    fn from(status: BatteryStatus) -> Self {
        match status {
            BatteryStatus::Available { percent, charging } => Self {
                percentage: Some(percent),
                charging: Some(charging),
            },
            BatteryStatus::Unavailable => Self {
                percentage: None,
                charging: None,
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PowerData {
    pub low_power_enabled: bool,
    pub low_power_polling_hz: u16,
    pub sleep_timer_minutes: Option<u32>,
    pub sleep_timer_ms: u32,
    pub auto_low_power_enabled: bool,
    pub auto_low_power_threshold_percent: u8,
}

impl From<&PowerConfig> for PowerData {
    fn from(config: &PowerConfig) -> Self {
        Self {
            low_power_enabled: config.low_power_enabled,
            low_power_polling_hz: config.low_power_polling.hz(),
            sleep_timer_minutes: config.sleep_timer.whole_minutes(),
            sleep_timer_ms: config.sleep_timer.milliseconds(),
            auto_low_power_enabled: config.auto_low_power_enabled,
            auto_low_power_threshold_percent: config.auto_low_power_threshold.percent(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EnabledData {
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct ScrollJumpData {
    pub enabled: bool,
    pub delay_ms: u16,
}

#[derive(Debug, Serialize)]
pub struct DelayData {
    pub delay_ms: u16,
}

#[derive(Debug, Serialize)]
pub struct LowPowerEnabledData {
    pub low_power_enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct LowPowerPollingData {
    pub low_power_polling_hz: u16,
}

#[derive(Debug, Serialize)]
pub struct AutoLowPowerEnabledData {
    pub auto_low_power_enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct AutoLowPowerThresholdData {
    pub auto_low_power_threshold_percent: u8,
}

#[derive(Debug, Serialize)]
pub struct SleepTimerData {
    pub sleep_timer_minutes: u64,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use steelseries_core::{BatteryStatus, DpiConfig, DpiStage, Error, LiftOffDistance};

    use super::*;

    fn parsed(output: &CommandOutput) -> Value {
        serde_json::from_str(&output.json_document()).unwrap()
    }

    #[test]
    fn success_envelope_is_compact_valid_json_with_stable_fields() {
        let output = CommandOutput::new(
            "wireless-stability.get",
            "Wireless Stability Enhancement: On",
            EnabledData { enabled: true },
        );
        let document = output.json_document();
        assert!(!document.contains("Wireless Stability Enhancement"));
        assert!(!document.contains('\n'));
        assert_eq!(
            serde_json::from_str::<Value>(&document).unwrap(),
            json!({
                "schema_version": 1,
                "ok": true,
                "command": "wireless-stability.get",
                "data": { "enabled": true }
            })
        );
        assert_eq!(output.human(), "Wireless Stability Enhancement: On");
    }

    #[test]
    fn toggle_get_and_set_responses_use_real_booleans() {
        for command in [
            "wireless-stability.get",
            "wireless-stability.set",
            "bluetooth-smoothing.get",
            "bluetooth-smoothing.set",
        ] {
            for enabled in [true, false] {
                let value = parsed(&CommandOutput::new(
                    command,
                    "human",
                    EnabledData { enabled },
                ));
                assert_eq!(value["data"]["enabled"], enabled);
            }
        }
    }

    #[test]
    fn scroll_jump_get_and_mutation_responses_are_typed() {
        let get = parsed(&CommandOutput::new(
            "scroll-jump.get",
            "human",
            ScrollJumpData {
                enabled: true,
                delay_ms: 500,
            },
        ));
        assert_eq!(get["data"], json!({"enabled": true, "delay_ms": 500}));

        let set = parsed(&CommandOutput::new(
            "scroll-jump.set",
            "human",
            EnabledData { enabled: false },
        ));
        assert_eq!(set["data"], json!({"enabled": false}));

        let delay = parsed(&CommandOutput::new(
            "scroll-jump.delay",
            "human",
            DelayData { delay_ms: 1_000 },
        ));
        assert_eq!(delay["data"], json!({"delay_ms": 1000}));
    }

    #[test]
    fn dpi_json_preserves_axes_and_uses_one_based_stage_ids() {
        let config = DpiConfig {
            stages: vec![
                DpiStage::scalar(400),
                DpiStage {
                    x: 1600,
                    y: 400,
                    lod: LiftOffDistance::High,
                },
            ],
            active: 1,
        };
        let value = parsed(&CommandOutput::new(
            "dpi.get",
            "human",
            DpiData::from_config(&config),
        ));
        assert_eq!(value["data"]["active_stage"], 2);
        assert_eq!(
            value["data"]["stages"],
            json!([
                {"stage": 1, "x": 400, "y": 400},
                {"stage": 2, "x": 1600, "y": 400}
            ])
        );
    }

    #[test]
    fn lod_json_uses_semantic_lowercase_values() {
        let config = DpiConfig {
            stages: vec![
                DpiStage::scalar(400),
                DpiStage {
                    x: 800,
                    y: 800,
                    lod: LiftOffDistance::High,
                },
            ],
            active: 0,
        };
        let value = parsed(&CommandOutput::new(
            "lod.get",
            "human",
            LodData::from_config(&config),
        ));
        assert_eq!(
            value["data"]["stages"],
            json!([{"stage": 1, "lod": "low"}, {"stage": 2, "lod": "high"}])
        );
    }

    #[test]
    fn polling_power_and_battery_json_use_numeric_and_boolean_types() {
        let polling = parsed(&CommandOutput::new(
            "polling.get",
            "human",
            PollingData {
                wireless_hz: 4_000,
                wired_hz: 1_000,
            },
        ));
        assert_eq!(
            polling["data"],
            json!({"wireless_hz": 4000, "wired_hz": 1000})
        );

        let power = parsed(&CommandOutput::new(
            "power.get",
            "human",
            PowerData {
                low_power_enabled: false,
                low_power_polling_hz: 500,
                sleep_timer_minutes: Some(5),
                sleep_timer_ms: 300_000,
                auto_low_power_enabled: true,
                auto_low_power_threshold_percent: 10,
            },
        ));
        assert_eq!(power["data"]["low_power_enabled"], false);
        assert_eq!(power["data"]["low_power_polling_hz"], 500);
        assert_eq!(power["data"]["sleep_timer_minutes"], 5);
        assert_eq!(power["data"]["sleep_timer_ms"], 300_000);
        assert_eq!(power["data"]["auto_low_power_enabled"], true);
        assert_eq!(power["data"]["auto_low_power_threshold_percent"], 10);

        let available = parsed(&CommandOutput::new(
            "battery.get",
            "human",
            BatteryData::from(BatteryStatus::Available {
                percent: 87,
                charging: false,
            }),
        ));
        assert_eq!(
            available["data"],
            json!({"percentage": 87, "charging": false})
        );

        let unavailable = parsed(&CommandOutput::new(
            "battery.get",
            "human",
            BatteryData::from(BatteryStatus::Unavailable),
        ));
        assert_eq!(
            unavailable["data"],
            json!({"percentage": null, "charging": null})
        );
    }

    #[test]
    fn device_list_json_uses_device_identity_without_hidraw_paths() {
        let value = parsed(&CommandOutput::new(
            "device.list",
            "human",
            DevicesData {
                devices: vec![DeviceData {
                    id: "6271700431492500250".to_owned(),
                    name: "Aerox 3 Wireless Gen 2",
                    model_key: "aerox_3_wireless_gen2",
                    connection: "wired",
                }],
            },
        ));
        assert_eq!(value["command"], "device.list");
        assert_eq!(value["data"]["devices"][0]["id"], "6271700431492500250");
        assert!(value.to_string().find("hidraw").is_none());
    }

    #[test]
    fn mutation_responses_include_requested_values() {
        let polling = parsed(&CommandOutput::new(
            "polling.set",
            "human",
            PollingSetData {
                connection: "wireless",
                hz: 4_000,
            },
        ));
        assert_eq!(
            polling["data"],
            json!({"connection": "wireless", "hz": 4000})
        );

        let stage = parsed(&CommandOutput::new(
            "dpi.use",
            "human",
            ActiveStageData { active_stage: 3 },
        ));
        assert_eq!(stage["data"], json!({"active_stage": 3}));
    }

    #[test]
    fn errors_are_valid_json_with_stable_classification_and_original_message() {
        let invalid = Error::InvalidScrollJumpDelay(250);
        let value: Value =
            serde_json::from_str(&error_document("scroll-jump.delay", &invalid)).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["ok"], false);
        assert_eq!(value["command"], "scroll-jump.delay");
        assert_eq!(value["error"]["code"], "invalid_argument");
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("250 ms")
        );
        assert_eq!(value["error"]["field"], "delay_ms");
        assert_eq!(value["error"]["details"]["min"], 100);
        assert_eq!(value["error"]["details"]["max"], 1500);
        assert_eq!(value["error"]["details"]["step"], 100);

        let timeout = Error::ReadTimeout {
            expected: 0xf0,
            observed: String::new(),
        };
        let value: Value =
            serde_json::from_str(&error_document("bluetooth-smoothing.set", &timeout)).unwrap();
        assert_eq!(value["error"]["code"], "timeout");
        assert_eq!(
            value["error"]["message"],
            "timed out waiting for response 0xf0"
        );
    }

    #[test]
    fn capability_json_uses_device_constraints_semantically() {
        let capabilities = CapabilitiesData::from(Aerox3WirelessGen2::capabilities());
        let value = serde_json::to_value(capabilities).unwrap();
        assert_eq!(
            value["dpi"],
            json!({
                "supported": true,
                "min": 50,
                "max": 26000,
                "step": 50,
                "max_stages": 5,
                "independent_axes": true
            })
        );
        assert_eq!(
            value["polling"]["modes"]["wireless"]["rates_hz"],
            json!([125, 250, 500, 1000, 2000, 4000])
        );
        assert_eq!(value["lod"]["values"][0]["value"], "low");
        assert_eq!(value["lod"]["values"][1]["distance_mm"], 2);
        assert_eq!(value["power"]["sleep_timer"]["max_minutes"], 71_582);
        assert_eq!(
            value["power"]["sleep_timer"]["recommended_range"],
            json!({"min_minutes": 5, "max_minutes": 20})
        );
        assert_eq!(value["scroll_jump"]["delay"]["step"], 100);
    }

    #[test]
    fn device_describe_json_combines_identity_and_capabilities() {
        let output = CommandOutput::new(
            "device.describe",
            "human",
            DeviceDescribeData {
                device: DeviceData {
                    id: "6271700431492500250".to_owned(),
                    name: "Aerox 3 Wireless Gen 2",
                    model_key: "aerox_3_wireless_gen2",
                    connection: "2.4_ghz",
                },
                capabilities: CapabilitiesData::from(Aerox3WirelessGen2::capabilities()),
            },
        );
        let value = parsed(&output);
        assert_eq!(value["command"], "device.describe");
        assert_eq!(
            value["data"]["device"]["model_key"],
            "aerox_3_wireless_gen2"
        );
        assert_eq!(value["data"]["capabilities"]["dpi"]["max"], 26_000);
    }

    #[test]
    fn device_snapshot_json_reuses_existing_state_shapes() {
        let output = CommandOutput::new(
            "device.snapshot",
            "human",
            DeviceSnapshotData {
                device: DeviceData {
                    id: "AAA".to_owned(),
                    name: "Aerox 3 Wireless Gen 2",
                    model_key: "aerox_3_wireless_gen2",
                    connection: "wired",
                },
                battery: BatteryData {
                    percentage: Some(87),
                    charging: Some(false),
                },
                dpi: DpiData {
                    active_stage: 1,
                    stages: vec![DpiStageData {
                        stage: 1,
                        x: 800,
                        y: 1_600,
                    }],
                },
                lod: LodData {
                    stages: vec![LodStageData {
                        stage: 1,
                        lod: "high",
                    }],
                },
                polling: PollingData {
                    wireless_hz: 4_000,
                    wired_hz: 1_000,
                },
                power: PowerData {
                    low_power_enabled: false,
                    low_power_polling_hz: 500,
                    sleep_timer_minutes: Some(5),
                    sleep_timer_ms: 300_000,
                    auto_low_power_enabled: true,
                    auto_low_power_threshold_percent: 10,
                },
                wireless_stability: EnabledData { enabled: true },
                bluetooth_smoothing: EnabledData { enabled: false },
                scroll_jump: ScrollJumpData {
                    enabled: true,
                    delay_ms: 500,
                },
            },
        );
        let value = parsed(&output);
        assert_eq!(value["command"], "device.snapshot");
        assert_eq!(value["data"]["dpi"]["stages"][0]["y"], 1_600);
        assert_eq!(value["data"]["lod"]["stages"][0]["lod"], "high");
        assert_eq!(value["data"]["battery"]["percentage"], 87);
        assert_eq!(value["data"]["scroll_jump"]["delay_ms"], 500);
    }

    #[test]
    fn unsupported_capability_is_represented_explicitly() {
        let mut capabilities = *Aerox3WirelessGen2::capabilities();
        capabilities.scroll_jump.supported = false;
        let value = serde_json::to_value(CapabilitiesData::from(&capabilities)).unwrap();
        assert_eq!(value["scroll_jump"]["supported"], false);
    }

    #[test]
    fn structured_validation_errors_do_not_parse_messages() {
        let dpi = Error::InvalidDpiValue {
            axis: "X",
            value: 27_000,
            stage: Some(1),
        };
        let value: Value = serde_json::from_str(&error_document("dpi.set", &dpi)).unwrap();
        assert_eq!(value["error"]["field"], "stages[0].x");
        assert_eq!(value["error"]["details"]["axis"], "x");
        assert_eq!(value["error"]["details"]["value"], 27_000);
        assert_eq!(value["error"]["details"]["min"], 50);
        assert_eq!(value["error"]["details"]["max"], 26_000);
        assert_eq!(value["error"]["details"]["step"], 50);

        let polling = Error::WiredPollingTooHigh { value: 2_000 };
        let value: Value = serde_json::from_str(&error_document("polling.set", &polling)).unwrap();
        assert_eq!(value["error"]["field"], "wired_hz");
        assert_eq!(
            value["error"]["details"]["allowed"],
            json!([125, 250, 500, 1000])
        );
    }
}
