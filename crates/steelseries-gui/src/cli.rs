use std::{env, io, path::PathBuf, process::Command};

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

const JSON_SCHEMA_VERSION: u8 = 1;
const DEVICE_LIST_COMMAND: &str = "device.list";
const DEVICE_DESCRIBE_COMMAND: &str = "device.describe";
const DEVICE_SNAPSHOT_COMMAND: &str = "device.snapshot";
const BATTERY_GET_COMMAND: &str = "battery.get";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Device {
    pub connection: String,
    pub id: String,
    pub model_key: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DeviceDescription {
    pub device: Device,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Capabilities {
    pub dpi: DpiCapabilities,
    pub lod: LodCapabilities,
    pub polling: PollingCapabilities,
    pub battery: BatteryCapabilities,
    pub power: PowerCapabilities,
    pub wireless_stability: SupportedCapability,
    pub bluetooth_smoothing: SupportedCapability,
    pub scroll_jump: ScrollJumpCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DpiCapabilities {
    pub supported: bool,
    pub min: u16,
    pub max: u16,
    pub step: u16,
    pub max_stages: usize,
    pub independent_axes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LodCapabilities {
    pub supported: bool,
    pub per_stage: bool,
    pub values: Vec<LodValueCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LodValueCapability {
    pub value: String,
    pub label: String,
    pub distance_mm: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PollingCapabilities {
    pub supported: bool,
    pub modes: PollingModes,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PollingModes {
    pub wireless: PollingMode,
    pub wired: PollingMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PollingMode {
    pub rates_hz: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BatteryCapabilities {
    pub supported: bool,
    pub charging_state: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PowerCapabilities {
    pub supported: bool,
    pub low_power_mode: bool,
    pub low_power_polling: PollingMode,
    pub sleep_timer: SleepTimerCapabilities,
    pub auto_low_power: AutoLowPowerCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SleepTimerCapabilities {
    pub supported: bool,
    pub min_minutes: u64,
    pub max_minutes: u64,
    pub step_minutes: u64,
    #[serde(default)]
    pub recommended_range: Option<RecommendedRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RecommendedRange {
    pub min_minutes: u64,
    pub max_minutes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AutoLowPowerCapabilities {
    pub supported: bool,
    pub threshold: UnitRange<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SupportedCapability {
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScrollJumpCapabilities {
    pub supported: bool,
    pub delay: UnitRange<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UnitRange<T> {
    pub min: T,
    pub max: T,
    pub step: T,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DeviceSnapshot {
    pub device: Device,
    pub battery: BatteryState,
    pub dpi: DpiState,
    pub lod: LodState,
    pub polling: PollingState,
    pub power: PowerState,
    pub wireless_stability: EnabledState,
    pub bluetooth_smoothing: EnabledState,
    pub scroll_jump: ScrollJumpState,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BatteryState {
    pub percentage: Option<u8>,
    pub charging: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DpiState {
    pub active_stage: usize,
    pub stages: Vec<DpiStageState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DpiStageState {
    pub stage: usize,
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LodState {
    pub stages: Vec<LodStageState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LodStageState {
    pub stage: usize,
    pub lod: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PollingState {
    pub wireless_hz: u16,
    pub wired_hz: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PowerState {
    pub low_power_enabled: bool,
    pub low_power_polling_hz: u16,
    pub sleep_timer_minutes: Option<u32>,
    pub sleep_timer_ms: u32,
    pub auto_low_power_enabled: bool,
    pub auto_low_power_threshold_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EnabledState {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScrollJumpState {
    pub enabled: bool,
    pub delay_ms: u16,
}

#[derive(Debug, Deserialize)]
struct DeviceListData {
    devices: Vec<Device>,
}

#[derive(Debug, Deserialize)]
struct SuccessEnvelope<T> {
    schema_version: u8,
    ok: bool,
    command: String,
    data: T,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    schema_version: u8,
    ok: bool,
    command: String,
    error: CliErrorBody,
}

#[derive(Debug, Deserialize)]
struct CliErrorBody {
    code: String,
    message: String,
    field: Option<String>,
    details: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiValue {
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mutation {
    Dpi(Vec<DpiValue>),
    ActiveDpi(usize),
    Lod { stage: usize, value: String },
    Polling { mode: &'static str, hz: u16 },
    LowPower(bool),
    LowPowerPolling(u16),
    SleepTimer(u64),
    AutoLowPower(bool),
    AutoLowPowerThreshold(u8),
    WirelessStability(bool),
    BluetoothSmoothing(bool),
    ScrollJump(bool),
    ScrollJumpDelay(u16),
}

impl Mutation {
    fn command(&self) -> &'static str {
        match self {
            Self::Dpi(_) => "dpi.set",
            Self::ActiveDpi(_) => "dpi.use",
            Self::Lod { .. } => "lod.set",
            Self::Polling { .. } => "polling.set",
            Self::LowPower(_) => "power.low-power.set",
            Self::LowPowerPolling(_) => "power.low-power.polling",
            Self::SleepTimer(_) => "power.sleep.set",
            Self::AutoLowPower(_) => "power.auto-low-power.set",
            Self::AutoLowPowerThreshold(_) => "power.auto-low-power.threshold",
            Self::WirelessStability(_) => "wireless-stability.set",
            Self::BluetoothSmoothing(_) => "bluetooth-smoothing.set",
            Self::ScrollJump(_) => "scroll-jump.set",
            Self::ScrollJumpDelay(_) => "scroll-jump.delay",
        }
    }

    fn arguments(&self, device_id: &str) -> Vec<String> {
        let mut arguments = vec![
            "--json".to_owned(),
            "--device".to_owned(),
            device_id.to_owned(),
        ];
        let state = |enabled| if enabled { "on" } else { "off" };
        match self {
            Self::Dpi(stages) => {
                arguments.extend(["dpi".to_owned(), "set".to_owned()]);
                arguments.extend(stages.iter().map(|stage| {
                    if stage.x == stage.y {
                        stage.x.to_string()
                    } else {
                        format!("{}x{}", stage.x, stage.y)
                    }
                }));
            }
            Self::ActiveDpi(stage) => {
                arguments.extend(["dpi".to_owned(), "use".to_owned(), stage.to_string()])
            }
            Self::Lod { stage, value } => arguments.extend([
                "lod".to_owned(),
                "set".to_owned(),
                value.clone(),
                stage.to_string(),
            ]),
            Self::Polling { mode, hz } => arguments.extend([
                "polling".to_owned(),
                "set".to_owned(),
                (*mode).to_owned(),
                hz.to_string(),
            ]),
            Self::LowPower(enabled) => arguments.extend([
                "power".to_owned(),
                "low-power".to_owned(),
                "set".to_owned(),
                state(*enabled).to_owned(),
            ]),
            Self::LowPowerPolling(hz) => arguments.extend([
                "power".to_owned(),
                "low-power".to_owned(),
                "polling".to_owned(),
                hz.to_string(),
            ]),
            Self::SleepTimer(minutes) => arguments.extend([
                "power".to_owned(),
                "sleep".to_owned(),
                "set".to_owned(),
                minutes.to_string(),
            ]),
            Self::AutoLowPower(enabled) => arguments.extend([
                "power".to_owned(),
                "auto-low-power".to_owned(),
                "set".to_owned(),
                state(*enabled).to_owned(),
            ]),
            Self::AutoLowPowerThreshold(percent) => arguments.extend([
                "power".to_owned(),
                "auto-low-power".to_owned(),
                "threshold".to_owned(),
                percent.to_string(),
            ]),
            Self::WirelessStability(enabled) => arguments.extend([
                "wireless-stability".to_owned(),
                "set".to_owned(),
                state(*enabled).to_owned(),
            ]),
            Self::BluetoothSmoothing(enabled) => arguments.extend([
                "bluetooth-smoothing".to_owned(),
                "set".to_owned(),
                state(*enabled).to_owned(),
            ]),
            Self::ScrollJump(enabled) => arguments.extend([
                "scroll-jump".to_owned(),
                "set".to_owned(),
                state(*enabled).to_owned(),
            ]),
            Self::ScrollJumpDelay(milliseconds) => arguments.extend([
                "scroll-jump".to_owned(),
                "delay".to_owned(),
                milliseconds.to_string(),
            ]),
        }
        arguments
    }
}

#[derive(Debug, Error)]
pub enum CliClientError {
    #[error("failed to start {path}: {source}")]
    Start {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("steelseriesctl returned malformed JSON for {command}: {source}")]
    MalformedResponse {
        command: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("steelseriesctl returned malformed error JSON: {json_error}; stderr: {stderr}")]
    MalformedError {
        json_error: serde_json::Error,
        stderr: String,
    },
    #[error("unsupported steelseriesctl JSON schema {actual}; expected {expected}")]
    UnsupportedSchema { actual: u8, expected: u8 },
    #[error("unexpected steelseriesctl command identifier '{actual}'; expected '{expected}'")]
    UnexpectedCommand { actual: String, expected: String },
    #[error("steelseriesctl returned a success response with ok=false")]
    UnsuccessfulSuccess,
    #[error("steelseriesctl command failed: {0}")]
    CommandFailed(Box<CommandFailure>),
}

#[derive(Debug)]
pub struct CommandFailure {
    code: String,
    message: String,
    command: String,
    field: Option<String>,
    details: Option<Value>,
}

impl std::fmt::Display for CommandFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} ({}): {}",
            self.command, self.code, self.message
        )
    }
}

impl CliClientError {
    pub fn user_message(&self) -> String {
        match self {
            Self::Start { source, .. } if source.kind() == io::ErrorKind::NotFound => {
                "steelseriesctl was not found. Install the CLI or set STEELSERIESCTL_PATH."
                    .to_owned()
            }
            Self::CommandFailed(failure) => failure.message.clone(),
            Self::MalformedResponse { .. } | Self::MalformedError { .. } => {
                "steelseriesctl returned invalid JSON.".to_owned()
            }
            Self::UnsupportedSchema { .. } => {
                "The installed steelseriesctl JSON API is not compatible.".to_owned()
            }
            Self::UnexpectedCommand { .. } | Self::UnsuccessfulSuccess => {
                "steelseriesctl returned an unexpected response.".to_owned()
            }
            Self::Start { .. } => "Could not start steelseriesctl.".to_owned(),
        }
    }

    pub fn device_message(&self) -> String {
        if matches!(self, Self::CommandFailed(failure) if failure.code == "device_not_found") {
            "Device disconnected or unavailable.".to_owned()
        } else {
            self.user_message()
        }
    }

    pub fn field(&self) -> Option<&str> {
        match self {
            Self::CommandFailed(failure) => failure.field.as_deref(),
            _ => None,
        }
    }

    pub fn validation_hint(&self) -> Option<String> {
        let Self::CommandFailed(failure) = self else {
            return None;
        };
        let details = failure.details.as_ref()?;
        if let (Some(min), Some(max), Some(step)) =
            (details.get("min"), details.get("max"), details.get("step"))
        {
            return Some(format!("Expected {min}–{max} in steps of {step}."));
        }
        details
            .get("allowed")
            .map(|allowed| format!("Allowed values: {allowed}."))
    }

    pub fn is_device_not_found(&self) -> bool {
        matches!(self, Self::CommandFailed(failure) if failure.code == "device_not_found")
    }
}

#[derive(Debug, Clone)]
pub struct CliClient {
    executable: PathBuf,
}

impl CliClient {
    #[must_use]
    pub fn from_environment() -> Self {
        let executable = env::var_os("STEELSERIESCTL_PATH")
            .filter(|value| !value.is_empty())
            .map_or_else(|| PathBuf::from("steelseriesctl"), PathBuf::from);
        Self { executable }
    }

    pub fn list_devices(&self) -> Result<Vec<Device>, CliClientError> {
        self.run_json::<DeviceListData>(&["--json", "device", "list"], DEVICE_LIST_COMMAND)
            .map(|data| data.devices)
    }

    pub fn describe_device(&self, device_id: &str) -> Result<DeviceDescription, CliClientError> {
        self.run_json(
            &["--json", "--device", device_id, "device", "describe"],
            DEVICE_DESCRIBE_COMMAND,
        )
    }

    pub fn snapshot_device(&self, device_id: &str) -> Result<DeviceSnapshot, CliClientError> {
        self.run_json(
            &["--json", "--device", device_id, "device", "snapshot"],
            DEVICE_SNAPSHOT_COMMAND,
        )
    }

    pub fn get_battery(&self, device_id: &str) -> Result<BatteryState, CliClientError> {
        self.run_json(
            &["--json", "--device", device_id, "battery", "get"],
            BATTERY_GET_COMMAND,
        )
    }

    pub fn set_dpi(&self, device_id: &str, stages: &[DpiValue]) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::Dpi(stages.to_vec()))
    }

    pub fn set_active_dpi_stage(
        &self,
        device_id: &str,
        stage: usize,
    ) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::ActiveDpi(stage))
    }

    pub fn set_lod(
        &self,
        device_id: &str,
        stage: usize,
        value: &str,
    ) -> Result<(), CliClientError> {
        self.run_mutation(
            device_id,
            Mutation::Lod {
                stage,
                value: value.to_owned(),
            },
        )
    }

    pub fn set_polling(
        &self,
        device_id: &str,
        mode: &'static str,
        hz: u16,
    ) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::Polling { mode, hz })
    }

    pub fn set_low_power(&self, device_id: &str, enabled: bool) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::LowPower(enabled))
    }

    pub fn set_low_power_polling(&self, device_id: &str, hz: u16) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::LowPowerPolling(hz))
    }

    pub fn set_sleep_timer(&self, device_id: &str, minutes: u64) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::SleepTimer(minutes))
    }

    pub fn set_auto_low_power(&self, device_id: &str, enabled: bool) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::AutoLowPower(enabled))
    }

    pub fn set_auto_low_power_threshold(
        &self,
        device_id: &str,
        percent: u8,
    ) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::AutoLowPowerThreshold(percent))
    }

    pub fn set_wireless_stability(
        &self,
        device_id: &str,
        enabled: bool,
    ) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::WirelessStability(enabled))
    }

    pub fn set_bluetooth_smoothing(
        &self,
        device_id: &str,
        enabled: bool,
    ) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::BluetoothSmoothing(enabled))
    }

    pub fn set_scroll_jump(&self, device_id: &str, enabled: bool) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::ScrollJump(enabled))
    }

    pub fn set_scroll_jump_delay(
        &self,
        device_id: &str,
        milliseconds: u16,
    ) -> Result<(), CliClientError> {
        self.run_mutation(device_id, Mutation::ScrollJumpDelay(milliseconds))
    }

    fn run_mutation(&self, device_id: &str, mutation: Mutation) -> Result<(), CliClientError> {
        let arguments = mutation.arguments(device_id);
        let borrowed = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        self.run_json::<Value>(&borrowed, mutation.command())
            .map(|_| ())
    }

    fn run_json<T>(&self, arguments: &[&str], expected: &str) -> Result<T, CliClientError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let output = Command::new(&self.executable)
            .args(arguments)
            .output()
            .map_err(|source| CliClientError::Start {
                path: self.executable.display().to_string(),
                source,
            })?;
        if output.status.success() {
            parse_success(&output.stdout, expected)
        } else {
            Err(parse_cli_error(&output.stderr, expected))
        }
    }
}

fn parse_success<T>(json: &[u8], expected: &str) -> Result<T, CliClientError>
where
    T: for<'de> Deserialize<'de>,
{
    let response: SuccessEnvelope<T> =
        serde_json::from_slice(json).map_err(|source| CliClientError::MalformedResponse {
            command: expected.to_owned(),
            source,
        })?;
    validate_envelope(response.schema_version, &response.command, expected)?;
    if !response.ok {
        return Err(CliClientError::UnsuccessfulSuccess);
    }
    Ok(response.data)
}

fn parse_cli_error(json: &[u8], expected: &str) -> CliClientError {
    match serde_json::from_slice::<ErrorEnvelope>(json) {
        Ok(response) => {
            if let Err(error) =
                validate_envelope(response.schema_version, &response.command, expected)
            {
                return error;
            }
            if response.ok {
                return CliClientError::UnsuccessfulSuccess;
            }
            CliClientError::CommandFailed(Box::new(CommandFailure {
                code: response.error.code,
                message: response.error.message,
                command: response.command,
                field: response.error.field,
                details: response.error.details,
            }))
        }
        Err(json_error) => CliClientError::MalformedError {
            json_error,
            stderr: String::from_utf8_lossy(json).trim().to_owned(),
        },
    }
}

fn validate_envelope(
    schema_version: u8,
    command: &str,
    expected: &str,
) -> Result<(), CliClientError> {
    if schema_version != JSON_SCHEMA_VERSION {
        return Err(CliClientError::UnsupportedSchema {
            actual: schema_version,
            expected: JSON_SCHEMA_VERSION,
        });
    }
    if command != expected {
        return Err(CliClientError::UnexpectedCommand {
            actual: command.to_owned(),
            expected: expected.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICE: &str = r#"{"connection":"wired","id":"6271700431492500250","model_key":"aerox_3_wireless_gen2","name":"Aerox 3 Wireless Gen 2"}"#;
    const CAPABILITIES: &str = r#"{"dpi":{"supported":true,"min":50,"max":26000,"step":50,"max_stages":5,"independent_axes":true},"lod":{"supported":true,"per_stage":true,"values":[{"value":"low","label":"Low","distance_mm":1},{"value":"high","label":"High","distance_mm":2}]},"polling":{"supported":true,"modes":{"wireless":{"rates_hz":[125,250,500,1000,2000,4000]},"wired":{"rates_hz":[125,250,500,1000]}}},"battery":{"supported":true,"charging_state":true},"power":{"supported":true,"low_power_mode":true,"low_power_polling":{"rates_hz":[125,250,500]},"sleep_timer":{"supported":true,"min_minutes":1,"max_minutes":71582,"step_minutes":1,"recommended_range":{"min_minutes":5,"max_minutes":20}},"auto_low_power":{"supported":true,"threshold":{"min":5,"max":25,"step":1,"unit":"percent"}}},"wireless_stability":{"supported":true},"bluetooth_smoothing":{"supported":true},"scroll_jump":{"supported":true,"delay":{"min":100,"max":1500,"step":100,"unit":"ms"}}}"#;
    const SNAPSHOT: &str = r#"{"device":{"connection":"wired","id":"6271700431492500250","model_key":"aerox_3_wireless_gen2","name":"Aerox 3 Wireless Gen 2"},"battery":{"percentage":82,"charging":true},"dpi":{"active_stage":2,"stages":[{"stage":1,"x":400,"y":400},{"stage":2,"x":800,"y":1600}]},"lod":{"stages":[{"stage":1,"lod":"low"},{"stage":2,"lod":"high"}]},"polling":{"wireless_hz":4000,"wired_hz":1000},"power":{"low_power_enabled":false,"low_power_polling_hz":500,"sleep_timer_minutes":5,"sleep_timer_ms":300000,"auto_low_power_enabled":true,"auto_low_power_threshold_percent":10},"wireless_stability":{"enabled":true},"bluetooth_smoothing":{"enabled":false},"scroll_jump":{"enabled":true,"delay_ms":500}}"#;

    fn envelope(command: &str, data: &str) -> String {
        format!(r#"{{"schema_version":1,"ok":true,"command":"{command}","data":{data}}}"#)
    }

    #[test]
    fn parses_successful_device_list_and_preserves_fields() {
        let json = envelope("device.list", &format!(r#"{{"devices":[{DEVICE}]}}"#));
        let data: DeviceListData = parse_success(json.as_bytes(), DEVICE_LIST_COMMAND).unwrap();
        assert_eq!(data.devices.len(), 1);
        assert_eq!(data.devices[0].connection, "wired");
        assert_eq!(data.devices[0].model_key, "aerox_3_wireless_gen2");
    }

    #[test]
    fn parses_zero_and_multiple_devices() {
        let empty: DeviceListData = parse_success(
            envelope("device.list", r#"{"devices":[]}"#).as_bytes(),
            DEVICE_LIST_COMMAND,
        )
        .unwrap();
        assert!(empty.devices.is_empty());
        let devices = format!(r#"{{"devices":[{DEVICE},{DEVICE}]}}"#);
        let multiple: DeviceListData = parse_success(
            envelope("device.list", &devices).as_bytes(),
            DEVICE_LIST_COMMAND,
        )
        .unwrap();
        assert_eq!(multiple.devices.len(), 2);
    }

    #[test]
    fn parses_device_description_and_capabilities() {
        let data = format!(r#"{{"device":{DEVICE},"capabilities":{CAPABILITIES}}}"#);
        let description: DeviceDescription = parse_success(
            envelope("device.describe", &data).as_bytes(),
            DEVICE_DESCRIBE_COMMAND,
        )
        .unwrap();
        assert!(description.capabilities.dpi.independent_axes);
        assert_eq!(description.capabilities.dpi.max_stages, 5);
        assert_eq!(description.capabilities.lod.values[1].distance_mm, 2);
        assert_eq!(
            description.capabilities.polling.modes.wireless.rates_hz,
            [125, 250, 500, 1000, 2000, 4000]
        );
        assert_eq!(description.capabilities.scroll_jump.delay.step, 100);
        assert_eq!(
            description.capabilities.power.sleep_timer.recommended_range,
            Some(RecommendedRange {
                min_minutes: 5,
                max_minutes: 20,
            })
        );
    }

    #[test]
    fn parses_device_snapshot_with_all_current_state() {
        let snapshot: DeviceSnapshot = parse_success(
            envelope("device.snapshot", SNAPSHOT).as_bytes(),
            DEVICE_SNAPSHOT_COMMAND,
        )
        .unwrap();
        assert_eq!(snapshot.battery.percentage, Some(82));
        assert_eq!(snapshot.battery.charging, Some(true));
        assert_eq!(
            (snapshot.dpi.stages[1].x, snapshot.dpi.stages[1].y),
            (800, 1600)
        );
        assert_eq!(snapshot.polling.wireless_hz, 4000);
        assert!(snapshot.power.auto_low_power_enabled);
        assert!(snapshot.wireless_stability.enabled);
        assert!(!snapshot.bluetooth_smoothing.enabled);
        assert_eq!(snapshot.scroll_jump.delay_ms, 500);
    }

    #[test]
    fn parses_null_battery_telemetry() {
        let data = SNAPSHOT.replace(
            r#""percentage":82,"charging":true"#,
            r#""percentage":null,"charging":null"#,
        );
        let snapshot: DeviceSnapshot = parse_success(
            envelope("device.snapshot", &data).as_bytes(),
            DEVICE_SNAPSHOT_COMMAND,
        )
        .unwrap();
        assert_eq!(snapshot.battery.percentage, None);
        assert_eq!(snapshot.battery.charging, None);
    }

    #[test]
    fn parses_standalone_battery_response_for_monitoring() {
        let battery: BatteryState = parse_success(
            envelope("battery.get", r#"{"percentage":90,"charging":false}"#).as_bytes(),
            BATTERY_GET_COMMAND,
        )
        .unwrap();
        assert_eq!(battery.percentage, Some(90));
        assert_eq!(battery.charging, Some(false));

        let error = parse_cli_error(
            br#"{"schema_version":1,"ok":false,"command":"battery.get","error":{"code":"device_not_found","message":"gone"}}"#,
            BATTERY_GET_COMMAND,
        );
        assert!(error.is_device_not_found());
    }

    #[test]
    fn rejects_malformed_describe_and_snapshot_json() {
        assert!(matches!(
            parse_success::<DeviceDescription>(b"not json", DEVICE_DESCRIBE_COMMAND),
            Err(CliClientError::MalformedResponse { .. })
        ));
        assert!(matches!(
            parse_success::<DeviceSnapshot>(b"not json", DEVICE_SNAPSHOT_COMMAND),
            Err(CliClientError::MalformedResponse { .. })
        ));
    }

    #[test]
    fn parses_structured_cli_error() {
        let error = parse_cli_error(br#"{"schema_version":1,"ok":false,"command":"device.snapshot","error":{"code":"device_not_found","message":"not found"}}"#, DEVICE_SNAPSHOT_COMMAND);
        assert!(matches!(
            error,
            CliClientError::CommandFailed(ref failure)
                if failure.code == "device_not_found" && failure.command == "device.snapshot"
        ));
        assert_eq!(
            error.device_message(),
            "Device disconnected or unavailable."
        );
    }

    #[test]
    fn rejects_wrong_schema_or_command() {
        let wrong_schema = envelope("device.list", r#"{"devices":[]}"#)
            .replace("\"schema_version\":1", "\"schema_version\":2");
        assert!(matches!(
            parse_success::<DeviceListData>(wrong_schema.as_bytes(), DEVICE_LIST_COMMAND),
            Err(CliClientError::UnsupportedSchema { actual: 2, .. })
        ));
        let wrong_command = envelope("devices", r#"{"devices":[]}"#);
        assert!(
            matches!(parse_success::<DeviceListData>(wrong_command.as_bytes(), DEVICE_LIST_COMMAND), Err(CliClientError::UnexpectedCommand { actual, .. }) if actual == "devices")
        );
    }

    fn args(mutation: Mutation) -> Vec<String> {
        mutation.arguments("DEVICE-AAA")
    }

    #[test]
    fn mutation_commands_always_include_selected_physical_id() {
        for mutation in [
            Mutation::Dpi(vec![DpiValue { x: 400, y: 400 }]),
            Mutation::ActiveDpi(2),
            Mutation::Lod {
                stage: 1,
                value: "low".to_owned(),
            },
            Mutation::Polling {
                mode: "wireless",
                hz: 1000,
            },
            Mutation::LowPower(true),
            Mutation::LowPowerPolling(250),
            Mutation::SleepTimer(30),
            Mutation::AutoLowPower(true),
            Mutation::AutoLowPowerThreshold(15),
            Mutation::WirelessStability(true),
            Mutation::BluetoothSmoothing(false),
            Mutation::ScrollJump(true),
            Mutation::ScrollJumpDelay(500),
        ] {
            assert_eq!(&args(mutation)[..3], ["--json", "--device", "DEVICE-AAA"]);
        }
    }

    #[test]
    fn builds_scalar_and_independent_dpi_commands() {
        assert_eq!(
            args(Mutation::Dpi(vec![
                DpiValue { x: 400, y: 400 },
                DpiValue { x: 800, y: 1600 },
            ])),
            [
                "--json",
                "--device",
                "DEVICE-AAA",
                "dpi",
                "set",
                "400",
                "800x1600"
            ]
        );
        assert_eq!(args(Mutation::ActiveDpi(5)).last().unwrap(), "5");
    }

    #[test]
    fn builds_lod_polling_and_power_commands() {
        assert_eq!(
            &args(Mutation::Lod {
                stage: 3,
                value: "high".to_owned()
            })[3..],
            ["lod", "set", "high", "3"]
        );
        assert_eq!(
            &args(Mutation::Polling {
                mode: "wireless",
                hz: 4000
            })[3..],
            ["polling", "set", "wireless", "4000"]
        );
        assert_eq!(
            &args(Mutation::Polling {
                mode: "wired",
                hz: 1000
            })[3..],
            ["polling", "set", "wired", "1000"]
        );
        assert_eq!(
            &args(Mutation::LowPower(false))[3..],
            ["power", "low-power", "set", "off"]
        );
        assert_eq!(
            &args(Mutation::LowPowerPolling(500))[3..],
            ["power", "low-power", "polling", "500"]
        );
        assert_eq!(
            &args(Mutation::SleepTimer(1440))[3..],
            ["power", "sleep", "set", "1440"]
        );
        assert_eq!(
            &args(Mutation::AutoLowPower(true))[3..],
            ["power", "auto-low-power", "set", "on"]
        );
        assert_eq!(
            &args(Mutation::AutoLowPowerThreshold(25))[3..],
            ["power", "auto-low-power", "threshold", "25"]
        );
    }

    #[test]
    fn builds_wireless_and_scroll_jump_commands() {
        assert_eq!(
            &args(Mutation::WirelessStability(true))[3..],
            ["wireless-stability", "set", "on"]
        );
        assert_eq!(
            &args(Mutation::BluetoothSmoothing(false))[3..],
            ["bluetooth-smoothing", "set", "off"]
        );
        assert_eq!(
            &args(Mutation::ScrollJump(true))[3..],
            ["scroll-jump", "set", "on"]
        );
        assert_eq!(
            &args(Mutation::ScrollJumpDelay(1500))[3..],
            ["scroll-jump", "delay", "1500"]
        );
    }

    #[test]
    fn parses_json_mutation_success_and_structured_error_metadata() {
        let value: Value = parse_success(
            envelope("dpi.use", r#"{"active_stage":2}"#).as_bytes(),
            "dpi.use",
        )
        .unwrap();
        assert_eq!(value["active_stage"], 2);

        let error = parse_cli_error(
            br#"{"schema_version":1,"ok":false,"command":"dpi.set","error":{"code":"invalid_argument","message":"bad DPI","field":"stages[0].x","details":{"min":50,"max":26000,"step":50}}}"#,
            "dpi.set",
        );
        assert_eq!(error.field(), Some("stages[0].x"));
        assert!(matches!(
            error,
            CliClientError::CommandFailed(ref failure)
                if failure.details.as_ref().unwrap()["step"] == 50
        ));
    }
}
