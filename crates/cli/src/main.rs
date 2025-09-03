use clap::{Parser, Subcommand};
use miassistant_core::{usb, adb, device::DeviceInfo, sideload, md5, validate};
use tracing_subscriber::{EnvFilter, fmt};
use anyhow::Result;

#[derive(Parser)]
#[command(author, version, about = "MiAssistantTool v2 (Rust)")]
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
    Adb { cmd: String },
    Sideload { file: String, #[arg(short, long)] validate: Option<String> },
    Md5 { file: String },
    ListRoms,
    Flash { file: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let filter = if cli.verbose { "debug" } else { "info" };
    fmt().with_env_filter(EnvFilter::new(filter)).init();
    match cli.command {
        Commands::Detect => {
            let _dev = usb::find_first_adb()?; println!("Device detected (endpoints ok)");
        }
        Commands::Info => {
            let mut dev = usb::find_first_adb()?; let mut t = adb::AdbTransport { dev: &mut dev, timeout_ms: 5_000 }; t.connect()?; let mut info = DeviceInfo::default();
            for (field, query) in [
                (&mut info.device, "getdevice:"),
                (&mut info.version, "getversion:"),
                (&mut info.sn, "getsn:"),
                (&mut info.codebase, "getcodebase:"),
                (&mut info.branch, "getbranch:"),
                (&mut info.language, "getlanguage:"),
                (&mut info.region, "getregion:"),
                (&mut info.romzone, "getromzone:"),
            ] { *field = t.simple_command(query)?; }
            println!("{}", serde_json::to_string_pretty(&info)?);
        }
        Commands::Adb { cmd } => {
            let mut dev = usb::find_first_adb()?; let mut t = adb::AdbTransport { dev: &mut dev, timeout_ms: 5_000 }; t.connect()?; let out = t.simple_command(&cmd)?; println!("{}", out);
        }
        Commands::Sideload { file, validate } => {
            let mut dev = usb::find_first_adb()?; let mut t = adb::AdbTransport { dev: &mut dev, timeout_ms: 30_000 }; t.connect()?; let token = validate.unwrap_or_else(|| "token-placeholder".into()); sideload::sideload(&mut t, &file, &token)?;
        }
        Commands::Md5 { file } => { println!("{}", md5::md5_file(&file)?); }
        Commands::ListRoms => {
            let mut dev = usb::find_first_adb()?; let mut t = adb::AdbTransport { dev: &mut dev, timeout_ms: 5_000 }; t.connect()?; let mut info = DeviceInfo::default();
            for (field, query) in [
                (&mut info.device, "getdevice:"), (&mut info.version, "getversion:"), (&mut info.sn, "getsn:"), (&mut info.codebase, "getcodebase:"), (&mut info.branch, "getbranch:"), (&mut info.language, "getlanguage:"), (&mut info.region, "getregion:"), (&mut info.romzone, "getromzone:"),
            ] { *field = t.simple_command(query)?; }
            let v = validate::Validator::new()?.validate(&info, "", false)?; match v { validate::ValidationResult::Listing(val) => println!("{}", serde_json::to_string_pretty(&val)?), _ => {} }
        }
        Commands::Flash { file } => {
            let md5sum = md5::md5_file(&file)?; let mut dev = usb::find_first_adb()?; let mut t = adb::AdbTransport { dev: &mut dev, timeout_ms: 10_000 }; t.connect()?; let mut info = DeviceInfo::default();
            for (field, query) in [
                (&mut info.device, "getdevice:"), (&mut info.version, "getversion:"), (&mut info.sn, "getsn:"), (&mut info.codebase, "getcodebase:"), (&mut info.branch, "getbranch:"), (&mut info.language, "getlanguage:"), (&mut info.region, "getregion:"), (&mut info.romzone, "getromzone:"),
            ] { *field = t.simple_command(query)?; }
            let res = validate::Validator::new()?.validate(&info, &md5sum, true)?; if let validate::ValidationResult::FlashToken { token, erase } = res { eprintln!("erase={erase} token={}", token); sideload::sideload(&mut t, &file, &token)?; } else { return Err(anyhow::anyhow!("Expected flash token")); }
        }
    }
    Ok(())
}
