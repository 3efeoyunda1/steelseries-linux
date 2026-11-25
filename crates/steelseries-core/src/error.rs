use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("no supported SteelSeries devices are connected")]
    DeviceNotConnected,
    #[error(
        "a supported SteelSeries USB device was found, but its configuration HID interface was not"
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
    #[error("failed to open SteelSeries configuration interface: {0}")]
    Open(#[source] hidapi::HidError),
    #[error(
        "failed to open SteelSeries configuration interface: {source}\n\nSteelSeries Linux udev permissions may not be installed or active.\nInstall the project's udev rules and reconnect the device."
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
    #[error(
        "{axis}-axis DPI {value} is invalid; DPI must be between {min} and {max} in steps of {step}"
    )]
    InvalidDpiValue {
        axis: &'static str,
        value: u16,
        stage: Option<usize>,
        min: u16,
        max: u16,
        step: u16,
    },
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
    #[error("wired polling rate {value} Hz is not supported; expected 125, 250, 500, or 1000 Hz")]
    WiredPollingTooHigh { value: u16 },
    #[error("expected polling set <HZ> or polling set <wired|wireless> <HZ>")]
    InvalidPollingArguments,
    #[error(
        "the selected device supports both wired and wireless polling. Specify a mode, for example 'steelseriesctl polling set wired {wired_hz}' or 'steelseriesctl polling set wireless {wireless_hz}'"
    )]
    PollingModeRequired { wired_hz: u16, wireless_hz: u16 },
    #[error("unsupported {connection} polling rate {hz} Hz. Supported rates in Hz: {rates}", rates = .allowed.iter().map(|rate| rate.hz().to_string()).collect::<Vec<_>>().join(", "))]
    InvalidPollingRate {
        connection: &'static str,
        hz: u16,
        allowed: &'static [crate::PollingRate],
    },
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
    #[error("multiple connected devices report the same ID {0}. Device selection is ambiguous")]
    AmbiguousDeviceId(String),
    #[error("{feature} is not supported by {model}")]
    UnsupportedFeature {
        model: crate::DeviceModel,
        feature: &'static str,
    },
    #[error("selected device ID {0} disconnected or changed connection. Run device list again")]
    DeviceChanged(String),
    #[error("cannot resolve the physical USB connection for {path}: {source}")]
    UsbTopology {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to lock the SteelSeries configuration interface: {0}")]
    Lock(#[source] std::io::Error),
    #[error("timed out waiting for another SteelSeries session to release the device")]
    LockTimeout,
    #[error(
        "malformed HID report for command 0x{command:02x}: expected exactly {expected} bytes, received {actual}"
    )]
    InvalidReportLength {
        command: u8,
        expected: usize,
        actual: usize,
    },
    #[error("the previous HID exchange did not complete. Reopen the device before retrying")]
    IncompleteHidExchange,
    #[error("unknown DPI sensor code 0x{0:02x}")]
    UnknownDpiCode(u8),
    #[error("invalid DPI response: {stage_count} stages with active index {active}")]
    MalformedDpiState { stage_count: usize, active: usize },
    #[error(
        "HID command 0x{command:02x} returned invalid acknowledgement bytes {status:02x} {detail:02x}"
    )]
    InvalidAcknowledgement { command: u8, status: u8, detail: u8 },
    #[error("invalid receiver link state 0x{0:02x}; expected 0x00 or 0x01")]
    InvalidReceiverLinkState(u8),
}

impl Error {
    pub(crate) fn open_error(error: hidapi::HidError) -> Self {
        if hid_error_is_permission_denied(&error) {
            Self::OpenPermissionDenied { source: error }
        } else {
            Self::Open(error)
        }
    }
}

pub(crate) fn hid_error_is_permission_denied(error: &hidapi::HidError) -> bool {
    match error {
        hidapi::HidError::IoError { error } => error.kind() == std::io::ErrorKind::PermissionDenied,
        hidapi::HidError::HidApiError { message } => {
            let message = message.to_ascii_lowercase();
            message.contains("permission denied") || message.contains("access denied")
        }
        _ => false,
    }
}
