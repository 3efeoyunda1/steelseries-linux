use std::{
    convert::TryFrom,
    time::{Duration, Instant},
};

use hidapi::{HidApi, HidDevice};
use thiserror::Error;

use crate::device::{DpiConfig, DpiStage, PollingConfig, PollingRate};

pub const VENDOR_ID: u16 = 0x1038;
pub const PRODUCT_ID: u16 = 0x1890;
pub const CONFIG_INTERFACE: i32 = 3;

pub const CMD_COMMIT: u8 = 0x11;
pub const CMD_SET_DPI: u8 = 0x6d;
pub const CMD_GET_DPI: u8 = 0xad;
pub const CMD_SET_POLLING: u8 = 0x6b;
pub const CMD_GET_POLLING: u8 = 0xab;

const REPORT_SIZE: usize = 64;
const WRITE_SIZE: usize = REPORT_SIZE + 1;
const RESPONSE_TIMEOUT: Duration = Duration::from_millis(1_500);
const MAX_DPI_STAGES: usize = 5;

#[derive(Debug, Error)]
pub enum Error {
    #[error("SteelSeries Aerox 3 Wireless Gen 2 receiver is not connected")]
    DeviceNotConnected,
    #[error("SteelSeries Aerox 3 Wireless Gen 2 receiver was found, but configuration HID interface 3 was not")]
    InterfaceNotFound,
    #[error("multiple SteelSeries Aerox 3 Wireless Gen 2 configuration interfaces were found; disconnect all but one receiver")]
    MultipleDevices,
    #[error("failed to initialize HID access: {0}")]
    HidInitialization(#[source] hidapi::HidError),
    #[error("failed to open SteelSeries Aerox 3 Wireless Gen 2 configuration interface: {0}")]
    Open(#[source] hidapi::HidError),
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
    #[error("malformed response to command 0x{command:02x}: expected at least {expected} bytes, received {actual}")]
    MalformedResponse {
        command: u8,
        expected: usize,
        actual: usize,
    },
    #[error(
        "unexpected response command byte: expected 0x{expected:02x}, received 0x{actual:02x}"
    )]
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
        let api = HidApi::new().map_err(Error::HidInitialization)?;
        Self::open_with_api(&api)
    }

    fn open_with_api(api: &HidApi) -> Result<Self, Error> {
        let product_matches: Vec<_> = api
            .device_list()
            .filter(|info| info.vendor_id() == VENDOR_ID && info.product_id() == PRODUCT_ID)
            .collect();

        if product_matches.is_empty() {
            return Err(Error::DeviceNotConnected);
        }

        let interface_matches: Vec<_> = product_matches
            .into_iter()
            .filter(|info| info.interface_number() == CONFIG_INTERFACE)
            .collect();

        match interface_matches.as_slice() {
            [] => Err(Error::InterfaceNotFound),
            [info] => info
                .open_device(api)
                .map(|device| Self { device })
                .map_err(Error::Open),
            _ => Err(Error::MultipleDevices),
        }
    }

    /// Returns whether the exact receiver and configuration interface are enumerated.
    pub fn is_connected() -> Result<bool, Error> {
        let api = HidApi::new().map_err(Error::HidInitialization)?;
        let connected = api.device_list().any(|info| {
            info.vendor_id() == VENDOR_ID
                && info.product_id() == PRODUCT_ID
                && info.interface_number() == CONFIG_INTERFACE
        });
        Ok(connected)
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
            &[0x6d, 3, 1, 0x90, 1, 0x90, 1, 0, 0x20, 3, 0x20, 3, 0, 0x40, 6, 0x40, 6, 0]
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
}
