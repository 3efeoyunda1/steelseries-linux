use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use hidapi::{BusType, HidApi};
use sha2::{Digest, Sha256};

use crate::{
    AutoLowPowerThreshold, BatteryStatus, ConnectionType, DeviceCapabilities, DeviceIdentity,
    DeviceModel, DpiConfig, Error, IdentitySource, LiftOffDistance, LowPowerPollingRate,
    PhysicalDevice, PollingRate, PowerConfig, ScrollJumpConfig, SleepTimer, WirelessFeatures,
    devices::{
        aerox_3_wired::{self as wired, Aerox3Wired},
        aerox_3_wireless_gen2::{self as wireless, Aerox3WirelessGen2},
    },
    usb::UsbEndpoint,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndpointKind {
    Wired,
    Receiver2_4Ghz,
}

#[derive(Clone, Copy)]
struct Registration {
    vendor: u16,
    product: u16,
    interface: i32,
    model: DeviceModel,
    kind: EndpointKind,
}

const REGISTRY: &[Registration] = &[
    Registration {
        vendor: wireless::VENDOR_ID,
        product: wireless::PID_WIRED,
        interface: wireless::CONFIG_INTERFACE,
        model: DeviceModel::Aerox3WirelessGen2,
        kind: EndpointKind::Wired,
    },
    Registration {
        vendor: wireless::VENDOR_ID,
        product: wireless::PID_RECEIVER,
        interface: wireless::CONFIG_INTERFACE,
        model: DeviceModel::Aerox3WirelessGen2,
        kind: EndpointKind::Receiver2_4Ghz,
    },
    Registration {
        vendor: wired::VENDOR_ID,
        product: wired::PRODUCT_ID,
        interface: wired::CONFIG_INTERFACE,
        model: DeviceModel::Aerox3Wired,
        kind: EndpointKind::Wired,
    },
];

pub(crate) fn registered_models() -> impl Iterator<Item = DeviceModel> {
    REGISTRY.iter().map(|entry| entry.model)
}

fn registration(vendor: u16, product: u16) -> Option<Registration> {
    REGISTRY
        .iter()
        .copied()
        .find(|entry| entry.vendor == vendor && entry.product == product)
}

impl DeviceModel {
    #[must_use]
    pub fn from_usb_id(vendor: u16, product: u16) -> Option<Self> {
        registration(vendor, product).map(|entry| entry.model)
    }

    #[must_use]
    pub const fn capabilities(self) -> &'static DeviceCapabilities {
        match self {
            Self::Aerox3WirelessGen2 => &Aerox3WirelessGen2::CAPABILITIES,
            Self::Aerox3Wired => &Aerox3Wired::CAPABILITIES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceFeature {
    LiftOffDistance,
    WirelessPolling,
    Battery,
    Power,
    WirelessStability,
    BluetoothSmoothing,
    ScrollJump,
}

impl DeviceFeature {
    pub fn require(self, model: DeviceModel) -> Result<(), Error> {
        let caps = model.capabilities();
        let supported = match self {
            Self::LiftOffDistance => caps.lod.supported,
            Self::WirelessPolling => !caps.polling.wireless.is_empty(),
            Self::Battery => caps.battery.supported,
            Self::Power => caps.power.supported,
            Self::WirelessStability => caps.wireless_features.wireless_stability.supported,
            Self::BluetoothSmoothing => caps.wireless_features.bluetooth_smoothing.supported,
            Self::ScrollJump => caps.scroll_jump.supported,
        };
        if supported {
            Ok(())
        } else {
            Err(Error::UnsupportedFeature {
                model,
                feature: self.name(),
            })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollingState {
    pub wired: PollingRate,
    pub wireless: Option<PollingRate>,
}

macro_rules! optional_operation {
    ($name:ident($($arg:ident: $type:ty),*) -> $result:ty, $feature:ident) => {
        fn $name(&self, $($arg: $type),*) -> Result<$result, Error> {
            Err(Error::UnsupportedFeature {
                model: self.model(),
                feature: DeviceFeature::$feature.name(),
            })
        }
    };
}

impl DeviceFeature {
    fn name(self) -> &'static str {
        match self {
            Self::LiftOffDistance => "lift-off distance",
            Self::WirelessPolling => "wireless polling",
            Self::Battery => "battery and charging status",
            Self::Power => "power management",
            Self::WirelessStability => "Wireless Stability Enhancement",
            Self::BluetoothSmoothing => "Bluetooth Smoothing",
            Self::ScrollJump => "Scroll Jump Protection",
        }
    }
}

/// A session on the selected HID interface. Mutations are persisted by calling
/// `commit` on the same session.
pub trait DeviceDriver {
    fn model(&self) -> DeviceModel;
    fn get_dpi_config(&self) -> Result<DpiConfig, Error>;
    fn set_dpi_config(&self, config: &DpiConfig) -> Result<(), Error>;
    fn replace_dpi_stages(&self, values: &[(u16, u16)]) -> Result<DpiConfig, Error>;
    fn set_active_dpi_stage(&self, stage_id: usize) -> Result<(), Error>;
    fn get_polling_config(&self) -> Result<PollingState, Error>;
    fn set_wired_polling(&self, rate: PollingRate) -> Result<(), Error>;
    fn commit(&self) -> Result<(), Error>;

    optional_operation!(set_lift_off_distance(_stage: usize, _lod: LiftOffDistance) -> (), LiftOffDistance);
    optional_operation!(set_wireless_polling(_rate: PollingRate) -> (), WirelessPolling);
    optional_operation!(get_battery_status() -> BatteryStatus, Battery);
    optional_operation!(get_power_config() -> PowerConfig, Power);
    optional_operation!(set_low_power_enabled(_enabled: bool) -> (), Power);
    optional_operation!(set_low_power_polling(_rate: LowPowerPollingRate) -> (), Power);
    optional_operation!(set_auto_low_power_enabled(_enabled: bool) -> (), Power);
    optional_operation!(set_auto_low_power_threshold(_threshold: AutoLowPowerThreshold) -> (), Power);
    optional_operation!(set_sleep_timer(_timer: SleepTimer) -> (), Power);
    optional_operation!(get_wireless_features() -> WirelessFeatures, WirelessStability);
    optional_operation!(set_wireless_stability(_enabled: bool) -> (), WirelessStability);
    optional_operation!(set_bluetooth_smoothing(_enabled: bool) -> (), BluetoothSmoothing);
    optional_operation!(get_scroll_jump_config() -> ScrollJumpConfig, ScrollJump);
    optional_operation!(set_scroll_jump_enabled(_enabled: bool) -> (), ScrollJump);
    optional_operation!(set_scroll_jump_delay(_delay_ms: u16) -> (), ScrollJump);
}

pub(crate) struct IdentifiedEndpoint<T> {
    pub model: DeviceModel,
    pub kind: EndpointKind,
    pub identity: DeviceIdentity,
    pub endpoint: T,
}

pub(crate) struct EndpointPair<T> {
    pub model: DeviceModel,
    pub identity: DeviceIdentity,
    pub wired: Option<T>,
    pub receiver: Option<T>,
}

impl<T> EndpointPair<T> {
    pub fn physical_device(&self) -> PhysicalDevice {
        PhysicalDevice {
            model: self.model,
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

    pub fn into_selected_endpoint(self) -> T {
        self.wired
            .or(self.receiver)
            .expect("physical device has an endpoint")
    }
}

pub(crate) fn group_identified_endpoints<T>(
    endpoints: impl IntoIterator<Item = IdentifiedEndpoint<T>>,
) -> Vec<EndpointPair<T>> {
    let mut by_identity: HashMap<_, Vec<_>> = HashMap::new();
    for entry in endpoints {
        by_identity
            .entry((entry.model, entry.identity))
            .or_default()
            .push((entry.kind, entry.endpoint));
    }

    let mut pairs = Vec::new();
    for ((model, identity), mut group) in by_identity {
        // A unique Wireless Gen 2 hardware ID can link one USB endpoint
        // and one receiver. Every repeated connection remains a collision.
        if model == DeviceModel::Aerox3WirelessGen2
            && identity.source() == IdentitySource::Hardware
            && group.len() == 2
            && group[0].0 != group[1].0
        {
            if group[0].0 == EndpointKind::Wired {
                group.swap(0, 1);
            }
            let wired = group.pop().map(|(_, endpoint)| endpoint);
            let receiver = group.pop().map(|(_, endpoint)| endpoint);
            pairs.push(EndpointPair {
                model,
                identity,
                wired,
                receiver,
            });
        } else {
            for (kind, endpoint) in group {
                let (wired, receiver) = match kind {
                    EndpointKind::Wired => (Some(endpoint), None),
                    EndpointKind::Receiver2_4Ghz => (None, Some(endpoint)),
                };
                pairs.push(EndpointPair {
                    model,
                    identity: identity.clone(),
                    wired,
                    receiver,
                });
            }
        }
    }
    pairs.sort_by(|a, b| a.identity.as_str().cmp(b.identity.as_str()));
    pairs
}

pub(crate) fn select_endpoint_pair<T>(
    mut pairs: Vec<EndpointPair<T>>,
    requested: Option<&str>,
) -> Result<EndpointPair<T>, Error> {
    if let Some(requested) = requested {
        let mut matches = pairs
            .iter()
            .enumerate()
            .filter(|(_, pair)| pair.identity.as_str() == requested)
            .map(|(index, _)| index);
        return match (matches.next(), matches.next()) {
            (Some(index), None) => Ok(pairs.swap_remove(index)),
            (Some(_), Some(_)) => Err(Error::AmbiguousDeviceId(requested.to_owned())),
            _ => {
                let available = identity_list(&pairs);
                let available = if available.is_empty() {
                    available
                } else {
                    format!("\n\nAvailable device IDs:{available}")
                };
                Err(Error::RequestedDeviceNotFound {
                    requested: requested.to_owned(),
                    available,
                })
            }
        };
    }
    match pairs.len() {
        0 => Err(Error::DeviceNotConnected),
        1 => Ok(pairs.pop().expect("one physical device exists")),
        _ => Err(Error::MultipleDevices {
            available: identity_list(&pairs),
        }),
    }
}

fn identity_list<T>(pairs: &[EndpointPair<T>]) -> String {
    let mut identities: Vec<_> = pairs.iter().map(|pair| pair.identity.as_str()).collect();
    identities.sort_unstable();
    identities
        .into_iter()
        .map(|id| format!("\n  {id}"))
        .collect()
}

fn identify(
    model: DeviceModel,
    hardware: Option<DeviceIdentity>,
    serial: Option<&str>,
    location: &Path,
) -> DeviceIdentity {
    if let Some(hardware) = hardware {
        return hardware;
    }
    if let Some(serial) = serial {
        return DeviceIdentity::with_source(serial, IdentitySource::Serial);
    }
    let mut hash = Sha256::new();
    hash.update(model.key());
    hash.update(b"\0");
    hash.update(location.as_os_str().as_encoded_bytes());
    let id = hash.finalize()[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    DeviceIdentity::with_source(id, IdentitySource::Custom)
}

struct Endpoint {
    registration: Registration,
    usb: UsbEndpoint,
}

impl Endpoint {
    fn open(&self, api: &HidApi, id: &str) -> Result<crate::usb::HidSession, Error> {
        let entry = self.registration;
        self.usb
            .open(api, entry.vendor, entry.product, entry.interface, id)
    }

    fn probe(&self, api: &HidApi) -> Result<Option<DeviceIdentity>, Error> {
        let entry = self.registration;
        let hardware = match entry.model {
            DeviceModel::Aerox3WirelessGen2 => {
                let session = self.open(api, self.usb.location.to_string_lossy().as_ref())?;
                let driver = Aerox3WirelessGen2::new(session, None);
                if entry.kind == EndpointKind::Receiver2_4Ghz
                    && !driver.get_receiver_link_active()?
                {
                    return Ok(None);
                }
                driver.probe_identity()?
            }
            DeviceModel::Aerox3Wired => None,
        };
        Ok(Some(identify(
            entry.model,
            hardware,
            self.usb.serial.as_deref(),
            &self.usb.location,
        )))
    }
}

#[derive(Default)]
struct Discovery {
    pairs: Vec<EndpointPair<Endpoint>>,
    first_error: Option<Error>,
    unlinked_receiver: bool,
}

fn discover(model: Option<DeviceModel>) -> Result<Discovery, Error> {
    let api = HidApi::new().map_err(Error::HidInitialization)?;
    let mut result = Discovery::default();
    let mut identified = Vec::new();
    let mut seen = HashSet::new();
    let mut usb_devices = HashSet::new();
    let mut config_devices = HashSet::new();
    for info in api.device_list() {
        let Some(entry) = registration(info.vendor_id(), info.product_id()) else {
            continue;
        };
        if !matches!(info.bus_type(), BusType::Usb)
            || model.is_some_and(|model| entry.model != model)
        {
            continue;
        }
        if !seen.insert(info.path()) {
            continue;
        }
        let mut probe = || {
            let usb = UsbEndpoint::resolve(
                info.path(),
                entry.vendor,
                entry.product,
                info.interface_number(),
            )?;
            usb_devices.insert((entry.product, usb.location.clone()));
            if info.interface_number() != entry.interface {
                return Ok(None);
            }
            config_devices.insert((entry.product, usb.location.clone()));
            let endpoint = Endpoint {
                registration: entry,
                usb,
            };
            let identity = endpoint.probe(&api)?;
            Ok(identity.map(|identity| IdentifiedEndpoint {
                model: entry.model,
                kind: entry.kind,
                identity,
                endpoint,
            }))
        };
        match probe() {
            Ok(Some(endpoint)) => identified.push(endpoint),
            Ok(None) => result.unlinked_receiver |= info.interface_number() == entry.interface,
            Err(error) => {
                result.first_error.get_or_insert(error);
            }
        }
    }
    if !usb_devices.is_subset(&config_devices) {
        result.first_error.get_or_insert(Error::InterfaceNotFound);
    }
    result.pairs = group_identified_endpoints(identified);
    Ok(result)
}

/// A selected physical device and its current command endpoint.
pub struct Device {
    physical: PhysicalDevice,
    endpoint: Endpoint,
}

impl Device {
    pub fn discover() -> Result<Vec<PhysicalDevice>, Error> {
        Self::discover_model(None)
    }

    pub(crate) fn discover_model(model: Option<DeviceModel>) -> Result<Vec<PhysicalDevice>, Error> {
        let found = discover(model)?;
        if found.pairs.is_empty()
            && let Some(error) = found.first_error
        {
            return Err(error);
        }
        Ok(found
            .pairs
            .iter()
            .map(EndpointPair::physical_device)
            .collect())
    }

    pub fn select(requested: Option<&str>) -> Result<Self, Error> {
        Self::select_model(None, requested)
    }

    pub(crate) fn select_model(
        model: Option<DeviceModel>,
        requested: Option<&str>,
    ) -> Result<Self, Error> {
        let found = discover(model)?;
        // Failed probes may conceal another physical device. An automatic
        // selection is safe only after every candidate has been accounted for.
        if ((requested.is_none() && found.pairs.len() <= 1) || found.pairs.is_empty())
            && let Some(error) = found.first_error
        {
            return Err(error);
        }
        if requested.is_none() && found.pairs.is_empty() && found.unlinked_receiver {
            return Err(Error::ReceiverLinkUnavailable);
        }
        let pair = select_endpoint_pair(found.pairs, requested)?;
        Ok(Self {
            physical: pair.physical_device(),
            endpoint: pair.into_selected_endpoint(),
        })
    }

    #[must_use]
    pub fn physical_device(&self) -> &PhysicalDevice {
        &self.physical
    }

    #[must_use]
    pub fn capabilities(&self) -> &'static DeviceCapabilities {
        self.physical.model.capabilities()
    }

    pub fn open(self) -> Result<Box<dyn DeviceDriver>, Error> {
        match self.physical.model {
            DeviceModel::Aerox3WirelessGen2 => Ok(Box::new(self.open_wireless()?)),
            DeviceModel::Aerox3Wired => {
                let api = HidApi::new().map_err(Error::HidInitialization)?;
                let session = self.endpoint.open(&api, self.physical.identity.as_str())?;
                Ok(Box::new(Aerox3Wired::new(session)))
            }
        }
    }

    pub(crate) fn open_wireless(self) -> Result<Aerox3WirelessGen2, Error> {
        let api = HidApi::new().map_err(Error::HidInitialization)?;
        let session = self.endpoint.open(&api, self.physical.identity.as_str())?;
        let driver = Aerox3WirelessGen2::new(session, Some(self.physical));
        if self.endpoint.registration.kind == EndpointKind::Receiver2_4Ghz {
            // The USB receiver can stay attached while the linked mouse changes.
            let physical = driver.physical_device();
            let changed = || Error::DeviceChanged(physical.identity.to_string());
            if !driver.get_receiver_link_active()? {
                return Err(changed());
            }
            let id = identify(
                physical.model,
                driver.probe_identity()?,
                self.endpoint.usb.serial.as_deref(),
                &self.endpoint.usb.location,
            );
            if id != physical.identity {
                return Err(changed());
            }
        }
        Ok(driver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIRED: DeviceModel = DeviceModel::Aerox3Wired;
    const WIRELESS: DeviceModel = DeviceModel::Aerox3WirelessGen2;
    const PORT: &str = "/sys/devices/pci0000:00/usb1/1-2/1-2.3";

    fn endpoint<T>(
        model: DeviceModel,
        kind: EndpointKind,
        identity: DeviceIdentity,
        endpoint: T,
    ) -> IdentifiedEndpoint<T> {
        IdentifiedEndpoint {
            model,
            kind,
            identity,
            endpoint,
        }
    }

    #[test]
    fn registers_only_verified_models_by_vid_and_pid() {
        for (pid, expected) in [(0x1836, WIRED), (0x1890, WIRELESS), (0x1892, WIRELESS)] {
            let entry = registration(0x1038, pid).unwrap();
            assert_eq!(entry.interface, 3);
            assert_eq!(DeviceModel::from_usb_id(0x1038, pid), Some(expected));
            assert_eq!(DeviceModel::from_usb_id(0x1234, pid), None);
        }
        assert_eq!(DeviceModel::from_usb_id(0x1038, 0x1837), None);
    }

    #[test]
    fn custom_id_is_the_first_eight_sha256_bytes_without_a_prefix() {
        let location = Path::new(PORT);
        let id = identify(WIRED, None, None, location);
        assert_eq!(id.as_str(), "7771d83499e94704");
        assert_eq!(id.as_str().len(), 16);
        assert!(
            id.as_str()
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert_eq!(id.source(), IdentitySource::Custom);
        assert_eq!(id, identify(WIRED, None, None, location));
        assert_ne!(
            id,
            identify(
                WIRED,
                None,
                None,
                Path::new("/sys/devices/pci0000:00/usb1/1-2/1-2.4")
            )
        );
        assert_ne!(id, identify(WIRELESS, None, None, location));
    }

    #[test]
    fn identity_prefers_hardware_then_actual_usb_serial() {
        let location = Path::new(PORT);
        let hardware = DeviceIdentity::new("6271700431492500250");
        let selected = identify(
            WIRELESS,
            Some(hardware.clone()),
            Some("usb-serial"),
            location,
        );
        assert_eq!(selected, hardware);
        assert_eq!(selected.source(), IdentitySource::Hardware);
        let selected = identify(WIRELESS, None, Some("A1B2C3"), location);
        assert_eq!(selected.as_str(), "A1B2C3");
        assert_eq!(selected.source(), IdentitySource::Serial);
    }

    #[test]
    fn selects_two_wired_mice_by_id_and_never_falls_back_after_disconnect() {
        let a = identify(WIRED, None, None, Path::new(PORT));
        let b = identify(
            WIRED,
            None,
            None,
            Path::new("/sys/devices/pci0000:00/usb1/1-2/1-2.4"),
        );
        let make_a = || endpoint(WIRED, EndpointKind::Wired, a.clone(), "path-a");
        let make_b = || endpoint(WIRED, EndpointKind::Wired, b.clone(), "path-b");
        assert!(matches!(
            select_endpoint_pair(group_identified_endpoints([make_a(), make_b()]), None),
            Err(Error::MultipleDevices { .. })
        ));
        for (id, path) in [(a.as_str(), "path-a"), (b.as_str(), "path-b")] {
            let pair =
                select_endpoint_pair(group_identified_endpoints([make_a(), make_b()]), Some(id))
                    .unwrap();
            assert_eq!(pair.into_selected_endpoint(), path);
        }
        assert!(matches!(
            select_endpoint_pair(group_identified_endpoints([make_b()]), Some(a.as_str())),
            Err(Error::RequestedDeviceNotFound { .. })
        ));
    }

    #[test]
    fn polling_capabilities_follow_the_selected_device_in_a_mixed_list() {
        let endpoints = || {
            // Both mice are connected using USB. Their supported modes differ.
            group_identified_endpoints([
                endpoint(
                    WIRELESS,
                    EndpointKind::Wired,
                    DeviceIdentity::new("gen2"),
                    (),
                ),
                endpoint(WIRED, EndpointKind::Wired, DeviceIdentity::new("wired"), ()),
            ])
        };
        assert!(matches!(
            select_endpoint_pair(endpoints(), None),
            Err(Error::MultipleDevices { .. })
        ));

        let wired = select_endpoint_pair(endpoints(), Some("wired"))
            .unwrap()
            .physical_device();
        let gen2 = select_endpoint_pair(endpoints(), Some("gen2"))
            .unwrap()
            .physical_device();
        assert_eq!(wired.active_connection, gen2.active_connection);
        assert!(!wired.model.capabilities().polling.wired.is_empty());
        assert!(wired.model.capabilities().polling.wireless.is_empty());
        assert!(!gen2.model.capabilities().polling.wired.is_empty());
        assert!(
            gen2.model
                .capabilities()
                .polling
                .wireless
                .contains(&PollingRate::Hz4000)
        );
    }

    #[test]
    fn never_collapses_duplicate_hardware_ids_into_a_wireless_pair() {
        let id = DeviceIdentity::new("6271700431492500250");
        for repeated_kind in [EndpointKind::Wired, EndpointKind::Receiver2_4Ghz] {
            let other = if repeated_kind == EndpointKind::Wired {
                EndpointKind::Receiver2_4Ghz
            } else {
                EndpointKind::Wired
            };
            let pairs = group_identified_endpoints([
                endpoint(WIRELESS, repeated_kind, id.clone(), "first"),
                endpoint(WIRELESS, repeated_kind, id.clone(), "second"),
                endpoint(WIRELESS, other, id.clone(), "third"),
            ]);
            assert_eq!(pairs.len(), 3);
            assert!(matches!(select_endpoint_pair(pairs, Some(id.as_str())),
                Err(Error::AmbiguousDeviceId(value)) if value == id.as_str()));
        }
    }

    #[test]
    fn detects_collisions_across_models_and_identity_sources() {
        let id = "7771d83499e94704";
        let pairs = group_identified_endpoints([
            endpoint(
                WIRED,
                EndpointKind::Wired,
                DeviceIdentity::with_source(id, IdentitySource::Custom),
                "wired",
            ),
            endpoint(
                WIRELESS,
                EndpointKind::Receiver2_4Ghz,
                DeviceIdentity::with_source(id, IdentitySource::Serial),
                "receiver",
            ),
        ]);
        assert_eq!(pairs.len(), 2);
        assert!(matches!(
            select_endpoint_pair(pairs, Some(id)),
            Err(Error::AmbiguousDeviceId(_))
        ));

        let serial = DeviceIdentity::with_source("shared", IdentitySource::Serial);
        let pairs = group_identified_endpoints([
            endpoint(WIRELESS, EndpointKind::Wired, serial.clone(), "wired"),
            endpoint(WIRELESS, EndpointKind::Receiver2_4Ghz, serial, "receiver"),
        ]);
        assert_eq!(pairs.len(), 2);
        assert!(matches!(
            select_endpoint_pair(pairs, Some("shared")),
            Err(Error::AmbiguousDeviceId(_))
        ));
    }

    #[test]
    fn rejects_wired_features_before_the_cli_opens_an_endpoint() {
        for feature in [
            DeviceFeature::Battery,
            DeviceFeature::Power,
            DeviceFeature::LiftOffDistance,
            DeviceFeature::WirelessPolling,
            DeviceFeature::WirelessStability,
            DeviceFeature::BluetoothSmoothing,
            DeviceFeature::ScrollJump,
        ] {
            assert!(matches!(
                feature.require(WIRED),
                Err(Error::UnsupportedFeature { model: WIRED, .. })
            ));
            feature.require(WIRELESS).unwrap();
        }
    }
}
