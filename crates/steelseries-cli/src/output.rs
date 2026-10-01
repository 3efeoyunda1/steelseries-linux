use serde::Serialize;
use serde_json::Value;
use steelseries_core::{
    BatteryStatus, ConnectionType, DpiConfig, Error, LiftOffDistance, PhysicalDevice, PowerConfig,
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
}

pub fn error_document(command: &str, error: &Error) -> String {
    let message = error.to_string();
    error_document_with_code(command, error_code(error), &message)
}

pub fn invalid_argument_document(command: &str, message: &str) -> String {
    error_document_with_code(command, "invalid_argument", message)
}

fn error_document_with_code(command: &str, code: &'static str, message: &str) -> String {
    serde_json::to_string(&ErrorEnvelope {
        schema_version: SCHEMA_VERSION,
        ok: false,
        command,
        error: ErrorBody { code, message },
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
        | Error::WiredPollingTooHigh
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
    pub connection: &'static str,
}

impl DevicesData {
    pub fn from_devices(devices: &[PhysicalDevice]) -> Self {
        Self {
            devices: devices
                .iter()
                .map(|device| DeviceData {
                    id: device.identity.to_string(),
                    name: "Aerox 3 Wireless Gen 2",
                    connection: match device.active_connection {
                        ConnectionType::Wired => "wired",
                        ConnectionType::Wireless2_4Ghz => "2.4_ghz",
                    },
                })
                .collect(),
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
    fn devices_json_uses_device_identity_without_hidraw_paths() {
        let value = parsed(&CommandOutput::new(
            "devices",
            "human",
            DevicesData {
                devices: vec![DeviceData {
                    id: "6271700431492500250".to_owned(),
                    name: "Aerox 3 Wireless Gen 2",
                    connection: "wired",
                }],
            },
        ));
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
}
