use std::{
    convert::TryFrom,
    ffi::CString,
    time::{Duration, Instant},
};

use hidapi::{HidApi, HidDevice};
use thiserror::Error;

use crate::device::{
    AutoLowPowerThreshold, BatteryStatus, ConnectionType, DeviceCapabilities, DeviceIdentity,
    DpiCapabilities, DpiConfig, DpiStage, LiftOffDistance, LowPowerPollingRate, PhysicalDevice,
    PollingCapabilities, PollingConfig, PollingRate, PowerCapabilities, PowerConfig,
    ScrollJumpCapabilities, ScrollJumpConfig, SleepTimer, WirelessFeatureCapabilities,
    WirelessFeatures,
};

pub const VENDOR_ID: u16 = 0x1038;
pub const PID_RECEIVER: u16 = 0x1890;
pub const PID_WIRED: u16 = 0x1892;
pub const CONFIG_INTERFACE: i32 = 3;

pub const CMD_COMMIT: u8 = 0x11;
pub const CMD_SET_DPI: u8 = 0x6d;
pub const CMD_GET_DPI: u8 = 0xad;
pub const CMD_SET_POLLING: u8 = 0x6b;
pub const CMD_GET_POLLING: u8 = 0xab;
pub const CMD_SET_POWER: u8 = 0x68;
pub const CMD_GET_POWER: u8 = 0xa8;
pub const CMD_SET_WIRELESS_FEATURES: u8 = 0x55;
pub const CMD_GET_WIRELESS_FEATURES: u8 = 0x95;
pub const CMD_SET_SCROLL_JUMP: u8 = 0x56;
pub const CMD_GET_SCROLL_JUMP: u8 = 0x96;
pub const CMD_GET_BATTERY: u8 = 0x92;
pub const CMD_GET_DEVICE_IDENTITY: u8 = 0xf0;
pub const CMD_GET_RECEIVER_LINK: u8 = 0xbc;

const REPORT_SIZE: usize = 64;
const WRITE_SIZE: usize = REPORT_SIZE + 1;
const RESPONSE_TIMEOUT: Duration = Duration::from_millis(1_500);
const IDENTITY_ATTEMPT_TIMEOUT: Duration = Duration::from_millis(300);
const IDENTITY_MAX_ATTEMPTS: usize = 5;
const POWER_RESERVED_OFFSET: usize = 9;
const POWER_RESERVED_SIZE: usize = REPORT_SIZE - POWER_RESERVED_OFFSET;
const WIRELESS_FEATURES_RESERVED_OFFSET: usize = 3;
const WIRELESS_FEATURES_RESERVED_SIZE: usize = REPORT_SIZE - WIRELESS_FEATURES_RESERVED_OFFSET;
const SCROLL_JUMP_RESERVED_OFFSET: usize = 4;
const SCROLL_JUMP_RESERVED_SIZE: usize = REPORT_SIZE - SCROLL_JUMP_RESERVED_OFFSET;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndpointKind {
    Receiver2_4Ghz,
    Wired,
}

struct IdentifiedEndpoint<T> {
    identity: DeviceIdentity,
    kind: EndpointKind,
    endpoint: T,
}

struct EndpointPair<T> {
    identity: DeviceIdentity,
    wired: Option<T>,
    receiver: Option<T>,
}

impl<T> EndpointPair<T> {
    fn physical_device(&self) -> PhysicalDevice {
        PhysicalDevice {
            identity: self.identity.clone(),
            active_connection: if self.wired.is_some() {
                ConnectionType::Wired
            } else {
                ConnectionType::Wireless2_4Ghz
            },
            wired_endpoint_available: self.wired.is_some(),
            receiver_endpoint_available: self.receiver.is_some(),
        }
    }

    fn into_selected_endpoint(self) -> T {
        self.wired
            .or(self.receiver)
            .expect("endpoint pair is nonempty")
    }
}

struct EndpointDiscovery<T> {
    pairs: Vec<EndpointPair<T>>,
    supported_usb_present: bool,
    config_interface_present: bool,
    unlinked_receiver_present: bool,
    first_error: Option<Error>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(
        "SteelSeries Aerox 3 Wireless Gen 2 is not connected over wired USB or a linked 2.4 GHz receiver"
    )]
    DeviceNotConnected,
    #[error(
        "SteelSeries Aerox 3 Wireless Gen 2 USB endpoint was found, but configuration HID interface 3 was not"
    )]
    InterfaceNotFound,
    #[error(
        "multiple supported SteelSeries devices are connected:{available}\n\nSpecify one with --device <ID>."
    )]
    MultipleDevices { available: String },
    #[error("no usable SteelSeries device with ID {requested} was found.{available}")]
    RequestedDeviceNotFound {
        requested: String,
        available: String,
    },
    #[error("Aerox 3 Wireless Gen 2 receiver found, but the mouse is not connected over 2.4 GHz")]
    ReceiverLinkUnavailable,
    #[error("failed to initialize HID access: {0}")]
    HidInitialization(#[source] hidapi::HidError),
    #[error("failed to open SteelSeries Aerox 3 Wireless Gen 2 configuration interface: {0}")]
    Open(#[source] hidapi::HidError),
    #[error(
        "failed to open SteelSeries Aerox 3 Wireless Gen 2 configuration interface: {source}\n\nSteelSeries Linux udev permissions may not be installed or active.\nInstall the project's udev rules and reconnect the device."
    )]
    OpenPermissionDenied {
        #[source]
        source: hidapi::HidError,
    },
    #[error("failed to write HID command 0x{command:02x}: {source}")]
    Write {
        command: u8,
        #[source]
        source: hidapi::HidError,
    },
    #[error("HID command 0x{command:02x} wrote {actual} bytes; expected {expected}")]
    ShortWrite {
        command: u8,
        expected: usize,
        actual: usize,
    },
    #[error("timed out waiting for response 0x{expected:02x}{observed}")]
    ReadTimeout { expected: u8, observed: String },
    #[error("failed to read response to HID command 0x{command:02x}: {source}")]
    Read {
        command: u8,
        #[source]
        source: hidapi::HidError,
    },
    #[error(
        "malformed response to command 0x{command:02x}: expected at least {expected} bytes, received {actual}"
    )]
    MalformedResponse {
        command: u8,
        expected: usize,
        actual: usize,
    },
    #[error("unexpected response command byte: expected 0x{expected:02x}, received 0x{actual:02x}")]
    UnexpectedCommand { expected: u8, actual: u8 },
    #[error("DPI configuration must contain between 1 and 5 stages (received {0})")]
    InvalidDpiStageCount(usize),
    #[error("{axis}-axis DPI {value} is invalid; DPI must be between 50 and 26000 in steps of 50")]
    InvalidDpiValue { axis: &'static str, value: u16 },
    #[error("active DPI stage index {active} is outside the {stage_count} configured stages")]
    InvalidDpiActiveIndex { active: usize, stage_count: usize },
    #[error("stage ID {0} is outside the supported range 1..=5")]
    InvalidStageId(usize),
    #[error(
        "stage ID {stage_id} is not configured; current configuration contains {stage_count} stages"
    )]
    StageNotConfigured { stage_id: usize, stage_count: usize },
    #[error("unknown lift-off-distance protocol value 0x{0:02x}; expected 0x00 or 0x01")]
    UnknownLiftOffDistance(u8),
    #[error("unknown polling-rate protocol code 0x{0:02x}")]
    UnknownPollingCode(u8),
    #[error("unsupported polling rate {0} Hz; expected 125, 250, 500, 1000, 2000, or 4000 Hz")]
    UnsupportedPollingRate(u16),
    #[error("wired polling rate supports a maximum of 1000 Hz")]
    WiredPollingTooHigh,
    #[error("unknown low-power polling protocol code 0x{0:02x}")]
    UnknownLowPowerPollingCode(u8),
    #[error("unsupported low-power polling rate {0} Hz; expected 125, 250, or 500 Hz")]
    UnsupportedLowPowerPollingRate(u16),
    #[error("invalid {field} state 0x{value:02x}; expected 0x00 or 0x01")]
    InvalidPowerBoolean { field: &'static str, value: u8 },
    #[error("Auto Low Power threshold must be between 5% and 25% (received {0}%)")]
    InvalidAutoLowPowerThreshold(u8),
    #[error("sleep timer must be at least 1 minute; zero/off encoding is not supported")]
    SleepTimerZero,
    #[error("sleep timer is too large. Maximum encodable value is 71582 minutes")]
    SleepTimerTooLarge,
    #[error("invalid sleep timer value {0} ms; zero/off encoding is not supported")]
    InvalidSleepTimerMilliseconds(u32),
    #[error("invalid {field} state 0x{value:02x}; expected 0x00 or 0x01")]
    InvalidWirelessFeatureBoolean { field: &'static str, value: u8 },
    #[error("invalid Scroll Jump Protection state 0x{0:02x}; expected 0x00 or 0x01")]
    InvalidScrollJumpEnabled(u8),
    #[error(
        "unsupported Scroll Jump Protection delay {0} ms; expected 100-1500 ms in 100 ms steps"
    )]
    InvalidScrollJumpDelay(u16),
    #[error("malformed battery response: invalid charging state 0x{0:02x}; expected 0x00 or 0x01")]
    InvalidBatteryChargingState(u8),
    #[error(
        "malformed battery response: invalid percentage {0}; expected 0..=100 or unavailable marker 0xff"
    )]
    InvalidBatteryPercentage(u8),
    #[error("device identity response is empty")]
    EmptyDeviceIdentity,
    #[error("malformed device identity response: missing zero terminator")]
    UnterminatedDeviceIdentity,
    #[error("malformed device identity response: identity is not ASCII")]
    InvalidDeviceIdentity,
    #[error("invalid receiver link state 0x{0:02x}; expected 0x00 or 0x01")]
    InvalidReceiverLinkState(u8),
}

impl TryFrom<u16> for PollingRate {
    type Error = Error;

    fn try_from(hz: u16) -> Result<Self, Self::Error> {
        match hz {
            125 => Ok(Self::Hz125),
            250 => Ok(Self::Hz250),
            500 => Ok(Self::Hz500),
            1000 => Ok(Self::Hz1000),
            2000 => Ok(Self::Hz2000),
            4000 => Ok(Self::Hz4000),
            _ => Err(Error::UnsupportedPollingRate(hz)),
        }
    }
}

impl TryFrom<u8> for PollingRate {
    type Error = Error;

    fn try_from(code: u8) -> Result<Self, Self::Error> {
        match code {
            0x00 => Ok(Self::Hz4000),
            0x01 => Ok(Self::Hz2000),
            0x02 => Ok(Self::Hz1000),
            0x03 => Ok(Self::Hz500),
            0x04 => Ok(Self::Hz250),
            0x05 => Ok(Self::Hz125),
            _ => Err(Error::UnknownPollingCode(code)),
        }
    }
}

impl From<PollingRate> for u8 {
    fn from(rate: PollingRate) -> Self {
        match rate {
            PollingRate::Hz4000 => 0x00,
            PollingRate::Hz2000 => 0x01,
            PollingRate::Hz1000 => 0x02,
            PollingRate::Hz500 => 0x03,
            PollingRate::Hz250 => 0x04,
            PollingRate::Hz125 => 0x05,
        }
    }
}

impl TryFrom<u16> for LowPowerPollingRate {
    type Error = Error;

    fn try_from(hz: u16) -> Result<Self, Self::Error> {
        match hz {
            125 => Ok(Self::Hz125),
            250 => Ok(Self::Hz250),
            500 => Ok(Self::Hz500),
            _ => Err(Error::UnsupportedLowPowerPollingRate(hz)),
        }
    }
}

impl TryFrom<u8> for LowPowerPollingRate {
    type Error = Error;

