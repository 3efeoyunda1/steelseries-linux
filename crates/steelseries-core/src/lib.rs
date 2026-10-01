pub mod device;
pub mod devices;

pub use device::{
    AutoLowPowerThreshold, BatteryStatus, ConnectionType, DeviceCapabilities, DeviceIdentity,
    DeviceModel, DpiCapabilities, DpiConfig, DpiStage, LiftOffDistance, LowPowerPollingRate,
    PhysicalDevice, PollingCapabilities, PollingConfig, PollingRate, PowerCapabilities,
    PowerConfig, ScrollJumpCapabilities, ScrollJumpConfig, SleepTimer, WirelessFeatureCapabilities,
    WirelessFeatures,
};
pub use devices::aerox_3_wireless_gen2::{Aerox3WirelessGen2, Error};
