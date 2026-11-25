pub mod device;
pub mod devices;
mod error;
mod manager;
mod usb;
pub mod validation;

pub use device::{
    AutoLowPowerThreshold, BatteryCapabilities, BatteryStatus, ConnectionType, DeviceCapabilities,
    DeviceIdentity, DeviceModel, DpiCapabilities, DpiConfig, DpiStage, FeatureCapabilities,
    IdentitySource, LiftOffDistance, LiftOffDistanceCapabilities, LiftOffDistanceValueCapability,
    LowPowerPollingRate, PhysicalDevice, PollingCapabilities, PollingConfig, PollingRate,
    PowerCapabilities, PowerConfig, RangeCapabilities, RecommendedRange, ScrollJumpCapabilities,
    ScrollJumpConfig, SleepTimer, SleepTimerCapabilities, WirelessFeatureCapabilities,
    WirelessFeatures,
};
pub use devices::aerox_3_wireless_gen2::Aerox3WirelessGen2;
pub use error::Error;
pub use manager::{Device, DeviceDriver, DeviceFeature, PollingState};
