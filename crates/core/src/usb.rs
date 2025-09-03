use rusb::{Device, DeviceHandle, GlobalContext};
use crate::errors::{Result, Error};
use tracing::{debug};
use std::panic;

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

#[derive(Debug, Clone)]
pub struct DeviceSummary {
    pub bus: u8,
    pub address: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub has_adb: bool,
}

pub fn find_first_adb() -> Result<OpenDevice> {
    let devices_result = panic::catch_unwind(rusb::devices);
    let devices = match devices_result {
        Ok(Ok(devs)) => devs,
        Ok(Err(e)) => return Err(Error::Usb(format!("Failed to enumerate USB devices: {}", e))),
        Err(_) => return Err(Error::Usb("USB subsystem initialization failed. Please check if USB drivers are installed and you have permissions to access USB devices.".to_string())),
    };
    for device in devices.iter() {
        if let Some(eps) = inspect_device(&device)? {
            let handle = device.open().map_err(|e| Error::Usb(e.to_string()))?;
            #[cfg(not(windows))]
            let _ = handle.set_auto_detach_kernel_driver(true);
            handle.claim_interface(eps.interface_number).map_err(|e| Error::Usb(e.to_string()))?;
            return Ok(OpenDevice { handle, endpoints: eps });
        }
    }
    Err(Error::DeviceNotFound)
}

pub fn list_adb_devices() -> Result<Vec<DeviceSummary>> {
    let devices_result = panic::catch_unwind(rusb::devices);
    let devices = match devices_result {
        Ok(Ok(devs)) => devs,
        Ok(Err(e)) => return Err(Error::Usb(format!("Failed to enumerate USB devices: {}", e))),
        Err(_) => return Err(Error::Usb("USB subsystem initialization failed. Please check if USB drivers are installed and you have permissions to access USB devices.".to_string())),
    };
    let mut out = Vec::new();
    for device in devices.iter() {
        let dd = match device.device_descriptor() { Ok(d) => d, Err(_) => continue };
    let has = inspect_device(&device)?.is_some();
        if has {
            out.push(DeviceSummary { bus: device.bus_number(), address: device.address(), vendor_id: dd.vendor_id(), product_id: dd.product_id(), has_adb: true });
        }
    }
    Ok(out)
}

pub fn open_by_location(bus: u8, address: u8) -> Result<OpenDevice> {
    let devices = rusb::devices().map_err(|e| Error::Usb(e.to_string()))?;
    for device in devices.iter() {
        if device.bus_number() == bus && device.address() == address {
            if let Some(eps) = inspect_device(&device)? {
                let handle = device.open().map_err(|e| Error::Usb(e.to_string()))?;
                #[cfg(not(windows))]
                let _ = handle.set_auto_detach_kernel_driver(true);
                handle.claim_interface(eps.interface_number).map_err(|e| Error::Usb(e.to_string()))?;
                return Ok(OpenDevice { handle, endpoints: eps });
            }
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
