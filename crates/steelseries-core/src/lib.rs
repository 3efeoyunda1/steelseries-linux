pub mod device;
pub mod devices;

pub use device::{DeviceModel, DpiConfig, DpiStage, PollingConfig, PollingRate};
pub use devices::aerox_3_wireless_gen2::{Aerox3WirelessGen2, Error};
