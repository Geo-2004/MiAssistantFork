use crate::errors::{Error, Result};
use crate::usb::OpenDevice;
use std::convert::TryInto;
use tracing::{debug, instrument, trace};

pub const ADB_CONNECT: u32 = 0x4E584E43; // CNXN
pub const ADB_OPEN: u32 = 0x4E45504F; // OPEN
pub const ADB_OKAY: u32 = 0x59414B4F; // OKAY
pub const ADB_WRTE: u32 = 0x45545257; // WRTE
pub const ADB_CLSE: u32 = 0x45534C43; // CLSE

pub const ADB_MAX_DATA: u32 = 1024 * 1024;

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct AdbPacket {
    pub cmd: u32,
    pub arg0: u32,
    pub arg1: u32,
    pub len: u32,
    pub checksum: u32,
    pub magic: u32,
}

impl AdbPacket {
    pub fn new(cmd: u32, arg0: u32, arg1: u32, len: u32) -> Self {
        Self {
            cmd,
            arg0,
            arg1,
            len,
            checksum: 0,
            magic: cmd ^ 0xffffffff,
        }
    }
}

pub struct AdbTransport<'a> {
    pub dev: &'a mut OpenDevice,
    pub timeout_ms: u64,
}

impl<'a> AdbTransport<'a> {
    #[instrument(skip(self, payload), fields(cmd, len))]
    pub fn send(&mut self, pkt: &AdbPacket, payload: Option<&[u8]>) -> Result<()> {
        let cmd = pkt.cmd; // copy for tracing (avoid packed reference)
        let len = pkt.len;
        let header = encode_header(pkt);
        self.dev
            .bulk_write(self.dev.endpoints.bulk_out, &header, self.timeout_ms)?;
        if let Some(data) = payload {
            if !data.is_empty() {
                self.dev
                    .bulk_write(self.dev.endpoints.bulk_out, data, self.timeout_ms)?;
            }
        }
        trace!(cmd = format_args!("{:#X}", cmd), len, "sent packet");
        Ok(())
    }

    #[instrument(skip(self, buf), fields(timeout_ms = self.timeout_ms))]
    pub fn recv(&mut self, buf: &mut Vec<u8>) -> Result<AdbPacket> {
        let mut header = [0u8; 24];
        self.dev
            .bulk_read(self.dev.endpoints.bulk_in, &mut header, self.timeout_ms)?;
        let pkt = decode_header(&header)?;
        buf.clear();
        if pkt.len > 0 {
            buf.resize(pkt.len as usize, 0);
            self.dev
                .bulk_read(self.dev.endpoints.bulk_in, buf, self.timeout_ms)?;
        }
        trace!(?pkt, size = buf.len(), "recv packet");
        Ok(pkt)
    }

    #[instrument(skip(self), fields(cmd))]
    pub fn simple_command(&mut self, cmd: &str) -> Result<String> {
        let payload = cmd.as_bytes();
        self.send(
            &AdbPacket::new(ADB_OPEN, 1, 0, payload.len() as u32),
            Some(payload),
        )?;
        let mut data = Vec::new();
        let p = self.recv(&mut data)?; // expect WRTE or OKAY then WRTE
        if p.cmd == ADB_OKAY {
            let _ = self.recv(&mut data)?;
        }
        // send OKAY back
        self.send(&AdbPacket::new(ADB_OKAY, p.arg1, p.arg0, 0), None)?;
        // read CLSE (ignore payload)
        let _ = self.recv(&mut data)?;
        Ok(String::from_utf8_lossy(&data).trim_end().to_string())
    }

