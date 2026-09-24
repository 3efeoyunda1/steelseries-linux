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
