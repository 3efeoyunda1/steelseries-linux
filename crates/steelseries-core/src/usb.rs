use std::{
    ffi::{CStr, CString, OsStr},
    fs::{self, File, TryLockError},
    io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use hidapi::{HidApi, HidDevice};

use crate::Error;

const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UsbEndpoint {
    pub location: PathBuf,
    pub serial: Option<String>,
    path: CString,
    hid_node: PathBuf,
}

impl UsbEndpoint {
    pub fn resolve(path: &CStr, vendor: u16, product: u16, interface: i32) -> Result<Self, Error> {
        let device_path = Path::new(OsStr::from_bytes(path.to_bytes()));
        let resolve = || -> io::Result<Self> {
            let name = device_path
                .file_name()
                .filter(|name| name.as_encoded_bytes().starts_with(b"hidraw"))
                .ok_or_else(|| io::Error::other("expected a Linux hidraw path"))?;
            let hid_node = fs::canonicalize(Path::new("/sys/class/hidraw").join(name))?;
            Self::from_sysfs(path, hid_node, vendor, product, interface)
        };
        resolve().map_err(|source| Error::UsbTopology {
            path: device_path.to_owned(),
            source,
        })
    }

    fn from_sysfs(
        path: &CStr,
        hid_node: PathBuf,
        vendor: u16,
        product: u16,
        interface: i32,
    ) -> io::Result<Self> {
        let mut interface_verified = false;
        for parent in hid_node.ancestors() {
            if let Ok(value) = fs::read_to_string(parent.join("bInterfaceNumber")) {
                interface_verified = i32::from_str_radix(value.trim(), 16) == Ok(interface);
            }
            if let (Ok(vid), Ok(pid)) = (
                fs::read_to_string(parent.join("idVendor")),
                fs::read_to_string(parent.join("idProduct")),
            ) {
                if !interface_verified
                    || u16::from_str_radix(vid.trim(), 16) != Ok(vendor)
                    || u16::from_str_radix(pid.trim(), 16) != Ok(product)
                {
                    return Err(io::Error::other("hidraw USB device or interface changed"));
                }
                let serial = match fs::read_to_string(parent.join("serial")) {
                    Ok(serial) => Some(serial.trim_end_matches('\n').to_owned())
                        .filter(|serial| !serial.trim().is_empty()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error),
                };
                return Ok(Self {
                    location: parent.to_owned(),
                    serial,
                    path: path.to_owned(),
                    hid_node,
                });
            }
        }
        Err(io::Error::other("USB device ancestor was not found"))
    }

    pub fn open(
        &self,
        api: &HidApi,
        vendor: u16,
        product: u16,
        interface: i32,
        id: &str,
    ) -> Result<HidSession, Error> {
        let path = Path::new(OsStr::from_bytes(self.path.to_bytes()));
        let verify = || {
            if Self::resolve(&self.path, vendor, product, interface)
                .as_ref()
                .ok()
                == Some(self)
            {
                Ok(())
            } else {
                Err(Error::DeviceChanged(id.to_owned()))
            }
        };
        verify()?;
        // The advisory lock is on the hidraw device node itself. Keeping it for
        // the entire session serializes GET/SET/COMMIT across CLI processes.
        let open_error = |source| verify().err().unwrap_or_else(|| Error::open_error(source));
        let lock = File::open(path).map_err(|error| open_error(error.into()))?;
        lock_hid_node(&lock, LOCK_TIMEOUT)?;
        verify()?;
        let device = api.open_path(&self.path).map_err(open_error)?;
        verify()?;
        Ok(HidSession {
            device,
            _lock: lock,
        })
    }
}

fn lock_hid_node(file: &File, timeout: Duration) -> Result<(), Error> {
    let deadline = Instant::now() + timeout;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(TryLockError::Error(error)) => return Err(Error::Lock(error)),
            Err(TryLockError::WouldBlock) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(Error::LockTimeout);
                }
                thread::sleep(remaining.min(LOCK_RETRY_INTERVAL));
            }
        }
    }
}

pub(crate) struct HidSession {
    pub device: HidDevice,
    _lock: File,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hid_node(port: &Path, interface: u8, instance: &str, name: &str) -> PathBuf {
        let interface_path = port.join(format!("1-2:1.{interface}"));
        let hid_path = interface_path.join(instance).join("hidraw").join(name);
        fs::create_dir_all(&hid_path).unwrap();
        fs::write(
            interface_path.join("bInterfaceNumber"),
            format!("{interface:02x}\n"),
        )
        .unwrap();
        hid_path
    }

    #[test]
    fn locks_each_hid_node_for_one_session_at_a_time() {
        let node = tempfile::NamedTempFile::new().unwrap();
        let first = File::open(node.path()).unwrap();
        let second = File::open(node.path()).unwrap();
        lock_hid_node(&first, Duration::ZERO).unwrap();
        assert!(matches!(
            lock_hid_node(&second, Duration::ZERO),
            Err(Error::LockTimeout)
        ));
        drop(first);
        lock_hid_node(&second, Duration::ZERO).unwrap();
    }

    #[test]
    fn uses_physical_usb_port_even_when_the_interface_or_hidraw_node_changes() {
        let sys = tempfile::tempdir().unwrap();
        let port = sys.path().join("devices/pci0000:00/usb1/1-2");
        fs::create_dir_all(&port).unwrap();
        fs::write(port.join("idVendor"), "1038\n").unwrap();
        fs::write(port.join("idProduct"), "1836\n").unwrap();
        let first = UsbEndpoint::from_sysfs(
            c"/dev/hidraw18",
            hid_node(&port, 3, "0003:1038:1836.0042", "hidraw18"),
            0x1038,
            0x1836,
            3,
        )
        .unwrap();
        let reconnected = UsbEndpoint::from_sysfs(
            c"/dev/hidraw22",
            hid_node(&port, 3, "0003:1038:1836.0046", "hidraw22"),
            0x1038,
            0x1836,
            3,
        )
        .unwrap();
        let other_interface = UsbEndpoint::from_sysfs(
            c"/dev/hidraw25",
            hid_node(&port, 2, "0003:1038:1836.0047", "hidraw25"),
            0x1038,
            0x1836,
            2,
        )
        .unwrap();
        assert_eq!(first.location, port);
        assert_eq!(first.location, reconnected.location);
        assert_eq!(first.location, other_interface.location);
        assert_eq!(first.serial, None);
        assert_ne!(first, reconnected);
        assert!(
            UsbEndpoint::from_sysfs(
                c"/dev/hidraw25",
                other_interface.hid_node,
                0x1038,
                0x1836,
                3
            )
            .is_err()
        );

        fs::write(port.join("serial"), "physical-serial\n").unwrap();
        let with_serial =
            UsbEndpoint::from_sysfs(c"/dev/hidraw18", first.hid_node, 0x1038, 0x1836, 3).unwrap();
        assert_eq!(with_serial.serial.as_deref(), Some("physical-serial"));
    }
}
