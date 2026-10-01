use std::fmt;

/// A model whose USB identity and protocol have been independently verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceModel {
    Aerox3WirelessGen2,
}

impl fmt::Display for DeviceModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aerox3WirelessGen2 => f.write_str("Aerox 3 Wireless Gen 2"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiStage {
    pub x: u16,
    pub y: u16,
    pub lod: LiftOffDistance,
}

impl DpiStage {
    #[must_use]
    pub const fn scalar(dpi: u16) -> Self {
        Self {
            x: dpi,
            y: dpi,
            lod: LiftOffDistance::Low,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiftOffDistance {
    Low,
    High,
}

impl fmt::Display for LiftOffDistance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => f.write_str("Low (1 mm)"),
            Self::High => f.write_str("High (2 mm)"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpiConfig {
    pub stages: Vec<DpiStage>,
    pub active: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiCapabilities {
    pub min: u16,
    pub max: u16,
    pub step: u16,
    pub max_stages: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollingRate {
    Hz125,
    Hz250,
    Hz500,
    Hz1000,
    Hz2000,
    Hz4000,
}

impl PollingRate {
    #[must_use]
    pub const fn hz(self) -> u16 {
        match self {
            Self::Hz125 => 125,
            Self::Hz250 => 250,
            Self::Hz500 => 500,
            Self::Hz1000 => 1000,
            Self::Hz2000 => 2000,
            Self::Hz4000 => 4000,
        }
    }
}

impl fmt::Display for PollingRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} Hz", self.hz())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollingConfig {
    pub wireless: PollingRate,
    pub wired: PollingRate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollingCapabilities {
    pub wireless: &'static [PollingRate],
    pub wired: &'static [PollingRate],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowPowerPollingRate {
    Hz125,
    Hz250,
    Hz500,
}

impl LowPowerPollingRate {
    #[must_use]
    pub const fn hz(self) -> u16 {
        match self {
            Self::Hz125 => 125,
            Self::Hz250 => 250,
            Self::Hz500 => 500,
        }
    }
}

impl fmt::Display for LowPowerPollingRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} Hz", self.hz())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SleepTimer(u32);

impl SleepTimer {
    #[must_use]
    pub(crate) const fn from_milliseconds(milliseconds: u32) -> Self {
        Self(milliseconds)
    }

    #[must_use]
    pub const fn milliseconds(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn whole_minutes(self) -> Option<u32> {
        if self.0.is_multiple_of(60_000) {
            Some(self.0 / 60_000)
        } else {
            None
        }
    }
}

impl fmt::Display for SleepTimer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.whole_minutes() {
            Some(minutes) => write!(f, "{minutes} min"),
            None => write!(f, "{} ms", self.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoLowPowerThreshold(u8);

impl AutoLowPowerThreshold {
    #[must_use]
    pub(crate) const fn new(percent: u8) -> Self {
        Self(percent)
    }

    #[must_use]
    pub const fn percent(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerConfig {
    pub low_power_enabled: bool,
    pub low_power_polling: LowPowerPollingRate,
    pub sleep_timer: SleepTimer,
    pub auto_low_power_enabled: bool,
    pub auto_low_power_threshold: AutoLowPowerThreshold,
    pub(crate) reserved: [u8; 55],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WirelessFeatures {
    pub wireless_stability_enabled: bool,
    pub bluetooth_smoothing_enabled: bool,
    pub(crate) reserved: [u8; 61],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WirelessFeatureCapabilities {
    pub wireless_stability: bool,
    pub bluetooth_smoothing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrollJumpConfig {
    pub enabled: bool,
    pub delay_ms: u16,
    pub(crate) reserved: [u8; 60],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollJumpCapabilities {
    pub supported: bool,
    pub delay_min_ms: u16,
    pub delay_max_ms: u16,
    pub delay_step_ms: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerCapabilities {
    pub low_power_polling: &'static [LowPowerPollingRate],
    pub auto_low_power_threshold_min: u8,
    pub auto_low_power_threshold_max: u8,
    pub sleep_timer_min_minutes: u64,
    pub sleep_timer_max_minutes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceCapabilities {
    pub dpi: DpiCapabilities,
    pub polling: PollingCapabilities,
    pub power: PowerCapabilities,
    pub wireless_features: WirelessFeatureCapabilities,
    pub scroll_jump: ScrollJumpCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryStatus {
    Available { percent: u8, charging: bool },
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceIdentity(String);

impl DeviceIdentity {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    Wired,
    Wireless2_4Ghz,
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wired => f.write_str("Wired"),
            Self::Wireless2_4Ghz => f.write_str("2.4 GHz"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalDevice {
    pub identity: DeviceIdentity,
    pub active_connection: ConnectionType,
    pub wired_endpoint_available: bool,
    pub receiver_endpoint_available: bool,
}