    #[instrument(skip(self))]
    pub fn connect(&mut self) -> Result<String> {
        // Construct banner: host::
        let banner = b"host::\0"; // include NUL
        self.send(
            &AdbPacket::new(
                ADB_CONNECT,
                0x01000001,
                crate::adb::ADB_MAX_DATA,
                banner.len() as u32,
            ),
            Some(banner),
        )?;
        let mut data = Vec::new();
        let pkt = self.recv(&mut data)?;
        if pkt.cmd != ADB_CONNECT {
            let cmd = pkt.cmd;
            return Err(Error::Protocol(format!(
                "Expected CNXN reply, got {:x}",
                cmd
            )));
        }
        let banner_str = String::from_utf8_lossy(&data).to_string();
        debug!(?banner_str, "connected banner");
        Ok(banner_str)
    }
}

// --- Header encode/decode helpers (testable & fuzzable) ---

pub fn encode_header(pkt: &AdbPacket) -> [u8; 24] {
    let mut header = [0u8; 24];
    header[0..4].copy_from_slice(&pkt.cmd.to_le_bytes());
    header[4..8].copy_from_slice(&pkt.arg0.to_le_bytes());
    header[8..12].copy_from_slice(&pkt.arg1.to_le_bytes());
    header[12..16].copy_from_slice(&pkt.len.to_le_bytes());
    header[16..20].copy_from_slice(&pkt.checksum.to_le_bytes());
    header[20..24].copy_from_slice(&pkt.magic.to_le_bytes());
    header
}

pub fn decode_header(buf: &[u8; 24]) -> Result<AdbPacket> {
    let pkt = AdbPacket {
        cmd: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
        arg0: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
        arg1: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
        len: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
        checksum: u32::from_le_bytes(buf[16..20].try_into().unwrap()),
        magic: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
    };
    // Basic validation
    if pkt.magic != (pkt.cmd ^ 0xffffffff) {
        return Err(Error::Protocol("magic mismatch".into()));
    }
    if pkt.len > ADB_MAX_DATA {
        return Err(Error::Protocol("payload too large".into()));
    }
    Ok(pkt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let pkt = AdbPacket::new(ADB_OPEN, 12, 34, 56);
        let hdr = encode_header(&pkt);
        let out = decode_header(&hdr).unwrap();
        let (c, a0, a1, l, m) = (out.cmd, out.arg0, out.arg1, out.len, out.magic);
        let (pc, pa0, pa1, pl, pm) = (pkt.cmd, pkt.arg0, pkt.arg1, pkt.len, pkt.magic);
        assert_eq!(pc, c);
        assert_eq!(pa0, a0);
        assert_eq!(pa1, a1);
        assert_eq!(pl, l);
        assert_eq!(pm, m);
    }

    #[test]
    fn bad_magic_rejected() {
        let pkt = AdbPacket::new(ADB_OKAY, 0, 0, 0);
        let mut hdr = encode_header(&pkt);
        // corrupt magic
        hdr[20] ^= 0xFF;
        let err = decode_header(&hdr).unwrap_err();
        match err {
            Error::Protocol(m) => assert!(m.contains("magic")),
            _ => panic!("unexpected error"),
        }
    }

    #[test]
    fn oversize_payload_rejected() {
        let pkt = AdbPacket::new(ADB_WRTE, 0, 0, ADB_MAX_DATA + 1);
        // encode manually to bypass size check
        let mut hdr = [0u8; 24];
        hdr[0..4].copy_from_slice(&pkt.cmd.to_le_bytes());
        hdr[4..8].copy_from_slice(&pkt.arg0.to_le_bytes());
        hdr[8..12].copy_from_slice(&pkt.arg1.to_le_bytes());
        hdr[12..16].copy_from_slice(&(ADB_MAX_DATA + 1).to_le_bytes());
        hdr[16..20].copy_from_slice(&pkt.checksum.to_le_bytes());
        hdr[20..24].copy_from_slice(&pkt.magic.to_le_bytes());
        let err = decode_header(&hdr).unwrap_err();
        match err {
            Error::Protocol(m) => assert!(m.contains("payload")),
            _ => panic!("unexpected error"),
        }
    }
}
