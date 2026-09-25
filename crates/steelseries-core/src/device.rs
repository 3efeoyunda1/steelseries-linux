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
}

impl DpiStage {
    #[must_use]
    pub const fn scalar(dpi: u16) -> Self {
        Self { x: dpi, y: dpi }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpiConfig {
    pub stages: Vec<DpiStage>,
    pub active: usize,
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
