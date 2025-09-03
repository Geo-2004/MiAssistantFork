use crate::adb::{AdbTransport, ADB_OKAY, ADB_OPEN, ADB_WRTE};
use crate::errors::{Error, Result};
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub const SIDELOAD_CHUNK: usize = 1024 * 64;

#[derive(Debug, Serialize, Deserialize)]
struct SideloadState {
    file: String,
    size: u64,
    last_block: i64,
}

fn state_path(path: &str) -> String {
    format!("{}.sideload.state", path)
}

fn load_state(path: &str) -> Option<SideloadState> {
    let p = state_path(path);
    if Path::new(&p).exists() {
        std::fs::read(&p)
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
    } else {
        None
    }
}
fn save_state(st: &SideloadState) {
    let _ = std::fs::write(
        state_path(&st.file),
        serde_json::to_vec(st).unwrap_or_default(),
    );
}
fn clear_state(path: &str) {
    let _ = std::fs::remove_file(state_path(path));
}

pub fn sideload_resumable(
    transport: &mut AdbTransport,
    path: &str,
    validate: &str,
    cancel: &AtomicBool,
    resume: bool,
) -> Result<()> {
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let st = if resume { load_state(path) } else { None };
    let start_block: u64 = st
        .and_then(|s| {
            if s.size == size {
                Some((s.last_block + 1) as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let cmd = format!(
        "sideload-host:{size}:{chunk}:{validate}:{start_block}",
        size = size,
        chunk = SIDELOAD_CHUNK,
        validate = validate,
        start_block = start_block
    );
    let mut cmd_bytes = cmd.into_bytes();
    cmd_bytes.push(0);
    transport.send(
        &crate::adb::AdbPacket::new(ADB_OPEN, 1, 0, cmd_bytes.len() as u32),
        Some(&cmd_bytes),
    )?;
    let pb = ProgressBar::new(size);
    pb.set_style(
        ProgressStyle::with_template("{bar:40.cyan/blue} {bytes}/{total_bytes} ({eta})").unwrap(),
    );
    if start_block > 0 {
        pb.set_position(start_block * SIDELOAD_CHUNK as u64);
    }
    let mut block_buf = vec![0u8; SIDELOAD_CHUNK];
    let mut temp = Vec::new();
    let mut last_block: i64 = start_block as i64 - 1;
    loop {
        if cancel.load(Ordering::Relaxed) {
            save_state(&SideloadState {
                file: path.into(),
                size,
                last_block,
            });
            pb.abandon_with_message("Canceled");
            return Ok(());
        }
        let pkt = transport.recv(&mut temp)?;
        if pkt.cmd == ADB_OKAY {
            transport.send(
                &crate::adb::AdbPacket::new(ADB_OKAY, pkt.arg1, pkt.arg0, 0),
                None,
            )?;
            continue;
        }
        if pkt.cmd != ADB_WRTE {
            continue;
        }
        let block_str = String::from_utf8_lossy(&temp);
        if block_str.len() > 8 {
            break;
        }
        let block: u64 = block_str
            .trim()
            .parse::<u64>()
            .map_err(|e| Error::Protocol(e.to_string()))?;
        if block < start_block {
            continue;
        }
        let offset = block * SIDELOAD_CHUNK as u64;
        if offset > size {
            break;
        }
        let mut to_send = SIDELOAD_CHUNK as u64;
        if offset + to_send > size {
            to_send = size - offset;
        }
        f.seek(SeekFrom::Start(offset))?;
        let read = f.read(&mut block_buf[..to_send as usize])?;
        transport.send(
            &crate::adb::AdbPacket::new(ADB_WRTE, pkt.arg1, pkt.arg0, read as u32),
            Some(&block_buf[..read]),
        )?;
        transport.send(
            &crate::adb::AdbPacket::new(ADB_OKAY, pkt.arg1, pkt.arg0, 0),
            None,
        )?;
        pb.set_position(offset + read as u64);
        last_block = block as i64;
        if block % 16 == 0 {
            save_state(&SideloadState {
                file: path.into(),
                size,
                last_block,
            });
        }
    }
    clear_state(path);
    pb.finish_with_message("Done");
    Ok(())
}

pub fn sideload_resumable_with_progress<F>(
    transport: &mut AdbTransport,
    path: &str,
    validate: &str,
    cancel: &AtomicBool,
    resume: bool,
    mut progress: F,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let st = if resume { load_state(path) } else { None };
    let start_block: u64 = st
        .and_then(|s| {
            if s.size == size {
                Some((s.last_block + 1) as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let cmd = format!(
        "sideload-host:{size}:{chunk}:{validate}:{start_block}",
        size = size,
        chunk = SIDELOAD_CHUNK,
        validate = validate,
        start_block = start_block
    );
    let mut cmd_bytes = cmd.into_bytes();
    cmd_bytes.push(0);
    transport.send(
        &crate::adb::AdbPacket::new(ADB_OPEN, 1, 0, cmd_bytes.len() as u32),
        Some(&cmd_bytes),
    )?;
    if start_block > 0 {
        progress(start_block * SIDELOAD_CHUNK as u64, size);
    } else {
        progress(0, size);
    }
    let mut block_buf = vec![0u8; SIDELOAD_CHUNK];
    let mut temp = Vec::new();
    let mut last_block: i64 = start_block as i64 - 1;
    loop {
        if cancel.load(Ordering::Relaxed) {
            save_state(&SideloadState {
                file: path.into(),
                size,
                last_block,
            });
            return Ok(());
        }
        let pkt = transport.recv(&mut temp)?;
        if pkt.cmd == ADB_OKAY {
            transport.send(
                &crate::adb::AdbPacket::new(ADB_OKAY, pkt.arg1, pkt.arg0, 0),
                None,
            )?;
            continue;
        }
        if pkt.cmd != ADB_WRTE {
            continue;
        }
        let block_str = String::from_utf8_lossy(&temp);
        if block_str.len() > 8 {
            break;
        }
        let block: u64 = block_str
            .trim()
            .parse::<u64>()
            .map_err(|e| Error::Protocol(e.to_string()))?;
        if block < start_block {
            continue;
        }
        let offset = block * SIDELOAD_CHUNK as u64;
        if offset > size {
            break;
        }
        let mut to_send = SIDELOAD_CHUNK as u64;
        if offset + to_send > size {
            to_send = size - offset;
        }
        f.seek(SeekFrom::Start(offset))?;
        let read = f.read(&mut block_buf[..to_send as usize])?;
        transport.send(
            &crate::adb::AdbPacket::new(ADB_WRTE, pkt.arg1, pkt.arg0, read as u32),
            Some(&block_buf[..read]),
        )?;
        transport.send(
            &crate::adb::AdbPacket::new(ADB_OKAY, pkt.arg1, pkt.arg0, 0),
            None,
        )?;
        progress(offset + read as u64, size);
        last_block = block as i64;
        if block % 16 == 0 {
            save_state(&SideloadState {
                file: path.into(),
                size,
                last_block,
            });
        }
    }
    clear_state(path);
    progress(size, size);
    Ok(())
}