    fn try_from(code: u8) -> Result<Self, Self::Error> {
        match code {
            0x05 => Ok(Self::Hz125),
            0x04 => Ok(Self::Hz250),
            0x03 => Ok(Self::Hz500),
            _ => Err(Error::UnknownLowPowerPollingCode(code)),
        }
    }
}

impl From<LowPowerPollingRate> for u8 {
    fn from(rate: LowPowerPollingRate) -> Self {
        match rate {
            LowPowerPollingRate::Hz125 => 0x05,
            LowPowerPollingRate::Hz250 => 0x04,
            LowPowerPollingRate::Hz500 => 0x03,
        }
    }
}

impl TryFrom<u8> for LiftOffDistance {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Self::Low),
            0x01 => Ok(Self::High),
            _ => Err(Error::UnknownLiftOffDistance(value)),
        }
    }
}

impl From<LiftOffDistance> for u8 {
    fn from(value: LiftOffDistance) -> Self {
        match value {
            LiftOffDistance::Low => 0x00,
            LiftOffDistance::High => 0x01,
        }
    }
}

pub struct Aerox3WirelessGen2 {
    device: HidDevice,
}

impl Aerox3WirelessGen2 {
    pub const CAPABILITIES: DeviceCapabilities = DeviceCapabilities {
        dpi: DpiCapabilities {
            min: 50,
            max: 26_000,
            step: 50,
            max_stages: 5,
        },
        polling: PollingCapabilities {
            wireless: &[
                PollingRate::Hz125,
                PollingRate::Hz250,
                PollingRate::Hz500,
                PollingRate::Hz1000,
                PollingRate::Hz2000,
                PollingRate::Hz4000,
            ],
            wired: &[
                PollingRate::Hz125,
                PollingRate::Hz250,
                PollingRate::Hz500,
                PollingRate::Hz1000,
            ],
        },
        power: PowerCapabilities {
            low_power_polling: &[
                LowPowerPollingRate::Hz125,
                LowPowerPollingRate::Hz250,
                LowPowerPollingRate::Hz500,
            ],
            auto_low_power_threshold_min: 5,
            auto_low_power_threshold_max: 25,
            sleep_timer_min_minutes: 1,
            sleep_timer_max_minutes: 71_582,
        },
        wireless_features: WirelessFeatureCapabilities {
            wireless_stability: true,
            bluetooth_smoothing: true,
        },
        scroll_jump: ScrollJumpCapabilities {
            supported: true,
            delay_min_ms: 100,
            delay_max_ms: 1_500,
            delay_step_ms: 100,
        },
    };

    pub fn open() -> Result<Self, Error> {
        Self::open_selected(None)
    }

    pub fn open_selected(requested_identity: Option<&str>) -> Result<Self, Error> {
        let api = HidApi::new().map_err(Error::HidInitialization)?;
        let discovery = Self::discover_with_api(&api);

        if discovery.pairs.is_empty() {
            if discovery.supported_usb_present && !discovery.config_interface_present {
                return Err(Error::InterfaceNotFound);
            }
            if let Some(error) = discovery.first_error {
                return Err(error);
            }
            if requested_identity.is_none() && discovery.unlinked_receiver_present {
                return Err(Error::ReceiverLinkUnavailable);
            }
        }

        let pair = select_endpoint_pair(discovery.pairs, requested_identity)?;
        Ok(Self {
            device: pair.into_selected_endpoint(),
        })
    }

    pub fn discover() -> Result<Vec<PhysicalDevice>, Error> {
        let api = HidApi::new().map_err(Error::HidInitialization)?;
        let discovery = Self::discover_with_api(&api);

        if discovery.pairs.is_empty() {
            if discovery.supported_usb_present && !discovery.config_interface_present {
                return Err(Error::InterfaceNotFound);
            }
            if let Some(error) = discovery.first_error {
                return Err(error);
            }
        }

        Ok(discovery
            .pairs
            .iter()
            .map(EndpointPair::physical_device)
            .collect())
    }

    /// Returns whether at least one usable physical mouse is discoverable.
    pub fn is_connected() -> Result<bool, Error> {
        Ok(!Self::discover()?.is_empty())
    }

