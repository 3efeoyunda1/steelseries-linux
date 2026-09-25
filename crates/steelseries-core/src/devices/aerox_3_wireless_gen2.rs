use std::{
    convert::TryFrom,
    ffi::CString,
    time::{Duration, Instant},
};

use hidapi::{HidApi, HidDevice};
use thiserror::Error;

use crate::device::{
    BatteryStatus, ConnectionType, DeviceIdentity, DpiConfig, DpiStage, PhysicalDevice,
    PollingConfig, PollingRate,
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
pub const CMD_GET_BATTERY: u8 = 0x92;
pub const CMD_GET_DEVICE_IDENTITY: u8 = 0xf0;
pub const CMD_GET_RECEIVER_LINK: u8 = 0xbc;

const REPORT_SIZE: usize = 64;
const WRITE_SIZE: usize = REPORT_SIZE + 1;
const RESPONSE_TIMEOUT: Duration = Duration::from_millis(1_500);
const MAX_DPI_STAGES: usize = 5;

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
    #[error("active DPI stage index {active} is outside the {stage_count} configured stages")]
    InvalidDpiActiveIndex { active: usize, stage_count: usize },
    #[error("{0} DPI is not configured as a DPI stage")]
    DpiNotConfigured(u16),
    #[error("unknown polling-rate protocol code 0x{0:02x}")]
    UnknownPollingCode(u8),
    #[error("unsupported polling rate {0} Hz; expected 125, 250, 500, 1000, 2000, or 4000 Hz")]
    UnsupportedPollingRate(u16),
    #[error("wired polling rate supports a maximum of 1000 Hz")]
    WiredPollingTooHigh,
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

pub struct Aerox3WirelessGen2 {
    device: HidDevice,
}

impl Aerox3WirelessGen2 {
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
        let response = self.send_command_and_expect(
            command_report(CMD_GET_DEVICE_IDENTITY),
            CMD_GET_DEVICE_IDENTITY,
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

    /// Selects an existing scalar DPI stage. Call [`Self::commit`] to persist it.
    pub fn set_active_dpi(&self, dpi: u16) -> Result<(), Error> {
        let current = self.get_dpi_config()?;
        let updated = config_with_active_dpi(&current, dpi)?;
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
    mut read_report: F,
) -> Result<[u8; REPORT_SIZE], Error>
where
    F: FnMut(Duration) -> Result<Option<[u8; REPORT_SIZE]>, Error>,
{
    let deadline = Instant::now() + timeout;
    let mut observed_commands = Vec::new();

    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err(response_timeout_error(expected_command, &observed_commands));
        };
        if remaining.is_zero() {
            return Err(response_timeout_error(expected_command, &observed_commands));
        }

        let Some(report) = read_report(remaining)? else {
            return Err(response_timeout_error(expected_command, &observed_commands));
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
    if !(1..=MAX_DPI_STAGES).contains(&config.stages.len()) {
        return Err(Error::InvalidDpiStageCount(config.stages.len()));
    }
    if config.active >= config.stages.len() {
        return Err(Error::InvalidDpiActiveIndex {
            active: config.active,
            stage_count: config.stages.len(),
        });
    }
    Ok(())
}

fn parse_dpi_response(response: &[u8]) -> Result<DpiConfig, Error> {
    ensure_response_header(response, CMD_GET_DPI, 3)?;
    let stage_count = usize::from(response[1]);
    if !(1..=MAX_DPI_STAGES).contains(&stage_count) {
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
    }
    Ok(report)
}

/// Builds a scalar-stage configuration and preserves the active scalar DPI when possible.
pub fn config_from_scalar_dpis(current: &DpiConfig, dpis: &[u16]) -> Result<DpiConfig, Error> {
    if !(1..=MAX_DPI_STAGES).contains(&dpis.len()) {
        return Err(Error::InvalidDpiStageCount(dpis.len()));
    }
    validate_dpi_config(current)?;

    let active_value = current.stages[current.active];
    let active = if active_value.x == active_value.y {
        dpis.iter()
            .position(|dpi| *dpi == active_value.x)
            .unwrap_or(0)
    } else {
        0
    };
    Ok(DpiConfig {
        stages: dpis.iter().copied().map(DpiStage::scalar).collect(),
        active,
    })
}

/// Returns a copy with only the active index changed; stage values remain exact.
pub fn config_with_active_dpi(current: &DpiConfig, dpi: u16) -> Result<DpiConfig, Error> {
    validate_dpi_config(current)?;
    let active = current
        .stages
        .iter()
        .position(|stage| stage.x == dpi && stage.y == dpi)
        .ok_or(Error::DpiNotConfigured(dpi))?;
    Ok(DpiConfig {
        stages: current.stages.clone(),
        active,
    })
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
    match rate {
        PollingRate::Hz2000 | PollingRate::Hz4000 => Err(Error::WiredPollingTooHigh),
        _ => Ok(()),
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

fn write_u16_le(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
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
        response[..18].copy_from_slice(&[
            0xad, 3, 2, 0x90, 0x01, 0x90, 0x01, 0, 0x20, 0x03, 0x20, 0x03, 0, 0x40, 0x06, 0x40,
            0x06, 0,
        ]);
        assert_eq!(
            parse_dpi_response(&response).unwrap(),
            scalar_config(&[400, 800, 1600], 2)
        );
    }

    #[test]
    fn encodes_dpi_set_packet() {
        let report = encode_dpi_config(&scalar_config(&[400, 800, 1600], 1)).unwrap();
        assert_eq!(
            &report[..18],
            &[
                0x6d, 3, 1, 0x90, 1, 0x90, 1, 0, 0x20, 3, 0x20, 3, 0, 0x40, 6, 0x40, 6, 0
            ]
        );
        assert!(report[18..].iter().all(|byte| *byte == 0));
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
    fn dpi_use_preserves_existing_stages() {
        let current = DpiConfig {
            stages: vec![
                DpiStage { x: 400, y: 800 },
                DpiStage::scalar(800),
                DpiStage::scalar(1600),
            ],
            active: 2,
        };
        let updated = config_with_active_dpi(&current, 800).unwrap();
        assert_eq!(updated.stages, current.stages);
        assert_eq!(updated.active, 1);
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
        let updated = config_from_scalar_dpis(&current, &[400, 1600, 800]).unwrap();
        assert_eq!(updated.active, 2);
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
