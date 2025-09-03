use rusb::{Device, DeviceHandle, GlobalContext};
use crate::errors::{Result, Error};
use tracing::{debug};

pub const ADB_CLASS: u8 = 0xff;
pub const ADB_SUBCLASS: u8 = 0x42;
// pub const ADB_PROTOCOL: u8 = 0x01; // Sometimes absent / unreliable

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub bulk_in: u8,
    pub bulk_out: u8,
    pub interface_number: u8,
}

#[derive(Debug)]
pub struct OpenDevice {
    pub handle: DeviceHandle<GlobalContext>,
    pub endpoints: Endpoints,
}

pub fn find_first_adb() -> Result<OpenDevice> {
    for device in rusb::devices().map_err(|e| Error::Usb(e.to_string()))?.iter() {
        if let Some(eps) = inspect_device(&device)? {
            let handle = device.open().map_err(|e| Error::Usb(e.to_string()))?;
            #[cfg(not(windows))]
            let _ = handle.set_auto_detach_kernel_driver(true);
            handle.claim_interface(eps.interface_number as u8).map_err(|e| Error::Usb(e.to_string()))?;
            return Ok(OpenDevice { handle, endpoints: eps });
        }
    }
    Err(Error::DeviceNotFound)
}

fn inspect_device(device: &Device<GlobalContext>) -> Result<Option<Endpoints>> {
    let dd = device.device_descriptor().map_err(|e| Error::Usb(e.to_string()))?;
    for i in 0..dd.num_configurations() {
        let config = device.config_descriptor(i).map_err(|e| Error::Usb(e.to_string()))?;
        for interface in config.interfaces() {
            for descriptor in interface.descriptors() {
                if descriptor.class_code() == ADB_CLASS && descriptor.sub_class_code() == ADB_SUBCLASS {
                    let mut bulk_in = None;
                    let mut bulk_out = None;
                    for ep in descriptor.endpoint_descriptors() {
                        if ep.transfer_type() == rusb::TransferType::Bulk {
                            if ep.direction() == rusb::Direction::In && bulk_in.is_none() { bulk_in = Some(ep.address()); }
                            if ep.direction() == rusb::Direction::Out && bulk_out.is_none() { bulk_out = Some(ep.address()); }
                        }
                    }
                    if let (Some(bin), Some(bout)) = (bulk_in, bulk_out) {
                        debug!(?bin, ?bout, "Found ADB endpoints");
                        return Ok(Some(Endpoints { bulk_in: bin, bulk_out: bout, interface_number: descriptor.interface_number() }));
                    }
                }
            }
        }
    }
    Ok(None)
}

impl OpenDevice {
    pub fn bulk_read(&mut self, ep: u8, buf: &mut [u8], timeout_ms: u64) -> Result<usize> {
        self.handle.read_bulk(ep, buf, std::time::Duration::from_millis(timeout_ms))
            .map_err(|e| Error::Usb(e.to_string()))
    }
    pub fn bulk_write(&mut self, ep: u8, data: &[u8], timeout_ms: u64) -> Result<usize> {
        self.handle.write_bulk(ep, data, std::time::Duration::from_millis(timeout_ms))
            .map_err(|e| Error::Usb(e.to_string()))
    }
}
