use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use miassistant_core::{adb, device::DeviceInfo, md5, sideload, usb, validate};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser)]
#[command(
    author,
    version,
    about = "MAF (MiAssistantFork) – Xiaomi Recovery Flash & Rescue"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Enable verbose logs
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    Detect,
    Info,
    Adb {
        cmd: String,
    },
    Sideload {
        file: String,
        #[arg(short, long)]
        validate: Option<String>,
        #[arg(long)]
        resume: bool,
    },
    Md5 {
        file: String,
    },
    ListRoms,
    Flash {
        file: String,
        #[arg(long)]
        yes: bool,
    },
    FormatData,
    Reboot,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let filter = if cli.verbose { "debug" } else { "info" };
    fmt().with_env_filter(EnvFilter::new(filter)).init();
    match cli.command {
        Commands::Detect => {
            let _dev = usb::find_first_adb().context(
                "No device found. Put device in recovery/MiAssistant mode and connect via USB.",
            )?;
            println!("Device detected (endpoints ok)");
        }
        Commands::Info => {
            let mut dev = usb::find_first_adb().context(
                "No device found. Put device in recovery/MiAssistant mode and connect via USB.",
            )?;
            let mut t = adb::AdbTransport {
                dev: &mut dev,
                timeout_ms: 5_000,
            };
            t.connect().context("ADB connect failed")?;
            let mut info = DeviceInfo::default();
            for (field, query) in [
                (&mut info.device, "getdevice:"),
                (&mut info.version, "getversion:"),
                (&mut info.sn, "getsn:"),
                (&mut info.codebase, "getcodebase:"),
                (&mut info.branch, "getbranch:"),
                (&mut info.language, "getlanguage:"),
                (&mut info.region, "getregion:"),
                (&mut info.romzone, "getromzone:"),
            ] {
                *field = t
                    .simple_command(query)
                    .with_context(|| format!("Failed ADB query: {}", query))?;
            }
            println!("{}", serde_json::to_string_pretty(&info)?);
        }
        Commands::Adb { cmd } => {
            let mut dev = usb::find_first_adb().context(
                "No device found. Put device in recovery/MiAssistant mode and connect via USB.",
            )?;
            let mut t = adb::AdbTransport {
                dev: &mut dev,
                timeout_ms: 5_000,
            };
            t.connect().context("ADB connect failed")?;
            let out = t
                .simple_command(&cmd)
                .with_context(|| format!("Failed ADB command: {}", cmd))?;
            println!("{}", out);
        }
        Commands::Sideload {
            file,
            validate,
            resume,
        } => {
            use std::sync::{
                atomic::{AtomicBool, Ordering},
                Arc,
            };
            let cancel = Arc::new(AtomicBool::new(false));
            let _ = ctrlc::set_handler({
                let c = cancel.clone();
                move || {
                    c.store(true, Ordering::Relaxed);
                    eprintln!("Cancel requested – will save state and stop after current block");
                }
            });
            let mut dev = usb::find_first_adb().context(
                "No device found. Put device in recovery/MiAssistant mode and connect via USB.",
            )?;
            let mut t = adb::AdbTransport {
                dev: &mut dev,
                timeout_ms: 30_000,
            };
            t.connect().context("ADB connect failed")?;
            let token = if let Some(tok) = validate {
                tok
            } else {
                // Auto-compute token like 'flash' flow
                let md5sum = md5::md5_file(&file)
                    .with_context(|| format!("Failed to compute MD5 for {}", file))?;
                let mut info = DeviceInfo::default();
                for (field, query) in [
                    (&mut info.device, "getdevice:"),
                    (&mut info.version, "getversion:"),
                    (&mut info.sn, "getsn:"),
                    (&mut info.codebase, "getcodebase:"),
                    (&mut info.branch, "getbranch:"),
                    (&mut info.language, "getlanguage:"),
                    (&mut info.region, "getregion:"),
                    (&mut info.romzone, "getromzone:"),
                ] {
                    *field = t
                        .simple_command(query)
                        .with_context(|| format!("Failed ADB query: {}", query))?;
                }
                let res = validate::Validator::new()?
                    .validate(&info, &md5sum, true)
                    .context("Xiaomi validation failed (auth/token)")?;
                match res {
                    validate::ValidationResult::FlashToken { token, .. } => token,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Expected flash token (Validate) in Xiaomi response"
                        ))
                    }
                }
            };
            sideload::sideload_resumable(&mut t, &file, &token, &cancel, resume)
                .context("ADB sideload failed")?;
        }
        Commands::Md5 { file } => {
            println!("{}", md5::md5_file(&file)?);
        }
        Commands::ListRoms => {
            let mut dev = usb::find_first_adb().context(
                "No device found. Put device in recovery/MiAssistant mode and connect via USB.",
            )?;
            let mut t = adb::AdbTransport {
                dev: &mut dev,
                timeout_ms: 5_000,
            };
            t.connect().context("ADB connect failed")?;
            let mut info = DeviceInfo::default();
            for (field, query) in [
                (&mut info.device, "getdevice:"),
                (&mut info.version, "getversion:"),
                (&mut info.sn, "getsn:"),
                (&mut info.codebase, "getcodebase:"),
                (&mut info.branch, "getbranch:"),
                (&mut info.language, "getlanguage:"),
                (&mut info.region, "getregion:"),
                (&mut info.romzone, "getromzone:"),
            ] {
                *field = t
                    .simple_command(query)
                    .with_context(|| format!("Failed ADB query: {}", query))?;
            }
            let v = validate::Validator::new()?
                .validate(&info, "", false)
                .context("Xiaomi validation failed (listing)")?;
            if let validate::ValidationResult::Listing(val) = v {
                println!("{}", serde_json::to_string_pretty(&val)?);
            }
        }
        Commands::Flash { file, yes } => {
            use std::sync::{
                atomic::{AtomicBool, Ordering},
                Arc,
            };
            let cancel = Arc::new(AtomicBool::new(false));
            let _ = ctrlc::set_handler({
                let c = cancel.clone();
                move || {
                    c.store(true, Ordering::Relaxed);
                    eprintln!("Cancel requested – will stop after current block");
                }
            });
            let md5sum = md5::md5_file(&file)
                .with_context(|| format!("Failed to compute MD5 for {}", file))?;
            let mut dev = usb::find_first_adb().context(
                "No device found. Put device in recovery/MiAssistant mode and connect via USB.",
            )?;
            let mut t = adb::AdbTransport {
                dev: &mut dev,
                timeout_ms: 10_000,
            };
            t.connect().context("ADB connect failed")?;
            let mut info = DeviceInfo::default();
            for (field, query) in [
                (&mut info.device, "getdevice:"),
                (&mut info.version, "getversion:"),
                (&mut info.sn, "getsn:"),
                (&mut info.codebase, "getcodebase:"),
                (&mut info.branch, "getbranch:"),
                (&mut info.language, "getlanguage:"),
                (&mut info.region, "getregion:"),
                (&mut info.romzone, "getromzone:"),
            ] {
                *field = t
                    .simple_command(query)
                    .with_context(|| format!("Failed ADB query: {}", query))?;
            }
            let res = validate::Validator::new()?
                .validate(&info, &md5sum, true)
                .context("Xiaomi validation failed (auth/token)")?;
            if let validate::ValidationResult::FlashToken { token, erase } = res {
                if erase && !yes {
                    eprintln!("NOTICE: Data will be erased during flashing. Re-run with --yes to skip this prompt. Press Enter to continue...");
                    let mut s = String::new();
                    let _ = std::io::stdin().read_line(&mut s);
                }
                sideload::sideload_resumable(&mut t, &file, &token, &cancel, false)
                    .context("ADB sideload failed")?;
            } else {
                return Err(anyhow::anyhow!(
                    "Expected flash token (Validate) in Xiaomi response"
                ));
            }
        }
        Commands::FormatData => {
            let mut dev = usb::find_first_adb().context(
                "No device found. Put device in recovery/MiAssistant mode and connect via USB.",
            )?;
            let mut t = adb::AdbTransport {
                dev: &mut dev,
                timeout_ms: 5_000,
            };
            t.connect().context("ADB connect failed")?;
            let out = t
                .simple_command("format-data:")
                .context("Format data failed")?;
            println!("{}", out);
            let _ = t.simple_command("reboot:").context("Reboot failed")?;
        }
        Commands::Reboot => {
            let mut dev = usb::find_first_adb().context(
                "No device found. Put device in recovery/MiAssistant mode and connect via USB.",
            )?;
            let mut t = adb::AdbTransport {
                dev: &mut dev,
                timeout_ms: 5_000,
            };
            t.connect().context("ADB connect failed")?;
            let out = t.simple_command("reboot:").context("Reboot failed")?;
            println!("{}", out);
        }
    }
    Ok(())
}
