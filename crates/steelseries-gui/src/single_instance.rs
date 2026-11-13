//! Linux abstract sockets provide atomic ownership and automatic crash cleanup.
//! This channel only activates the existing GUI; it never carries device commands.
use std::{
    io,
    os::{
        linux::net::SocketAddrExt,
        unix::{
            fs::MetadataExt,
            net::{SocketAddr, UnixDatagram},
        },
    },
};

pub fn claim(activate: bool) -> io::Result<Option<UnixDatagram>> {
    let uid = std::fs::metadata("/proc/self")?.uid();
    claim_named(format!("steelseries-linux-gui-{uid}").as_bytes(), activate)
}

fn claim_named(name: &[u8], activate: bool) -> io::Result<Option<UnixDatagram>> {
    let address = SocketAddr::from_abstract_name(name)?;
    match UnixDatagram::bind_addr(&address) {
        Ok(socket) => Ok(Some(socket)),
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            if activate {
                let client = UnixDatagram::unbound()?;
                client.set_write_timeout(Some(std::time::Duration::from_secs(1)))?;
                client.send_to_addr(b"open", &address)?;
            }
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_launch_activates_owner_and_exit_releases_ownership() {
        let name = format!("steelseries-test-{}-activation", std::process::id());
        let owner = claim_named(name.as_bytes(), true).unwrap().unwrap();
        owner
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        assert!(claim_named(name.as_bytes(), true).unwrap().is_none());
        let mut message = [0; 16];
        let size = owner.recv(&mut message).unwrap();
        assert_eq!(&message[..size], b"open");
        drop(owner);
        assert!(claim_named(name.as_bytes(), true).unwrap().is_some());
    }

    #[test]
    fn repeated_autostart_does_not_open_hidden_window() {
        let name = format!("steelseries-test-{}-autostart", std::process::id());
        let owner = claim_named(name.as_bytes(), false).unwrap().unwrap();
        owner.set_nonblocking(true).unwrap();
        assert!(claim_named(name.as_bytes(), false).unwrap().is_none());
        assert_eq!(
            owner.recv(&mut [0; 16]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
}