    fn discover_with_api(api: &HidApi) -> EndpointDiscovery<HidDevice> {
        let mut supported_usb_present = false;
        let mut config_interface_present = false;
        let mut candidates: Vec<(EndpointKind, CString)> = Vec::new();

        for info in api.device_list() {
            let Some(kind) = supported_endpoint_kind(info.vendor_id(), info.product_id()) else {
                continue;
            };
            supported_usb_present = true;
            if info.interface_number() == CONFIG_INTERFACE {
                config_interface_present = true;
                candidates.push((kind, info.path().to_owned()));
            }
        }

        let mut identified = Vec::new();
        let mut unlinked_receiver_present = false;
        let mut first_error = None;

        for (kind, path) in candidates {
            let device = match api.open_path(path.as_c_str()) {
                Ok(device) => device,
                Err(error) => {
                    first_error.get_or_insert_with(|| open_error(error));
                    continue;
                }
            };
            let endpoint = Self { device };

            if kind == EndpointKind::Receiver2_4Ghz {
                match endpoint.get_receiver_link_active() {
                    Ok(false) => {
                        unlinked_receiver_present = true;
                        continue;
                    }
                    Ok(true) => {}
                    Err(error) => {
                        first_error.get_or_insert(error);
                        continue;
                    }
                }
            }

            match endpoint.get_device_identity() {
                Ok(identity) => identified.push(IdentifiedEndpoint {
                    identity,
                    kind,
                    endpoint: endpoint.device,
                }),
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }

        EndpointDiscovery {
            pairs: group_identified_endpoints(identified),
            supported_usb_present,
            config_interface_present,
            unlinked_receiver_present,
            first_error,
        }
    }

    fn get_device_identity(&self) -> Result<DeviceIdentity, Error> {
        let response = send_identity_query_with_retry(
            |report| self.write_report(report),
            |remaining| self.read_report_timeout(CMD_GET_DEVICE_IDENTITY, remaining),
        )?;
        parse_device_identity_response(&response)
    }

    fn get_receiver_link_active(&self) -> Result<bool, Error> {
        let response = self.send_command_and_expect(
            command_report(CMD_GET_RECEIVER_LINK),
            CMD_GET_RECEIVER_LINK,
        )?;
        parse_receiver_link_response(&response)
    }

    pub fn get_dpi_config(&self) -> Result<DpiConfig, Error> {
        let response = self.send_command_and_expect(command_report(CMD_GET_DPI), CMD_GET_DPI)?;
        parse_dpi_response(&response)
    }

    /// Writes a complete DPI configuration. Call [`Self::commit`] to persist it.
    pub fn set_dpi_config(&self, config: &DpiConfig) -> Result<(), Error> {
        self.send_command_and_expect(encode_dpi_config(config)?, CMD_SET_DPI)?;
        Ok(())
    }

    /// Selects an existing stage by its one-based ID. Call [`Self::commit`] to persist it.
    pub fn set_active_dpi_stage(&self, stage_id: usize) -> Result<(), Error> {
        let current = self.get_dpi_config()?;
        let updated = config_with_active_stage(&current, stage_id)?;
        self.set_dpi_config(&updated)
    }

    /// Changes one stage's LOD while preserving the complete sensor configuration.
    /// Call [`Self::commit`] to persist it.
    pub fn set_lift_off_distance(
        &self,
        stage_id: usize,
        lod: LiftOffDistance,
    ) -> Result<(), Error> {
        let current = self.get_dpi_config()?;
        let updated = config_with_lift_off_distance(&current, stage_id, lod)?;
        self.set_dpi_config(&updated)
    }

    pub fn get_polling_config(&self) -> Result<PollingConfig, Error> {
        let response =
            self.send_command_and_expect(command_report(CMD_GET_POLLING), CMD_GET_POLLING)?;
        parse_polling_response(&response)
    }

    pub fn get_battery_status(&self) -> Result<BatteryStatus, Error> {
        let response =
            self.send_command_and_expect(command_report(CMD_GET_BATTERY), CMD_GET_BATTERY)?;
        parse_battery_response(&response)
    }

    pub fn get_power_config(&self) -> Result<PowerConfig, Error> {
        let response =
            self.send_command_and_expect(command_report(CMD_GET_POWER), CMD_GET_POWER)?;
        parse_power_response(&response)
    }

    /// Changes Low Power Mode while preserving all other power fields.
    /// Call [`Self::commit`] to persist it.
    pub fn set_low_power_enabled(&self, enabled: bool) -> Result<(), Error> {
        let current = self.get_power_config()?;
        self.set_power_config(&power_with_low_power_enabled(current, enabled))
    }

    /// Changes Low Power polling while preserving all other power fields.
    /// Call [`Self::commit`] to persist it.
    pub fn set_low_power_polling(&self, rate: LowPowerPollingRate) -> Result<(), Error> {
        let current = self.get_power_config()?;
        self.set_power_config(&power_with_low_power_polling(current, rate))
    }

    /// Changes Auto Low Power while preserving all other power fields.
    /// Call [`Self::commit`] to persist it.
    pub fn set_auto_low_power_enabled(&self, enabled: bool) -> Result<(), Error> {
        let current = self.get_power_config()?;
        self.set_power_config(&power_with_auto_low_power_enabled(current, enabled))
    }

    /// Changes the Auto Low Power threshold while preserving all other power fields.
    /// Call [`Self::commit`] to persist it.
    pub fn set_auto_low_power_threshold(
        &self,
        threshold: AutoLowPowerThreshold,
    ) -> Result<(), Error> {
        let current = self.get_power_config()?;
        self.set_power_config(&power_with_auto_low_power_threshold(current, threshold))
    }

    /// Changes the sleep timer while preserving all other power fields.
    /// Call [`Self::commit`] to persist it.
    pub fn set_sleep_timer(&self, timer: SleepTimer) -> Result<(), Error> {
        let current = self.get_power_config()?;
        self.set_power_config(&power_with_sleep_timer(current, timer))
    }

    fn set_power_config(&self, config: &PowerConfig) -> Result<(), Error> {
        self.send_command_and_expect(encode_power_config(config)?, CMD_SET_POWER)?;
        Ok(())
    }

    pub fn get_wireless_features(&self) -> Result<WirelessFeatures, Error> {
        let response = self.send_command_and_expect(
            command_report(CMD_GET_WIRELESS_FEATURES),
            CMD_GET_WIRELESS_FEATURES,
        )?;
        parse_wireless_features_response(&response)
    }

    /// Changes Wireless Stability Enhancement while preserving Bluetooth Smoothing and all
    /// reserved bytes. Call [`Self::commit`] to persist it.
    pub fn set_wireless_stability(&self, enabled: bool) -> Result<(), Error> {
        let current = self.get_wireless_features()?;
        let updated = wireless_features_with_stability(current, enabled);
        self.set_wireless_features(&updated)
    }

    /// Changes Bluetooth Smoothing while preserving Wireless Stability Enhancement and all
    /// reserved bytes. Call [`Self::commit`] to persist it.
    pub fn set_bluetooth_smoothing(&self, enabled: bool) -> Result<(), Error> {
        let current = self.get_wireless_features()?;
        let updated = wireless_features_with_bluetooth_smoothing(current, enabled);
        self.set_wireless_features(&updated)
    }

    fn set_wireless_features(&self, features: &WirelessFeatures) -> Result<(), Error> {
        self.send_command_and_expect(
            encode_wireless_features(features),
            CMD_SET_WIRELESS_FEATURES,
        )?;
        Ok(())
    }

    pub fn get_scroll_jump_config(&self) -> Result<ScrollJumpConfig, Error> {
        let response =
            self.send_command_and_expect(command_report(CMD_GET_SCROLL_JUMP), CMD_GET_SCROLL_JUMP)?;
        parse_scroll_jump_response(&response)
    }

    /// Changes Scroll Jump Protection while preserving its delay and all reserved bytes.
    /// Call [`Self::commit`] to persist it.
    pub fn set_scroll_jump_enabled(&self, enabled: bool) -> Result<(), Error> {
        let current = self.get_scroll_jump_config()?;
        let updated = scroll_jump_with_enabled(current, enabled);
        self.set_scroll_jump_config(&updated)
    }

    /// Changes the Scroll Jump Protection delay while preserving its enabled state and all
    /// reserved bytes. Call [`Self::commit`] to persist it.
    pub fn set_scroll_jump_delay(&self, delay_ms: u16) -> Result<(), Error> {
        validate_scroll_jump_delay(delay_ms)?;
        let current = self.get_scroll_jump_config()?;
        let updated = scroll_jump_with_delay(current, delay_ms)?;
        self.set_scroll_jump_config(&updated)
    }

    fn set_scroll_jump_config(&self, config: &ScrollJumpConfig) -> Result<(), Error> {
        self.send_command_and_expect(encode_scroll_jump_config(config), CMD_SET_SCROLL_JUMP)?;
        Ok(())
    }

    /// Changes wireless polling while preserving wired polling. Call [`Self::commit`] to persist it.
    pub fn set_wireless_polling(&self, rate: PollingRate) -> Result<(), Error> {
        let current = self.get_polling_config()?;
        let updated = polling_with_wireless(current, rate);
        self.send_command_and_expect(encode_polling_config(updated)?, CMD_SET_POLLING)?;
        Ok(())
    }

    /// Changes wired polling while preserving wireless polling. Call [`Self::commit`] to persist it.
    pub fn set_wired_polling(&self, rate: PollingRate) -> Result<(), Error> {
        validate_wired_polling_rate(rate)?;
        let current = self.get_polling_config()?;
        let updated = polling_with_wired(current, rate)?;
        self.send_command_and_expect(encode_polling_config(updated)?, CMD_SET_POLLING)?;
        Ok(())
    }

    pub fn commit(&self) -> Result<(), Error> {
        self.send_command_and_expect(command_report(CMD_COMMIT), CMD_COMMIT)?;
        Ok(())
    }

    fn send_command_and_expect(
        &self,
        report: [u8; REPORT_SIZE],
        expected_response_command: u8,
    ) -> Result<[u8; REPORT_SIZE], Error> {
        self.write_report(&report)?;
        wait_for_expected_response(expected_response_command, RESPONSE_TIMEOUT, |remaining| {
            self.read_report_timeout(expected_response_command, remaining)
        })
    }

    fn read_report_timeout(
        &self,
        expected_command: u8,
        timeout: Duration,
    ) -> Result<Option<[u8; REPORT_SIZE]>, Error> {
        let mut response = [0_u8; REPORT_SIZE];
        let timeout_ms = i32::try_from(timeout.as_millis().max(1)).unwrap_or(i32::MAX);
        let read = self
            .device
            .read_timeout(&mut response, timeout_ms)
            .map_err(|source| Error::Read {
                command: expected_command,
                source,
            })?;
        if read == 0 {
            return Ok(None);
        }
        if read != REPORT_SIZE {
            return Err(Error::MalformedResponse {
                command: expected_command,
                expected: REPORT_SIZE,
                actual: read,
            });
        }
        Ok(Some(response))
    }

    fn write_report(&self, report: &[u8; REPORT_SIZE]) -> Result<(), Error> {
        let command = report[0];
        let mut hid_report = [0_u8; WRITE_SIZE];
        hid_report[1..].copy_from_slice(report);
        let written = self
            .device
            .write(&hid_report)
            .map_err(|source| Error::Write { command, source })?;
        if written != WRITE_SIZE {
            return Err(Error::ShortWrite {
                command,
                expected: WRITE_SIZE,
                actual: written,
            });
        }
        Ok(())
    }
}

fn supported_endpoint_kind(vendor_id: u16, product_id: u16) -> Option<EndpointKind> {
    if vendor_id != VENDOR_ID {
        return None;
    }
    match product_id {
        PID_RECEIVER => Some(EndpointKind::Receiver2_4Ghz),
        PID_WIRED => Some(EndpointKind::Wired),
        _ => None,
    }
}

fn select_endpoint_pair<T>(
    mut pairs: Vec<EndpointPair<T>>,
    requested_identity: Option<&str>,
) -> Result<EndpointPair<T>, Error> {
    if let Some(requested) = requested_identity {
        if let Some(index) = pairs
            .iter()
            .position(|pair| pair.identity.as_str() == requested)
        {
            return Ok(pairs.swap_remove(index));
        }
        return Err(Error::RequestedDeviceNotFound {
            requested: requested.to_owned(),
            available: available_identities_suffix(&pairs),
        });
    }

    match pairs.len() {
        0 => Err(Error::DeviceNotConnected),
        1 => Ok(pairs.pop().expect("one endpoint pair exists")),
        _ => Err(Error::MultipleDevices {
            available: ambiguity_identity_list(&pairs),
        }),
    }
}

fn sorted_identities<T>(pairs: &[EndpointPair<T>]) -> Vec<&str> {
    let mut identities: Vec<_> = pairs.iter().map(|pair| pair.identity.as_str()).collect();
    identities.sort_unstable();
    identities
}

fn ambiguity_identity_list<T>(pairs: &[EndpointPair<T>]) -> String {
    sorted_identities(pairs)
        .into_iter()
        .map(|identity| format!("\n  {identity}"))
        .collect()
}

fn available_identities_suffix<T>(pairs: &[EndpointPair<T>]) -> String {
    if pairs.is_empty() {
        return String::new();
    }
    let identities = sorted_identities(pairs)
        .into_iter()
        .map(|identity| format!("  {identity}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("\n\nAvailable device IDs:\n{identities}")
}

fn open_error(error: hidapi::HidError) -> Error {
    if hid_error_is_permission_denied(&error) {
        Error::OpenPermissionDenied { source: error }
    } else {
        Error::Open(error)
    }
}

fn hid_error_is_permission_denied(error: &hidapi::HidError) -> bool {
    match error {
        hidapi::HidError::IoError { error } => error.kind() == std::io::ErrorKind::PermissionDenied,
        hidapi::HidError::HidApiError { message } => {
            let message = message.to_ascii_lowercase();
            message.contains("permission denied") || message.contains("access denied")
        }
        _ => false,
    }
}

fn group_identified_endpoints<T>(
    endpoints: impl IntoIterator<Item = IdentifiedEndpoint<T>>,
) -> Vec<EndpointPair<T>> {
    let mut pairs: Vec<EndpointPair<T>> = Vec::new();

    for endpoint in endpoints {
        let IdentifiedEndpoint {
            identity,
            kind,
            endpoint,
        } = endpoint;
        if let Some(pair) = pairs.iter_mut().find(|pair| pair.identity == identity) {
            match kind {
                EndpointKind::Wired => {
                    pair.wired.get_or_insert(endpoint);
                }
                EndpointKind::Receiver2_4Ghz => {
                    pair.receiver.get_or_insert(endpoint);
                }
            }
            continue;
        }

        let (wired, receiver) = match kind {
            EndpointKind::Wired => (Some(endpoint), None),
            EndpointKind::Receiver2_4Ghz => (None, Some(endpoint)),
        };
        pairs.push(EndpointPair {
            identity,
            wired,
            receiver,
        });
    }

    pairs
}

fn wait_for_expected_response<F>(
    expected_command: u8,
    timeout: Duration,
    read_report: F,
) -> Result<[u8; REPORT_SIZE], Error>
where
    F: FnMut(Duration) -> Result<Option<[u8; REPORT_SIZE]>, Error>,
{
    let mut observed_commands = Vec::new();
    wait_for_expected_response_with_observed(
        expected_command,
        timeout,
        &mut observed_commands,
        read_report,
    )
}

fn wait_for_expected_response_with_observed<F>(
    expected_command: u8,
    timeout: Duration,
    observed_commands: &mut Vec<u8>,
    mut read_report: F,
) -> Result<[u8; REPORT_SIZE], Error>
where
    F: FnMut(Duration) -> Result<Option<[u8; REPORT_SIZE]>, Error>,
{
    let deadline = Instant::now() + timeout;

    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err(response_timeout_error(expected_command, observed_commands));
        };
        if remaining.is_zero() {
            return Err(response_timeout_error(expected_command, observed_commands));
        }

        let Some(report) = read_report(remaining)? else {
            return Err(response_timeout_error(expected_command, observed_commands));
        };

        let command = report[0];
        if command == expected_command {
            return Ok(report);
        }
        if !observed_commands.contains(&command) {
            observed_commands.push(command);
        }
    }
}

fn send_identity_query_with_retry<W, R>(
    mut write_report: W,
    mut read_report: R,
) -> Result<[u8; REPORT_SIZE], Error>
where
    W: FnMut(&[u8; REPORT_SIZE]) -> Result<(), Error>,
    R: FnMut(Duration) -> Result<Option<[u8; REPORT_SIZE]>, Error>,
{
    let report = command_report(CMD_GET_DEVICE_IDENTITY);
    let overall_deadline = Instant::now() + RESPONSE_TIMEOUT;
    let mut observed_commands = Vec::new();

    for _ in 0..IDENTITY_MAX_ATTEMPTS {
        let Some(overall_remaining) = overall_deadline.checked_duration_since(Instant::now())
        else {
            break;
        };
        if overall_remaining.is_zero() {
            break;
        }

        write_report(&report)?;
        let Some(overall_remaining) = overall_deadline.checked_duration_since(Instant::now())
        else {
            break;
        };
        if overall_remaining.is_zero() {
            break;
        }
        let attempt_timeout = IDENTITY_ATTEMPT_TIMEOUT.min(overall_remaining);
        match wait_for_expected_response_with_observed(
            CMD_GET_DEVICE_IDENTITY,
            attempt_timeout,
            &mut observed_commands,
            &mut read_report,
        ) {
            Ok(response) => return Ok(response),
            Err(Error::ReadTimeout {
                expected: CMD_GET_DEVICE_IDENTITY,
                ..
            }) => {}
            Err(error) => return Err(error),
        }
    }

    Err(response_timeout_error(
        CMD_GET_DEVICE_IDENTITY,
        &observed_commands,
    ))
}

fn response_timeout_error(expected: u8, observed_commands: &[u8]) -> Error {
    let observed = if observed_commands.is_empty() {
        String::new()
    } else {
        let commands = observed_commands
            .iter()
            .map(|command| format!("0x{command:02x}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(" (observed: {commands})")
    };
    Error::ReadTimeout { expected, observed }
}

fn command_report(command: u8) -> [u8; REPORT_SIZE] {
    let mut report = [0_u8; REPORT_SIZE];
    report[0] = command;
    report
}

fn validate_dpi_config(config: &DpiConfig) -> Result<(), Error> {
    if !(1..=Aerox3WirelessGen2::CAPABILITIES.dpi.max_stages).contains(&config.stages.len()) {
        return Err(Error::InvalidDpiStageCount(config.stages.len()));
    }
    if config.active >= config.stages.len() {
        return Err(Error::InvalidDpiActiveIndex {
            active: config.active,
            stage_count: config.stages.len(),
        });
    }
    for stage in &config.stages {
        validate_dpi_value("X", stage.x)?;
        validate_dpi_value("Y", stage.y)?;
    }
    Ok(())
}

fn validate_dpi_value(axis: &'static str, value: u16) -> Result<(), Error> {
    let capabilities = Aerox3WirelessGen2::CAPABILITIES.dpi;
    if value < capabilities.min
        || value > capabilities.max
        || !value.is_multiple_of(capabilities.step)
    {
        return Err(Error::InvalidDpiValue { axis, value });
    }
    Ok(())
}

/// Validates stage count and both axes against this model's DPI capabilities.
pub fn validate_dpi_values(dpis: &[(u16, u16)]) -> Result<(), Error> {
    if !(1..=Aerox3WirelessGen2::CAPABILITIES.dpi.max_stages).contains(&dpis.len()) {
        return Err(Error::InvalidDpiStageCount(dpis.len()));
    }
    for &(x, y) in dpis {
        validate_dpi_value("X", x)?;
        validate_dpi_value("Y", y)?;
    }
    Ok(())
}

fn parse_dpi_response(response: &[u8]) -> Result<DpiConfig, Error> {
    ensure_response_header(response, CMD_GET_DPI, 3)?;
    let stage_count = usize::from(response[1]);
    if !(1..=Aerox3WirelessGen2::CAPABILITIES.dpi.max_stages).contains(&stage_count) {
        return Err(Error::InvalidDpiStageCount(stage_count));
    }

    let required = 3 + stage_count * 5;
    if response.len() < required {
        return Err(Error::MalformedResponse {
            command: CMD_GET_DPI,
            expected: required,
            actual: response.len(),
        });
    }

    let active = usize::from(response[2]);
    let mut stages = Vec::with_capacity(stage_count);
    for offset in (3..required).step_by(5) {
        stages.push(DpiStage {
            x: read_u16_le(response, offset),
            y: read_u16_le(response, offset + 2),
            lod: LiftOffDistance::try_from(response[offset + 4])?,
        });
    }
    let config = DpiConfig { stages, active };
    validate_dpi_config(&config)?;
    Ok(config)
}

fn encode_dpi_config(config: &DpiConfig) -> Result<[u8; REPORT_SIZE], Error> {
    validate_dpi_config(config)?;
    let mut report = command_report(CMD_SET_DPI);
    report[1] = config.stages.len() as u8;
    report[2] = config.active as u8;
    for (index, stage) in config.stages.iter().enumerate() {
        let offset = 3 + index * 5;
        write_u16_le(&mut report, offset, stage.x);
        write_u16_le(&mut report, offset + 2, stage.y);
        report[offset + 4] = u8::from(stage.lod);
    }
    Ok(report)
}

/// Builds a DPI-stage configuration and preserves the active scalar DPI when possible.
pub fn config_from_dpi_values(
    current: &DpiConfig,
    dpis: &[(u16, u16)],
) -> Result<DpiConfig, Error> {
    validate_dpi_values(dpis)?;
    validate_dpi_config(current)?;

    let active_value = current.stages[current.active];
    let active = if active_value.x == active_value.y {
        dpis.iter()
            .position(|(x, y)| *x == active_value.x && *y == active_value.y)
            .unwrap_or(0)
    } else {
        0
    };
    Ok(DpiConfig {
        stages: dpis
            .iter()
            .enumerate()
            .map(|(index, &(x, y))| DpiStage {
                x,
                y,
                lod: current
                    .stages
                    .get(index)
                    .map_or(LiftOffDistance::Low, |stage| stage.lod),
            })
            .collect(),
        active,
    })
}

fn configured_stage_index(current: &DpiConfig, stage_id: usize) -> Result<usize, Error> {
    validate_dpi_config(current)?;
    let index = stage_id_to_index(stage_id)?;
    if index >= current.stages.len() {
        return Err(Error::StageNotConfigured {
            stage_id,
            stage_count: current.stages.len(),
        });
    }
    Ok(index)
}

pub fn validate_stage_id(stage_id: usize) -> Result<(), Error> {
    stage_id_to_index(stage_id).map(|_| ())
}

fn stage_id_to_index(stage_id: usize) -> Result<usize, Error> {
    if !(1..=Aerox3WirelessGen2::CAPABILITIES.dpi.max_stages).contains(&stage_id) {
        return Err(Error::InvalidStageId(stage_id));
    }
    Ok(stage_id - 1)
}

/// Returns a copy with only the active index changed; stage values remain exact.
pub fn config_with_active_stage(current: &DpiConfig, stage_id: usize) -> Result<DpiConfig, Error> {
    let active = configured_stage_index(current, stage_id)?;
    Ok(DpiConfig {
        stages: current.stages.clone(),
        active,
    })
}

/// Returns a copy with only one stage's lift-off distance changed.
pub fn config_with_lift_off_distance(
    current: &DpiConfig,
    stage_id: usize,
    lod: LiftOffDistance,
) -> Result<DpiConfig, Error> {
    let index = configured_stage_index(current, stage_id)?;
    let mut updated = current.clone();
    updated.stages[index].lod = lod;
    Ok(updated)
}

fn parse_polling_response(response: &[u8]) -> Result<PollingConfig, Error> {
    ensure_response_header(response, CMD_GET_POLLING, 3)?;
    let config = PollingConfig {
        wireless: PollingRate::try_from(response[1])?,
        wired: PollingRate::try_from(response[2])?,
    };
    validate_wired_polling_rate(config.wired)?;
    Ok(config)
}

fn parse_battery_response(response: &[u8]) -> Result<BatteryStatus, Error> {
    ensure_response_header(response, CMD_GET_BATTERY, 3)?;
    let charging = match response[1] {
        0x00 => false,
        0x01 => true,
        value => return Err(Error::InvalidBatteryChargingState(value)),
    };

    match response[2] {
        0xff => Ok(BatteryStatus::Unavailable),
        percent @ 0..=100 => Ok(BatteryStatus::Available { percent, charging }),
        value => Err(Error::InvalidBatteryPercentage(value)),
    }
}

fn parse_power_response(response: &[u8]) -> Result<PowerConfig, Error> {
    ensure_response_header(response, CMD_GET_POWER, REPORT_SIZE)?;
    let threshold = auto_low_power_threshold(response[8])?;
    let sleep_milliseconds = read_u32_le(response, 3);
    if sleep_milliseconds == 0 {
        return Err(Error::InvalidSleepTimerMilliseconds(sleep_milliseconds));
    }
    let mut reserved = [0_u8; POWER_RESERVED_SIZE];
    reserved.copy_from_slice(&response[POWER_RESERVED_OFFSET..REPORT_SIZE]);
    Ok(PowerConfig {
        low_power_enabled: parse_power_boolean("Low Power Mode", response[1])?,
        low_power_polling: LowPowerPollingRate::try_from(response[2])?,
        sleep_timer: SleepTimer::from_milliseconds(sleep_milliseconds),
        auto_low_power_enabled: parse_power_boolean("Auto Low Power", response[7])?,
        auto_low_power_threshold: threshold,
        reserved,
    })
}

fn parse_power_boolean(field: &'static str, value: u8) -> Result<bool, Error> {
    match value {
        0x00 => Ok(false),
        0x01 => Ok(true),
        value => Err(Error::InvalidPowerBoolean { field, value }),
    }
}

fn encode_power_config(config: &PowerConfig) -> Result<[u8; REPORT_SIZE], Error> {
    validate_auto_low_power_threshold(config.auto_low_power_threshold.percent())?;
    if config.sleep_timer.milliseconds() == 0 {
        return Err(Error::InvalidSleepTimerMilliseconds(0));
    }
    let mut report = command_report(CMD_SET_POWER);
    report[1] = u8::from(config.low_power_enabled);
    report[2] = config.low_power_polling.into();
    write_u32_le(&mut report, 3, config.sleep_timer.milliseconds());
    report[7] = u8::from(config.auto_low_power_enabled);
    report[8] = config.auto_low_power_threshold.percent();
    report[POWER_RESERVED_OFFSET..].copy_from_slice(&config.reserved);
    Ok(report)
}

fn parse_wireless_features_response(response: &[u8]) -> Result<WirelessFeatures, Error> {
    ensure_response_header(response, CMD_GET_WIRELESS_FEATURES, REPORT_SIZE)?;
    let mut reserved = [0_u8; WIRELESS_FEATURES_RESERVED_SIZE];
    reserved.copy_from_slice(&response[WIRELESS_FEATURES_RESERVED_OFFSET..REPORT_SIZE]);
    Ok(WirelessFeatures {
        wireless_stability_enabled: parse_wireless_feature_boolean(
            "Wireless Stability Enhancement",
            response[1],
        )?,
        bluetooth_smoothing_enabled: parse_wireless_feature_boolean(
            "Bluetooth Smoothing",
            response[2],
        )?,
        reserved,
    })
}

fn parse_wireless_feature_boolean(field: &'static str, value: u8) -> Result<bool, Error> {
    match value {
        0x00 => Ok(false),
        0x01 => Ok(true),
        value => Err(Error::InvalidWirelessFeatureBoolean { field, value }),
    }
}

fn encode_wireless_features(features: &WirelessFeatures) -> [u8; REPORT_SIZE] {
    let mut report = command_report(CMD_SET_WIRELESS_FEATURES);
    report[1] = u8::from(features.wireless_stability_enabled);
    report[2] = u8::from(features.bluetooth_smoothing_enabled);
    report[WIRELESS_FEATURES_RESERVED_OFFSET..].copy_from_slice(&features.reserved);
    report
}

#[must_use]
pub fn wireless_features_with_stability(
    mut current: WirelessFeatures,
    enabled: bool,
) -> WirelessFeatures {
    current.wireless_stability_enabled = enabled;
    current
}

#[must_use]
pub fn wireless_features_with_bluetooth_smoothing(
    mut current: WirelessFeatures,
    enabled: bool,
) -> WirelessFeatures {
    current.bluetooth_smoothing_enabled = enabled;
    current
}

fn parse_scroll_jump_response(response: &[u8]) -> Result<ScrollJumpConfig, Error> {
    ensure_response_header(response, CMD_GET_SCROLL_JUMP, REPORT_SIZE)?;
    let enabled = match response[1] {
        0x00 => false,
        0x01 => true,
        value => return Err(Error::InvalidScrollJumpEnabled(value)),
    };
    let mut reserved = [0_u8; SCROLL_JUMP_RESERVED_SIZE];
    reserved.copy_from_slice(&response[SCROLL_JUMP_RESERVED_OFFSET..REPORT_SIZE]);
    Ok(ScrollJumpConfig {
        enabled,
        delay_ms: read_u16_le(response, 2),
        reserved,
    })
}

fn encode_scroll_jump_config(config: &ScrollJumpConfig) -> [u8; REPORT_SIZE] {
    let mut report = command_report(CMD_SET_SCROLL_JUMP);
    report[1] = u8::from(config.enabled);
    write_u16_le(&mut report, 2, config.delay_ms);
    report[SCROLL_JUMP_RESERVED_OFFSET..].copy_from_slice(&config.reserved);
    report
}

#[must_use]
pub fn scroll_jump_with_enabled(mut current: ScrollJumpConfig, enabled: bool) -> ScrollJumpConfig {
    current.enabled = enabled;
    current
}

pub fn scroll_jump_with_delay(
    mut current: ScrollJumpConfig,
    delay_ms: u16,
) -> Result<ScrollJumpConfig, Error> {
    validate_scroll_jump_delay(delay_ms)?;
    current.delay_ms = delay_ms;
    Ok(current)
}

pub fn validate_scroll_jump_delay(delay_ms: u16) -> Result<(), Error> {
    let capabilities = Aerox3WirelessGen2::CAPABILITIES.scroll_jump;
    if !(capabilities.delay_min_ms..=capabilities.delay_max_ms).contains(&delay_ms)
        || !delay_ms.is_multiple_of(capabilities.delay_step_ms)
    {
        return Err(Error::InvalidScrollJumpDelay(delay_ms));
    }
    Ok(())
}

pub fn auto_low_power_threshold(percent: u8) -> Result<AutoLowPowerThreshold, Error> {
    validate_auto_low_power_threshold(percent)?;
    Ok(AutoLowPowerThreshold::new(percent))
}

fn validate_auto_low_power_threshold(percent: u8) -> Result<(), Error> {
    let capabilities = Aerox3WirelessGen2::CAPABILITIES.power;
    if !(capabilities.auto_low_power_threshold_min..=capabilities.auto_low_power_threshold_max)
        .contains(&percent)
    {
        return Err(Error::InvalidAutoLowPowerThreshold(percent));
    }
    Ok(())
}

pub fn sleep_timer_from_minutes(minutes: u64) -> Result<SleepTimer, Error> {
    let capabilities = Aerox3WirelessGen2::CAPABILITIES.power;
    if minutes < capabilities.sleep_timer_min_minutes {
        return Err(Error::SleepTimerZero);
    }
    if minutes > capabilities.sleep_timer_max_minutes {
        return Err(Error::SleepTimerTooLarge);
    }
    let milliseconds = minutes
        .checked_mul(60_000)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(Error::SleepTimerTooLarge)?;
    Ok(SleepTimer::from_milliseconds(milliseconds))
}

#[must_use]
pub fn power_with_low_power_enabled(mut current: PowerConfig, enabled: bool) -> PowerConfig {
    current.low_power_enabled = enabled;
    current
}

#[must_use]
pub fn power_with_low_power_polling(
    mut current: PowerConfig,
    rate: LowPowerPollingRate,
) -> PowerConfig {
    current.low_power_polling = rate;
    current
}

#[must_use]
pub fn power_with_auto_low_power_enabled(mut current: PowerConfig, enabled: bool) -> PowerConfig {
    current.auto_low_power_enabled = enabled;
    current
}

#[must_use]
pub fn power_with_auto_low_power_threshold(
    mut current: PowerConfig,
    threshold: AutoLowPowerThreshold,
) -> PowerConfig {
    current.auto_low_power_threshold = threshold;
    current
}

#[must_use]
pub fn power_with_sleep_timer(mut current: PowerConfig, timer: SleepTimer) -> PowerConfig {
    current.sleep_timer = timer;
    current
}

fn parse_device_identity_response(response: &[u8]) -> Result<DeviceIdentity, Error> {
    ensure_response_header(response, CMD_GET_DEVICE_IDENTITY, 2)?;
    let identity_end = response[1..]
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(Error::UnterminatedDeviceIdentity)?;
    if identity_end == 0 {
        return Err(Error::EmptyDeviceIdentity);
    }

    let identity_bytes = &response[1..1 + identity_end];
    if !identity_bytes.is_ascii() {
        return Err(Error::InvalidDeviceIdentity);
    }
    let identity = std::str::from_utf8(identity_bytes)
        .map_err(|_| Error::InvalidDeviceIdentity)?
        .to_owned();
    Ok(DeviceIdentity::new(identity))
}

fn parse_receiver_link_response(response: &[u8]) -> Result<bool, Error> {
    ensure_response_header(response, CMD_GET_RECEIVER_LINK, 2)?;
    match response[1] {
        0x00 => Ok(false),
        0x01 => Ok(true),
        value => Err(Error::InvalidReceiverLinkState(value)),
    }
}

fn encode_polling_config(config: PollingConfig) -> Result<[u8; REPORT_SIZE], Error> {
    validate_wired_polling_rate(config.wired)?;
    let mut report = command_report(CMD_SET_POLLING);
    report[1] = config.wireless.into();
    report[2] = config.wired.into();
    Ok(report)
}

#[must_use]
pub const fn polling_with_wireless(current: PollingConfig, wireless: PollingRate) -> PollingConfig {
    PollingConfig {
        wireless,
        wired: current.wired,
    }
}

pub fn polling_with_wired(
    current: PollingConfig,
    wired: PollingRate,
) -> Result<PollingConfig, Error> {
    validate_wired_polling_rate(wired)?;
    Ok(PollingConfig {
        wireless: current.wireless,
        wired,
    })
}

/// Validates the Aerox 3 Wireless Gen 2's wired-mode polling limit.
pub fn validate_wired_polling_rate(rate: PollingRate) -> Result<(), Error> {
    if Aerox3WirelessGen2::CAPABILITIES
        .polling
        .wired
        .contains(&rate)
    {
        Ok(())
    } else {
        Err(Error::WiredPollingTooHigh)
    }
}

fn ensure_response_header(response: &[u8], command: u8, minimum: usize) -> Result<(), Error> {
    if response.len() < minimum {
        return Err(Error::MalformedResponse {
            command,
            expected: minimum,
            actual: response.len(),
        });
    }
    if response[0] != command {
        return Err(Error::UnexpectedCommand {
            expected: command,
            actual: response[0],
        });
    }
    Ok(())
}

fn read_u16_le(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn write_u16_le(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32_le(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protocol_report(command: u8) -> [u8; REPORT_SIZE] {
        let mut report = [0_u8; REPORT_SIZE];
        report[0] = command;
        report
    }

    fn match_report_sequence(
        reports: impl IntoIterator<Item = [u8; REPORT_SIZE]>,
        expected: u8,
    ) -> Result<[u8; REPORT_SIZE], Error> {
        let mut reports = reports.into_iter();
        wait_for_expected_response(expected, Duration::from_secs(1), |_| Ok(reports.next()))
    }

    fn scalar_config(values: &[u16], active: usize) -> DpiConfig {
        DpiConfig {
            stages: values.iter().copied().map(DpiStage::scalar).collect(),
            active,
        }
    }

    fn power_config() -> PowerConfig {
        PowerConfig {
            low_power_enabled: false,
            low_power_polling: LowPowerPollingRate::Hz125,
            sleep_timer: sleep_timer_from_minutes(5).unwrap(),
            auto_low_power_enabled: true,
            auto_low_power_threshold: auto_low_power_threshold(10).unwrap(),
            reserved: [0xa5; POWER_RESERVED_SIZE],
        }
    }

    fn power_response(minutes: u64) -> [u8; REPORT_SIZE] {
        let mut response = protocol_report(CMD_GET_POWER);
        response[1] = 0x00;
        response[2] = 0x05;
        write_u32_le(
            &mut response,
            3,
            sleep_timer_from_minutes(minutes).unwrap().milliseconds(),
        );
        response[7] = 0x01;
        response[8] = 10;
        response[POWER_RESERVED_OFFSET..].fill(0xa5);
        response
    }

    fn wireless_features_response(stability: bool, smoothing: bool) -> [u8; REPORT_SIZE] {
        let mut response = protocol_report(CMD_GET_WIRELESS_FEATURES);
        response[1] = u8::from(stability);
        response[2] = u8::from(smoothing);
        for (index, byte) in response[WIRELESS_FEATURES_RESERVED_OFFSET..]
            .iter_mut()
            .enumerate()
        {
            *byte = u8::try_from(index + 1).unwrap();
        }
        response
    }

    fn scroll_jump_response(enabled: bool, delay_ms: u16) -> [u8; REPORT_SIZE] {
        let mut response = protocol_report(CMD_GET_SCROLL_JUMP);
        response[1] = u8::from(enabled);
        write_u16_le(&mut response, 2, delay_ms);
        for (index, byte) in response[SCROLL_JUMP_RESERVED_OFFSET..]
            .iter_mut()
            .enumerate()
        {
            *byte = u8::try_from(index + 1).unwrap();
        }
        response
    }

    fn identity_protocol_response(value: &str) -> [u8; REPORT_SIZE] {
        let mut response = protocol_report(CMD_GET_DEVICE_IDENTITY);
        response[1..1 + value.len()].copy_from_slice(value.as_bytes());
        response
    }

    fn identity(value: &str) -> DeviceIdentity {
        DeviceIdentity::new(value.to_owned())
    }

    fn identified_endpoint<T>(
        kind: EndpointKind,
        identity_value: &str,
        endpoint: T,
    ) -> IdentifiedEndpoint<T> {
        IdentifiedEndpoint {
            identity: identity(identity_value),
            kind,
            endpoint,
        }
    }

    fn selectable_pair(identity_value: &str) -> EndpointPair<&'static str> {
        EndpointPair {
            identity: identity(identity_value),
            wired: Some("wired"),
            receiver: None,
        }
    }

    #[test]
    fn parses_dpi_response() {
        let mut response = [0_u8; REPORT_SIZE];
        response[..28].copy_from_slice(&[
            0xad, 5, 0, 0x78, 0x05, 0x78, 0x05, 0, 0xaa, 0x05, 0xaa, 0x05, 0, 0xdc, 0x05, 0xdc,
            0x05, 0, 0x0e, 0x06, 0x0e, 0x06, 0, 0x40, 0x06, 0x40, 0x06, 1,
        ]);
        let config = parse_dpi_response(&response).unwrap();
        assert_eq!(config.active, 0);
        assert_eq!(config.stages.len(), 5);
        assert_eq!(config.stages[0], DpiStage::scalar(1400));
        assert_eq!(config.stages[3], DpiStage::scalar(1550));
        assert_eq!(
            config.stages[4],
            DpiStage {
                x: 1600,
                y: 1600,
                lod: LiftOffDistance::High,
            }
        );
    }

    #[test]
    fn encodes_dpi_set_packet() {
        let config = DpiConfig {
            stages: vec![
                DpiStage::scalar(400),
                DpiStage {
                    x: 800,
                    y: 800,
                    lod: LiftOffDistance::High,
                },
                DpiStage::scalar(1600),
            ],
            active: 1,
        };
        let report = encode_dpi_config(&config).unwrap();
        assert_eq!(
            &report[..18],
            &[
                0x6d, 3, 1, 0x90, 1, 0x90, 1, 0, 0x20, 3, 0x20, 3, 1, 0x40, 6, 0x40, 6, 0
            ]
        );
        assert!(report[18..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn lift_off_distance_protocol_values_map_in_both_directions() {
        assert_eq!(
            LiftOffDistance::try_from(0x00).unwrap(),
            LiftOffDistance::Low
        );
        assert_eq!(
            LiftOffDistance::try_from(0x01).unwrap(),
            LiftOffDistance::High
        );
        assert_eq!(u8::from(LiftOffDistance::Low), 0x00);
        assert_eq!(u8::from(LiftOffDistance::High), 0x01);
    }

    #[test]
    fn rejects_unknown_lift_off_distance_value() {
        assert!(matches!(
            LiftOffDistance::try_from(0x02),
            Err(Error::UnknownLiftOffDistance(0x02))
        ));
    }

    #[test]
    fn rejects_more_than_five_dpi_stages() {
        let error = encode_dpi_config(&scalar_config(&[1, 2, 3, 4, 5, 6], 0)).unwrap_err();
        assert!(matches!(error, Error::InvalidDpiStageCount(6)));
    }

    #[test]
    fn rejects_invalid_dpi_active_index() {
        let error = encode_dpi_config(&scalar_config(&[400], 1)).unwrap_err();
        assert!(matches!(
            error,
            Error::InvalidDpiActiveIndex {
                active: 1,
                stage_count: 1
            }
        ));
    }

    #[test]
    fn validates_aerox_3_wireless_gen2_dpi_capabilities() {
        for value in [50, 400, 1450, 26_000] {
            validate_dpi_values(&[(value, value)]).unwrap();
        }

        for value in [49, 51, 1451, 26_050] {
            assert!(matches!(
                validate_dpi_values(&[(value, 800)]),
                Err(Error::InvalidDpiValue { axis: "X", value: invalid }) if invalid == value
            ));
        }
    }

    #[test]
    fn validates_asymmetric_dpi_axes_independently() {
        validate_dpi_values(&[(50, 26_000), (800, 1600)]).unwrap();
        assert!(matches!(
            validate_dpi_values(&[(49, 800)]),
            Err(Error::InvalidDpiValue {
                axis: "X",
                value: 49
            })
        ));
        assert!(matches!(
            validate_dpi_values(&[(800, 26_050)]),
            Err(Error::InvalidDpiValue {
                axis: "Y",
                value: 26_050
            })
        ));
    }

    #[test]
    fn parses_all_verified_wireless_feature_states() {
        for (stability, smoothing) in [(false, false), (true, false), (false, true), (true, true)] {
            let response = wireless_features_response(stability, smoothing);
            let features = parse_wireless_features_response(&response).unwrap();
            assert_eq!(features.wireless_stability_enabled, stability);
            assert_eq!(features.bluetooth_smoothing_enabled, smoothing);
            assert_eq!(
                features.reserved.as_slice(),
                &response[WIRELESS_FEATURES_RESERVED_OFFSET..]
            );
        }
    }

    #[test]
    fn wireless_stability_updates_only_byte_one() {
        for (initial, updated) in [(false, true), (true, false)] {
            let response = wireless_features_response(initial, true);
            let current = parse_wireless_features_response(&response).unwrap();
            let report =
                encode_wireless_features(&wireless_features_with_stability(current, updated));
            let mut expected = response;
            expected[0] = CMD_SET_WIRELESS_FEATURES;
            expected[1] = u8::from(updated);
            assert_eq!(report, expected);
        }
    }

    #[test]
    fn bluetooth_smoothing_updates_only_byte_two() {
        for (initial, updated) in [(false, true), (true, false)] {
            let response = wireless_features_response(true, initial);
            let current = parse_wireless_features_response(&response).unwrap();
            let report = encode_wireless_features(&wireless_features_with_bluetooth_smoothing(
                current, updated,
            ));
            let mut expected = response;
            expected[0] = CMD_SET_WIRELESS_FEATURES;
            expected[2] = u8::from(updated);
            assert_eq!(report, expected);
        }
    }

    #[test]
    fn wireless_features_get_skips_stale_reports() {
        let expected = wireless_features_response(true, true);
        let matched = match_report_sequence(
            [
                protocol_report(CMD_COMMIT),
                protocol_report(CMD_SET_POWER),
                expected,
            ],
            CMD_GET_WIRELESS_FEATURES,
        )
        .unwrap();
        assert_eq!(matched, expected);
    }

    #[test]
    fn wireless_feature_get_and_set_reports_use_only_their_own_commands() {
        let get = command_report(CMD_GET_WIRELESS_FEATURES);
        assert_eq!(get[0], 0x95);
        assert!(get[1..].iter().all(|byte| *byte == 0));

        let current =
            parse_wireless_features_response(&wireless_features_response(false, false)).unwrap();
        let set = encode_wireless_features(&wireless_features_with_stability(current, true));
        assert_eq!(set[0], 0x55);
        assert_ne!(set[0], CMD_COMMIT);
    }

    #[test]
    fn rejects_invalid_wireless_feature_states() {
        let mut response = wireless_features_response(false, false);
        response[1] = 0x02;
        assert!(matches!(
            parse_wireless_features_response(&response),
            Err(Error::InvalidWirelessFeatureBoolean {
                field: "Wireless Stability Enhancement",
                value: 0x02
            })
        ));

        let mut response = wireless_features_response(false, false);
        response[2] = 0x02;
        assert!(matches!(
            parse_wireless_features_response(&response),
            Err(Error::InvalidWirelessFeatureBoolean {
                field: "Bluetooth Smoothing",
                value: 0x02
            })
        ));
    }

    #[test]
    fn parses_verified_scroll_jump_responses() {
        for (enabled, delay_ms) in [(false, 500), (true, 100), (true, 1_500)] {
            let response = scroll_jump_response(enabled, delay_ms);
            let config = parse_scroll_jump_response(&response).unwrap();
            assert_eq!(config.enabled, enabled);
            assert_eq!(config.delay_ms, delay_ms);
            assert_eq!(
                config.reserved.as_slice(),
                &response[SCROLL_JUMP_RESERVED_OFFSET..]
            );
        }
    }

    #[test]
    fn scroll_jump_enabled_updates_only_byte_one() {
        for (initial, updated) in [(false, true), (true, false)] {
            let response = scroll_jump_response(initial, 500);
            let current = parse_scroll_jump_response(&response).unwrap();
            let report = encode_scroll_jump_config(&scroll_jump_with_enabled(current, updated));
            let mut expected = response;
            expected[0] = CMD_SET_SCROLL_JUMP;
            expected[1] = u8::from(updated);
            assert_eq!(report, expected);
        }
    }

    #[test]
    fn scroll_jump_delay_updates_only_delay_bytes() {
        for enabled in [false, true] {
            let response = scroll_jump_response(enabled, 1_500);
            let current = parse_scroll_jump_response(&response).unwrap();
            let report = encode_scroll_jump_config(&scroll_jump_with_delay(current, 500).unwrap());
            let mut expected = response;
            expected[0] = CMD_SET_SCROLL_JUMP;
            expected[2..4].copy_from_slice(&500_u16.to_le_bytes());
            assert_eq!(report, expected);
        }
    }

    #[test]
    fn serializes_verified_scroll_jump_delays_as_little_endian() {
        for (delay_ms, bytes) in [
            (100, [0x64, 0x00]),
            (500, [0xf4, 0x01]),
            (1_500, [0xdc, 0x05]),
        ] {
            let current =
                parse_scroll_jump_response(&scroll_jump_response(true, delay_ms)).unwrap();
            let report = encode_scroll_jump_config(&current);
            assert_eq!(&report[2..4], &bytes);
        }
    }

    #[test]
    fn validates_gg_compatible_scroll_jump_delays() {
        for delay_ms in [100, 200, 500, 1_500] {
            validate_scroll_jump_delay(delay_ms).unwrap();
        }
        for delay_ms in [99, 101, 250, 1_501] {
            assert!(matches!(
                validate_scroll_jump_delay(delay_ms),
                Err(Error::InvalidScrollJumpDelay(value)) if value == delay_ms
            ));
        }
    }

    #[test]
    fn scroll_jump_get_skips_stale_reports() {
        let expected = scroll_jump_response(true, 500);
        let matched = match_report_sequence(
            [
                protocol_report(CMD_COMMIT),
                protocol_report(CMD_SET_WIRELESS_FEATURES),
                expected,
            ],
            CMD_GET_SCROLL_JUMP,
        )
        .unwrap();
        assert_eq!(matched, expected);
    }

    #[test]
    fn scroll_jump_get_and_set_reports_use_only_their_own_commands() {
        let get = command_report(CMD_GET_SCROLL_JUMP);
        assert_eq!(get[0], 0x96);
        assert!(get[1..].iter().all(|byte| *byte == 0));

        let current = parse_scroll_jump_response(&scroll_jump_response(false, 500)).unwrap();
        let set = encode_scroll_jump_config(&scroll_jump_with_enabled(current, true));
        assert_eq!(set[0], 0x56);
        assert_ne!(set[0], CMD_COMMIT);
    }

    #[test]
    fn rejects_invalid_scroll_jump_enabled_state() {
        let mut response = scroll_jump_response(false, 500);
        response[1] = 0x02;
        assert!(matches!(
            parse_scroll_jump_response(&response),
            Err(Error::InvalidScrollJumpEnabled(0x02))
        ));
    }

    #[test]
    fn parses_known_power_response_and_preserves_reserved_bytes() {
        let config = parse_power_response(&power_response(5)).unwrap();
        assert!(!config.low_power_enabled);
        assert_eq!(config.low_power_polling, LowPowerPollingRate::Hz125);
        assert_eq!(config.sleep_timer.whole_minutes(), Some(5));
        assert!(config.auto_low_power_enabled);
        assert_eq!(config.auto_low_power_threshold.percent(), 10);
        assert_eq!(config.reserved, [0xa5; POWER_RESERVED_SIZE]);

        let config = parse_power_response(&power_response(30)).unwrap();
        assert_eq!(config.sleep_timer.whole_minutes(), Some(30));
    }

    #[test]
    fn rejects_invalid_power_response_fields() {
        let mut response = power_response(5);
        response[0] = 0xa7;
        assert!(matches!(
            parse_power_response(&response),
            Err(Error::UnexpectedCommand {
                expected: CMD_GET_POWER,
                actual: 0xa7
            })
        ));

        let mut response = power_response(5);
        response[1] = 0x02;
        assert!(matches!(
            parse_power_response(&response),
            Err(Error::InvalidPowerBoolean {
                field: "Low Power Mode",
                value: 0x02
            })
        ));

        let mut response = power_response(5);
        response[8] = 4;
        assert!(matches!(
            parse_power_response(&response),
            Err(Error::InvalidAutoLowPowerThreshold(4))
        ));
    }

    #[test]
    fn low_power_polling_codes_map_in_both_directions() {
        for (code, rate) in [
            (0x05, LowPowerPollingRate::Hz125),
            (0x04, LowPowerPollingRate::Hz250),
            (0x03, LowPowerPollingRate::Hz500),
        ] {
            assert_eq!(LowPowerPollingRate::try_from(code).unwrap(), rate);
            assert_eq!(u8::from(rate), code);
            assert_eq!(LowPowerPollingRate::try_from(rate.hz()).unwrap(), rate);
        }
        for rate in [0, 1000, 2000, 4000] {
            assert!(matches!(
                LowPowerPollingRate::try_from(rate),
                Err(Error::UnsupportedLowPowerPollingRate(value)) if value == rate
            ));
        }
    }

    #[test]
    fn serializes_known_sleep_timer_values() {
        for (minutes, expected) in [
            (5, [0xe0, 0x93, 0x04, 0x00]),
            (10, [0xc0, 0x27, 0x09, 0x00]),
            (20, [0x80, 0x4f, 0x12, 0x00]),
            (30, [0x40, 0x77, 0x1b, 0x00]),
        ] {
            let config =
                power_with_sleep_timer(power_config(), sleep_timer_from_minutes(minutes).unwrap());
            let report = encode_power_config(&config).unwrap();
            assert_eq!(report[0], CMD_SET_POWER);
            assert_eq!(report[1], 0x00);
            assert_eq!(report[2], 0x05);
            assert_eq!(report[3..7], expected);
            assert_eq!(report[7], 0x01);
            assert_eq!(report[8], 10);
            assert_eq!(&report[POWER_RESERVED_OFFSET..], &config.reserved);
        }
    }

    #[test]
    fn validates_auto_low_power_threshold_range() {
        assert!(matches!(
            auto_low_power_threshold(4),
            Err(Error::InvalidAutoLowPowerThreshold(4))
        ));
        assert_eq!(auto_low_power_threshold(5).unwrap().percent(), 5);
        assert_eq!(auto_low_power_threshold(25).unwrap().percent(), 25);
        assert!(matches!(
            auto_low_power_threshold(26),
            Err(Error::InvalidAutoLowPowerThreshold(26))
        ));
    }

    #[test]
    fn validates_sleep_timer_encoding_range() {
        assert!(matches!(
            sleep_timer_from_minutes(0),
            Err(Error::SleepTimerZero)
        ));
        for minutes in [1, 20, 30, 1440, 71_582] {
            assert_eq!(
                sleep_timer_from_minutes(minutes).unwrap().whole_minutes(),
                Some(u32::try_from(minutes).unwrap())
            );
        }
        assert!(matches!(
            sleep_timer_from_minutes(71_583),
            Err(Error::SleepTimerTooLarge)
        ));
    }

    #[test]
    fn power_updates_preserve_unmodified_fields() {
        let current = power_config();

        let sleep = power_with_sleep_timer(current.clone(), sleep_timer_from_minutes(30).unwrap());
        assert_eq!(sleep.sleep_timer.whole_minutes(), Some(30));
        assert_eq!(
            PowerConfig {
                sleep_timer: current.sleep_timer,
                ..sleep.clone()
            },
            current
        );

        let threshold = power_with_auto_low_power_threshold(
            current.clone(),
            auto_low_power_threshold(25).unwrap(),
        );
        assert_eq!(threshold.auto_low_power_threshold.percent(), 25);
        assert_eq!(
            PowerConfig {
                auto_low_power_threshold: current.auto_low_power_threshold,
                ..threshold.clone()
            },
            current
        );

        let low_power = power_with_low_power_enabled(current.clone(), true);
        assert!(low_power.low_power_enabled);
        assert_eq!(
            PowerConfig {
                low_power_enabled: current.low_power_enabled,
                ..low_power
            },
            current
        );
    }

    #[test]
    fn polling_codes_map_in_both_directions() {
        let cases = [
            (0x00, PollingRate::Hz4000),
            (0x01, PollingRate::Hz2000),
            (0x02, PollingRate::Hz1000),
            (0x03, PollingRate::Hz500),
            (0x04, PollingRate::Hz250),
            (0x05, PollingRate::Hz125),
        ];
        for (code, rate) in cases {
            assert_eq!(PollingRate::try_from(code).unwrap(), rate);
            assert_eq!(u8::from(rate), code);
        }
    }

    #[test]
    fn rejects_unknown_polling_code() {
        assert!(matches!(
            PollingRate::try_from(0x06_u8),
            Err(Error::UnknownPollingCode(0x06))
        ));
    }

    #[test]
    fn rejects_wired_2000_and_4000_hz() {
        let current = PollingConfig {
            wireless: PollingRate::Hz1000,
            wired: PollingRate::Hz1000,
        };
        assert!(matches!(
            polling_with_wired(current, PollingRate::Hz2000),
            Err(Error::WiredPollingTooHigh)
        ));
        assert!(matches!(
            polling_with_wired(current, PollingRate::Hz4000),
            Err(Error::WiredPollingTooHigh)
        ));
    }

    #[test]
    fn parses_polling_response() {
        assert_eq!(
            parse_polling_response(&[0xab, 0x02, 0x02]).unwrap(),
            PollingConfig {
                wireless: PollingRate::Hz1000,
                wired: PollingRate::Hz1000,
            }
        );
    }

    #[test]
    fn parses_available_battery_not_charging() {
        assert_eq!(
            parse_battery_response(&[0x92, 0x00, 0x14]).unwrap(),
            BatteryStatus::Available {
                percent: 20,
                charging: false,
            }
        );
    }

    #[test]
    fn parses_available_battery_charging() {
        assert_eq!(
            parse_battery_response(&[0x92, 0x01, 0x15]).unwrap(),
            BatteryStatus::Available {
                percent: 21,
                charging: true,
            }
        );
    }

    #[test]
    fn parses_full_battery_not_charging() {
        assert_eq!(
            parse_battery_response(&[0x92, 0x00, 0x64]).unwrap(),
            BatteryStatus::Available {
                percent: 100,
                charging: false,
            }
        );
    }

    #[test]
    fn parses_unavailable_battery_marker() {
        assert_eq!(
            parse_battery_response(&[0x92, 0x00, 0xff]).unwrap(),
            BatteryStatus::Unavailable
        );
    }

    #[test]
    fn rejects_unexpected_battery_response_command() {
        assert!(matches!(
            parse_battery_response(&[0x91, 0x00, 0x14]),
            Err(Error::UnexpectedCommand {
                expected: CMD_GET_BATTERY,
                actual: 0x91,
            })
        ));
    }

    #[test]
    fn rejects_invalid_battery_charging_state() {
        assert!(matches!(
            parse_battery_response(&[0x92, 0x02, 0x14]),
            Err(Error::InvalidBatteryChargingState(0x02))
        ));
    }

    #[test]
    fn rejects_battery_percentage_above_100() {
        assert!(matches!(
            parse_battery_response(&[0x92, 0x00, 0x65]),
            Err(Error::InvalidBatteryPercentage(101))
        ));
    }

    #[test]
    fn encodes_polling_packet() {
        let report = encode_polling_config(PollingConfig {
            wireless: PollingRate::Hz4000,
            wired: PollingRate::Hz1000,
        })
        .unwrap();
        assert_eq!(&report[..3], &[0x6b, 0x00, 0x02]);
        assert!(report[3..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn dpi_use_selects_by_stage_id_and_preserves_existing_stages() {
        let current = DpiConfig {
            stages: vec![
                DpiStage {
                    x: 400,
                    y: 800,
                    lod: LiftOffDistance::High,
                },
                DpiStage::scalar(800),
                DpiStage::scalar(1600),
            ],
            active: 2,
        };
        let updated = config_with_active_stage(&current, 2).unwrap();
        assert_eq!(updated.stages, current.stages);
        assert_eq!(updated.active, 1);
        assert!(matches!(
            config_with_active_stage(&current, 800),
            Err(Error::InvalidStageId(800))
        ));
    }

    #[test]
    fn stage_ids_map_to_zero_based_firmware_indexes() {
        assert_eq!(stage_id_to_index(1).unwrap(), 0);
        assert_eq!(stage_id_to_index(5).unwrap(), 4);
    }

    #[test]
    fn rejects_nonexistent_stage() {
        let current = scalar_config(&[400, 800, 1600], 0);
        assert!(matches!(
            config_with_active_stage(&current, 4),
            Err(Error::StageNotConfigured {
                stage_id: 4,
                stage_count: 3,
            })
        ));
        assert!(matches!(
            config_with_lift_off_distance(&current, 4, LiftOffDistance::High),
            Err(Error::StageNotConfigured {
                stage_id: 4,
                stage_count: 3,
            })
        ));
    }

    #[test]
    fn changing_lod_preserves_every_other_stage_field() {
        let current = DpiConfig {
            stages: vec![
                DpiStage::scalar(400),
                DpiStage {
                    x: 800,
                    y: 900,
                    lod: LiftOffDistance::Low,
                },
                DpiStage {
                    x: 1600,
                    y: 1700,
                    lod: LiftOffDistance::High,
                },
            ],
            active: 2,
        };
        let updated = config_with_lift_off_distance(&current, 2, LiftOffDistance::High).unwrap();

        assert_eq!(updated.active, current.active);
        assert_eq!(updated.stages[0], current.stages[0]);
        assert_eq!(updated.stages[1].x, current.stages[1].x);
        assert_eq!(updated.stages[1].y, current.stages[1].y);
        assert_eq!(updated.stages[1].lod, LiftOffDistance::High);
        assert_eq!(updated.stages[2], current.stages[2]);
    }

    #[test]
    fn polling_updates_preserve_untouched_mode() {
        let current = PollingConfig {
            wireless: PollingRate::Hz1000,
            wired: PollingRate::Hz500,
        };
        let wireless = polling_with_wireless(current, PollingRate::Hz4000);
        assert_eq!(wireless.wired, PollingRate::Hz500);
        let wired = polling_with_wired(current, PollingRate::Hz125).unwrap();
        assert_eq!(wired.wireless, PollingRate::Hz1000);
    }

    #[test]
    fn replacing_dpi_stages_preserves_matching_scalar_active_value() {
        let current = scalar_config(&[400, 800, 1600], 1);
        let updated =
            config_from_dpi_values(&current, &[(400, 400), (1600, 1600), (800, 800)]).unwrap();
        assert_eq!(updated.active, 2);
    }

    #[test]
    fn replacing_dpi_stages_preserves_lod_by_position() {
        let mut current = scalar_config(&[400, 800, 1600], 0);
        current.stages[0].lod = LiftOffDistance::High;
        current.stages[2].lod = LiftOffDistance::High;

        let updated =
            config_from_dpi_values(&current, &[(500, 600), (900, 1000), (1700, 1800)]).unwrap();
        assert_eq!((updated.stages[0].x, updated.stages[0].y), (500, 600));
        assert_eq!((updated.stages[1].x, updated.stages[1].y), (900, 1000));
        assert_eq!((updated.stages[2].x, updated.stages[2].y), (1700, 1800));
        assert_eq!(updated.stages[0].lod, LiftOffDistance::High);
        assert_eq!(updated.stages[1].lod, LiftOffDistance::Low);
        assert_eq!(updated.stages[2].lod, LiftOffDistance::High);
    }

    #[test]
    fn newly_created_dpi_stage_defaults_to_low_lod() {
        let current = scalar_config(&[400], 0);
        let updated = config_from_dpi_values(&current, &[(400, 400), (800, 1600)]).unwrap();
        assert_eq!(updated.stages[1].lod, LiftOffDistance::Low);
    }

    #[test]
    fn response_matching_skips_stale_commit_acknowledgement() {
        let mut polling = protocol_report(CMD_GET_POLLING);
        polling[1] = 0x02;
        polling[2] = 0x02;

        let matched =
            match_report_sequence([protocol_report(CMD_COMMIT), polling], CMD_GET_POLLING).unwrap();

        assert_eq!(matched, polling);
    }

    #[test]
    fn response_matching_skips_multiple_unrelated_acknowledgements() {
        let dpi = protocol_report(CMD_GET_DPI);
        let matched = match_report_sequence(
            [
                protocol_report(CMD_SET_POLLING),
                protocol_report(CMD_COMMIT),
                dpi,
            ],
            CMD_GET_DPI,
        )
        .unwrap();

        assert_eq!(matched, dpi);
    }

    #[test]
    fn identity_response_matching_skips_stale_unrelated_report() {
        let identity_response = protocol_report(CMD_GET_DEVICE_IDENTITY);
        let matched = match_report_sequence(
            [protocol_report(0x40), identity_response],
            CMD_GET_DEVICE_IDENTITY,
        )
        .unwrap();

        assert_eq!(matched, identity_response);
    }

    #[test]
    fn identity_query_succeeds_on_first_request_without_retry() {
        let writes = std::cell::Cell::new(0);
        let response = identity_protocol_response("6271700431492500250");
        let mut reads = std::collections::VecDeque::from([Some(response)]);

        let matched = send_identity_query_with_retry(
            |report| {
                assert_eq!(report[0], CMD_GET_DEVICE_IDENTITY);
                writes.set(writes.get() + 1);
                Ok(())
            },
            |_| Ok(reads.pop_front().flatten()),
        )
        .unwrap();

        assert_eq!(matched, response);
        assert_eq!(writes.get(), 1);
    }

    #[test]
    fn identity_query_resends_after_attempt_timeout() {
        let writes = std::cell::Cell::new(0);
        let response = identity_protocol_response("6271700431492500250");
        let mut reads = std::collections::VecDeque::from([None, Some(response)]);

        let matched = send_identity_query_with_retry(
            |_| {
                writes.set(writes.get() + 1);
                Ok(())
            },
            |_| Ok(reads.pop_front().flatten()),
        )
        .unwrap();

        assert_eq!(matched, response);
        assert_eq!(writes.get(), 2);
    }

    #[test]
    fn identity_retry_keeps_filtering_stale_reports_across_attempts() {
        let writes = std::cell::Cell::new(0);
        let response = identity_protocol_response("6271700431492500250");
        let mut reads = std::collections::VecDeque::from([
            Some(protocol_report(CMD_COMMIT)),
            Some(protocol_report(0x40)),
            None,
            Some(protocol_report(CMD_SET_POLLING)),
            Some(response),
        ]);

        let matched = send_identity_query_with_retry(
            |_| {
                writes.set(writes.get() + 1);
                Ok(())
            },
            |_| Ok(reads.pop_front().flatten()),
        )
        .unwrap();

        assert_eq!(matched, response);
        assert_eq!(writes.get(), 2);
    }

    #[test]
    fn identity_retry_timeout_is_bounded_and_preserves_diagnostics() {
        let writes = std::cell::Cell::new(0);
        let mut reads = std::collections::VecDeque::from([
            Some(protocol_report(CMD_COMMIT)),
            None,
            Some(protocol_report(0x40)),
            None,
            None,
            None,
            None,
        ]);

        let error = send_identity_query_with_retry(
            |_| {
                writes.set(writes.get() + 1);
                Ok(())
            },
            |_| Ok(reads.pop_front().flatten()),
        )
        .unwrap_err();

        assert_eq!(writes.get(), IDENTITY_MAX_ATTEMPTS);
        assert_eq!(
            error.to_string(),
            "timed out waiting for response 0xf0 (observed: 0x11, 0x40)"
        );
    }

    #[test]
    fn identity_retry_does_not_retry_non_timeout_errors() {
        let writes = std::cell::Cell::new(0);
        let error = send_identity_query_with_retry(
            |_| {
                writes.set(writes.get() + 1);
                Ok(())
            },
            |_| {
                Err(Error::MalformedResponse {
                    command: CMD_GET_DEVICE_IDENTITY,
                    expected: REPORT_SIZE,
                    actual: 8,
                })
            },
        )
        .unwrap_err();

        assert_eq!(writes.get(), 1);
        assert!(matches!(error, Error::MalformedResponse { actual: 8, .. }));

        let writes = std::cell::Cell::new(0);
        let reads = std::cell::Cell::new(0);
        let error = send_identity_query_with_retry(
            |_| {
                writes.set(writes.get() + 1);
                Err(Error::ShortWrite {
                    command: CMD_GET_DEVICE_IDENTITY,
                    expected: WRITE_SIZE,
                    actual: 0,
                })
            },
            |_| {
                reads.set(reads.get() + 1);
                Ok(None)
            },
        )
        .unwrap_err();

        assert_eq!(writes.get(), 1);
        assert_eq!(reads.get(), 0);
        assert!(matches!(error, Error::ShortWrite { actual: 0, .. }));
    }

    #[test]
    fn malformed_matching_identity_response_is_not_retried() {
        let writes = std::cell::Cell::new(0);
        let mut response = protocol_report(CMD_GET_DEVICE_IDENTITY);
        response[1] = 0x00;
        let returned = send_identity_query_with_retry(
            |_| {
                writes.set(writes.get() + 1);
                Ok(())
            },
            |_| Ok(Some(response)),
        )
        .unwrap();

        assert!(matches!(
            parse_device_identity_response(&returned),
            Err(Error::EmptyDeviceIdentity)
        ));
        assert_eq!(writes.get(), 1);
    }

    #[test]
    fn response_matching_timeout_reports_observed_commands() {
        let error = match_report_sequence(
            [
                protocol_report(CMD_SET_POLLING),
                protocol_report(CMD_COMMIT),
            ],
            CMD_GET_DPI,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            Error::ReadTimeout {
                expected: CMD_GET_DPI,
                ..
            }
        ));
        assert_eq!(
            error.to_string(),
            "timed out waiting for response 0xad (observed: 0x6b, 0x11)"
        );
    }

    #[test]
    fn groups_matching_wired_and_linked_receiver_as_one_wired_device() {
        let pairs = group_identified_endpoints([
            identified_endpoint(EndpointKind::Wired, "AAA", "wired"),
            identified_endpoint(EndpointKind::Receiver2_4Ghz, "AAA", "receiver"),
        ]);

        assert_eq!(pairs.len(), 1);
        let device = pairs[0].physical_device();
        assert_eq!(device.identity, identity("AAA"));
        assert_eq!(device.active_connection, ConnectionType::Wired);
        assert!(device.wired_endpoint_available);
        assert!(device.receiver_endpoint_available);
    }

    #[test]
    fn selects_only_device_without_requested_identity() {
        let selected = select_endpoint_pair(vec![selectable_pair("AAA")], None).unwrap();
        assert_eq!(selected.identity, identity("AAA"));
    }

    #[test]
    fn rejects_ambiguous_devices_without_requested_identity() {
        let result =
            select_endpoint_pair(vec![selectable_pair("BBB"), selectable_pair("AAA")], None);

        assert!(matches!(&result, Err(Error::MultipleDevices { .. })));
        let error = result.err().unwrap().to_string();
        assert!(error.contains("  AAA\n  BBB"));
        assert!(error.contains("Specify one with --device <ID>."));
    }

    #[test]
    fn selects_explicit_first_device_from_multiple_devices() {
        let selected = select_endpoint_pair(
            vec![selectable_pair("AAA"), selectable_pair("BBB")],
            Some("AAA"),
        )
        .unwrap();

        assert_eq!(selected.identity, identity("AAA"));
    }

    #[test]
    fn selects_explicit_second_device_from_multiple_devices() {
        let selected = select_endpoint_pair(
            vec![selectable_pair("AAA"), selectable_pair("BBB")],
            Some("BBB"),
        )
        .unwrap();

        assert_eq!(selected.identity, identity("BBB"));
    }

    #[test]
    fn rejects_requested_identity_that_is_not_present() {
        let result = select_endpoint_pair(
            vec![selectable_pair("AAA"), selectable_pair("BBB")],
            Some("CCC"),
        );

        assert!(matches!(
            result,
            Err(Error::RequestedDeviceNotFound { ref requested, .. }) if requested == "CCC"
        ));
    }

    #[test]
    fn rejects_empty_device_list_without_requested_identity() {
        let pairs: Vec<EndpointPair<()>> = Vec::new();
        assert!(matches!(
            select_endpoint_pair(pairs, None),
            Err(Error::DeviceNotConnected)
        ));
    }

    #[test]
    fn reports_requested_identity_not_found_when_no_devices_are_usable() {
        let pairs: Vec<EndpointPair<()>> = Vec::new();
        assert!(matches!(
            select_endpoint_pair(pairs, Some("AAA")),
            Err(Error::RequestedDeviceNotFound { ref requested, .. }) if requested == "AAA"
        ));
    }

    #[test]
    fn matching_endpoint_pair_does_not_create_selection_ambiguity() {
        let pairs = group_identified_endpoints([
            identified_endpoint(EndpointKind::Wired, "AAA", "wired"),
            identified_endpoint(EndpointKind::Receiver2_4Ghz, "AAA", "receiver"),
        ]);

        assert!(select_endpoint_pair(pairs, None).is_ok());
    }

    #[test]
    fn unsupported_usb_device_does_not_count_as_supported_endpoint() {
        let endpoints = [
            (VENDOR_ID, PID_WIRED),
            (VENDOR_ID, 0x1824),
            (0x1234, PID_RECEIVER),
        ];
        let supported_count = endpoints
            .into_iter()
            .filter_map(|(vendor, product)| supported_endpoint_kind(vendor, product))
            .count();

        assert_eq!(supported_count, 1);
    }

    #[test]
    fn identity_selection_requires_exact_match() {
        let result = select_endpoint_pair(vec![selectable_pair("AAA123")], Some("AAA"));

        assert!(matches!(result, Err(Error::RequestedDeviceNotFound { .. })));
    }

    #[test]
    fn selects_linked_receiver_when_no_wired_endpoint_exists() {
        let pairs = group_identified_endpoints([identified_endpoint(
            EndpointKind::Receiver2_4Ghz,
            "AAA",
            "receiver",
        )]);

        assert_eq!(pairs.len(), 1);
        assert_eq!(
            pairs[0].physical_device().active_connection,
            ConnectionType::Wireless2_4Ghz
        );
    }

    #[test]
    fn inactive_receiver_produces_no_usable_mouse_endpoint() {
        assert!(!parse_receiver_link_response(&[0xbc, 0x00]).unwrap());
        let endpoints: Vec<IdentifiedEndpoint<()>> = Vec::new();
        assert!(group_identified_endpoints(endpoints).is_empty());
    }

    #[test]
    fn inactive_receiver_does_not_prevent_wired_device_use() {
        assert!(!parse_receiver_link_response(&[0xbc, 0x00]).unwrap());
        let pairs =
            group_identified_endpoints([identified_endpoint(EndpointKind::Wired, "AAA", "wired")]);

        assert_eq!(pairs.len(), 1);
        assert_eq!(
            pairs[0].physical_device().active_connection,
            ConnectionType::Wired
        );
    }

    #[test]
    fn different_identities_produce_two_physical_devices() {
        let pairs = group_identified_endpoints([
            identified_endpoint(EndpointKind::Wired, "AAA", "wired"),
            identified_endpoint(EndpointKind::Receiver2_4Ghz, "BBB", "receiver"),
        ]);

        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn matching_identity_is_grouped_exactly_once_regardless_of_endpoint_order() {
        let pairs = group_identified_endpoints([
            identified_endpoint(EndpointKind::Receiver2_4Ghz, "AAA", "receiver"),
            identified_endpoint(EndpointKind::Wired, "AAA", "wired"),
        ]);

        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].wired.is_some());
        assert!(pairs[0].receiver.is_some());
    }

    #[test]
    fn parses_verified_device_identity() {
        let expected = "6271700431492500250";
        let mut response = protocol_report(CMD_GET_DEVICE_IDENTITY);
        response[1..1 + expected.len()].copy_from_slice(expected.as_bytes());

        assert_eq!(
            parse_device_identity_response(&response).unwrap(),
            identity(expected)
        );
    }

    #[test]
    fn rejects_unexpected_device_identity_response_command() {
        assert!(matches!(
            parse_device_identity_response(&[0x40, b'A', 0x00]),
            Err(Error::UnexpectedCommand {
                expected: CMD_GET_DEVICE_IDENTITY,
                actual: 0x40,
            })
        ));
    }

    #[test]
    fn rejects_empty_device_identity() {
        assert!(matches!(
            parse_device_identity_response(&[0xf0, 0x00]),
            Err(Error::EmptyDeviceIdentity)
        ));
    }

    #[test]
    fn rejects_unterminated_device_identity() {
        assert!(matches!(
            parse_device_identity_response(&[0xf0, b'A']),
            Err(Error::UnterminatedDeviceIdentity)
        ));
    }

    #[test]
    fn rejects_non_ascii_device_identity() {
        assert!(matches!(
            parse_device_identity_response(&[0xf0, 0xff, 0x00]),
            Err(Error::InvalidDeviceIdentity)
        ));
    }

    #[test]
    fn parses_receiver_link_states() {
        assert!(!parse_receiver_link_response(&[0xbc, 0x00]).unwrap());
        assert!(parse_receiver_link_response(&[0xbc, 0x01]).unwrap());
    }

    #[test]
    fn rejects_invalid_receiver_link_state() {
        assert!(matches!(
            parse_receiver_link_response(&[0xbc, 0x02]),
            Err(Error::InvalidReceiverLinkState(0x02))
        ));
    }

    #[test]
    fn endpoint_selection_always_prefers_wired_for_matching_identity() {
        let mut pairs = group_identified_endpoints([
            identified_endpoint(EndpointKind::Receiver2_4Ghz, "AAA", "receiver"),
            identified_endpoint(EndpointKind::Wired, "AAA", "wired"),
        ]);

        assert_eq!(pairs.pop().unwrap().into_selected_endpoint(), "wired");
    }

    #[test]
    fn recognizes_io_permission_denied_open_error() {
        let error = hidapi::HidError::IoError {
            error: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        };

        assert!(hid_error_is_permission_denied(&error));
        assert!(matches!(
            open_error(error),
            Error::OpenPermissionDenied { .. }
        ));
    }

    #[test]
    fn recognizes_hidapi_permission_denied_open_error() {
        let error = hidapi::HidError::HidApiError {
            message: "Failed to open /dev/hidraw7: Permission denied".to_owned(),
        };

        assert!(hid_error_is_permission_denied(&error));
        let error = open_error(error);
        assert!(matches!(error, Error::OpenPermissionDenied { .. }));
        assert!(
            error
                .to_string()
                .contains("SteelSeries Linux udev permissions may not be installed or active")
        );
    }

    #[test]
    fn does_not_treat_unrelated_hid_error_as_permission_denied() {
        let error = hidapi::HidError::HidApiError {
            message: "device was disconnected".to_owned(),
        };

        assert!(!hid_error_is_permission_denied(&error));
        assert!(matches!(open_error(error), Error::Open(_)));
    }
}
