use crate::adb::{AdbTransport, ADB_OPEN, ADB_WRTE, ADB_OKAY};
use crate::errors::{Result, Error};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use indicatif::{ProgressBar, ProgressStyle};

pub const SIDELOAD_CHUNK: usize = 1024 * 64;

pub fn sideload(transport: &mut AdbTransport, path: &str, validate: &str) -> Result<()> {
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let cmd = format!("sideload-host:{size}:{chunk}:{validate}:0", size=size, chunk=SIDELOAD_CHUNK);
    transport.send(&crate::adb::AdbPacket::new(ADB_OPEN, 1, 0, cmd.len() as u32 + 1), Some(cmd.as_bytes()))?;
    let pb = ProgressBar::new(size);
    pb.set_style(ProgressStyle::with_template("{bar:40.cyan/blue} {bytes}/{total_bytes} ({eta})")
        .unwrap());
    let mut block_buf = vec![0u8; SIDELOAD_CHUNK];
    let mut temp = Vec::new();
    loop {
        let pkt = transport.recv(&mut temp)?;
        if pkt.cmd == ADB_OKAY { transport.send(&crate::adb::AdbPacket::new(ADB_OKAY, pkt.arg1, pkt.arg0, 0), None)?; continue; }
        if pkt.cmd != ADB_WRTE { continue; }
        let block_str = String::from_utf8_lossy(&temp);
        if block_str.len() > 8 { break; }
        let block: u64 = block_str.trim().parse().map_err(|e| Error::Protocol(e.to_string()))?;
        let offset = block * SIDELOAD_CHUNK as u64;
        if offset > size { break; }
        let mut to_send = SIDELOAD_CHUNK as u64;
        if offset + to_send > size { to_send = size - offset; }
        f.seek(SeekFrom::Start(offset))?;
        let read = f.read(&mut block_buf[..to_send as usize])?;
        transport.send(&crate::adb::AdbPacket::new(ADB_WRTE, pkt.arg1, pkt.arg0, read as u32), Some(&block_buf[..read]))?;
        transport.send(&crate::adb::AdbPacket::new(ADB_OKAY, pkt.arg1, pkt.arg0, 0), None)?;
        pb.set_position(offset + read as u64);
    }
    pb.finish_with_message("Done");
    Ok(())
}
