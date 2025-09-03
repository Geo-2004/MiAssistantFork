#[cfg(feature = "ui")]
use miassistant_core::{adb, device::DeviceInfo, md5, sideload, usb, validate};
#[cfg(feature = "ui")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "ui")]
use std::sync::mpsc::{self, Receiver, Sender};
#[cfg(feature = "ui")]
use std::sync::Arc;
#[cfg(feature = "ui")]
use std::thread;

#[cfg(feature = "ui")]
fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let native = eframe::NativeOptions::default();
    // We'll apply window size from persisted state in first frame.
    eframe::run_native(
        "MAF (MiAssistantFork) GUI",
        native,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
    .unwrap();
}

#[cfg(not(feature = "ui"))]
fn main() {
    eprintln!("GUI feature not enabled. Rebuild with --features ui");
}

#[cfg(feature = "ui")]
#[derive(Debug)]
enum Msg {
    Status(String),
    Log(String),
    Error(String),
    DeviceInfo(DeviceInfo),
    Roms(String),
    Token { token: String, erase: bool },
    Progress { sent: u64, total: u64 },
    FlashDone(Result<(), String>),
}

#[cfg(feature = "ui")]
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Language {
    #[default]
    En,
    Es,
}

#[cfg(feature = "ui")]
#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
struct Persisted {
    selected_file: Option<String>,
    window_size: Option<[f32; 2]>,
}

#[cfg(feature = "ui")]
#[derive(Default)]
struct App {
    status: String,
    logs: Vec<String>,
    device_info: Option<DeviceInfo>,
    roms: Option<String>,
    selected_file: Option<String>,
    validate_token: Option<String>,
    erase_flag: Option<bool>,
    progress: Option<(u64, u64)>,
    busy: bool,
    rx: Option<Receiver<Msg>>,
    persist: Persisted,
    pending_window_size: Option<egui::Vec2>,
    confirm_erase_dialog: bool,
    erase_confirmed: bool,
    cancel_token: Option<Arc<AtomicBool>>,
    devices: Vec<usb::DeviceSummary>,
    selected_device: Option<(u8, u8)>,
    last_error: Option<String>,
    last_flash_failed: bool,
    language: Language,
}

#[cfg(feature = "ui")]
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = App::default();
        if let Some(storage) = cc.storage {
            if let Some(s) = storage.get_string("miassistant_gui") {
                if let Ok(data) = serde_json::from_str::<Persisted>(&s) {
                    app.selected_file = data.selected_file.clone();
                    app.persist = data.clone();
                    if let Some([w, h]) = data.window_size {
                        app.pending_window_size = Some(egui::vec2(w, h));
                    }
                }
            }
        }
        app
    }
    fn t(&self, key: &str) -> String {
        match self.language {
            Language::En => match key {
                "EraseYes" => "Erase: YES (data wipe)".to_string(),
                _ => key.to_string(),
            },
            Language::Es => match key {
                "Detect Device" => "Detectar dispositivo".to_string(),
                "Read Info" => "Leer información".to_string(),
                "List ROMs" => "Listar ROMs".to_string(),
                "Pick ROM File" => "Elegir archivo ROM".to_string(),
                "Get Token" => "Obtener token".to_string(),
                "Flash" => "Flashear".to_string(),
                "Format Data" => "Formatear datos".to_string(),
                "Reboot" => "Reiniciar".to_string(),
                "Cancel Flash" => "Cancelar flasheo".to_string(),
                "Retry Flash" => "Reintentar flasheo".to_string(),
                "Refresh Devices" => "Actualizar dispositivos".to_string(),
                "Select device:" => "Seleccionar dispositivo:".to_string(),
                "Selected file:" => "Archivo seleccionado:".to_string(),
                "Token:" => "Token:".to_string(),
                "EraseYes" => "Borrado: SÍ (borra datos)".to_string(),
                "EraseNo" => "Borrado: No".to_string(),
                "Device Info" => "Información del dispositivo".to_string(),
                "ROMs" => "ROMs".to_string(),
                "Logs" => "Registros".to_string(),
                "Confirm Data Wipe" => "Confirmar borrado de datos".to_string(),
                "WipeMsg" => "Esta ROM requiere borrado de datos (Erase=YES).\nTodos los datos del dispositivo serán eliminados.".to_string(),
                "Cancel" => "Cancelar".to_string(),
                "Proceed" => "Continuar".to_string(),
                _ => key.to_string(),
            },
        }
    }
    fn push_log(&mut self, s: impl Into<String>) {
        let s = s.into();
        self.logs.push(s);
        if self.logs.len() > 500 {
            let remove = self.logs.len() - 500;
            self.logs.drain(0..remove);
        }
    }

    fn start_task<F: FnOnce(Sender<Msg>) + Send + 'static>(&mut self, f: F) {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.busy = true;
        thread::spawn(move || f(tx));
    }

    fn action_detect(&mut self) {
        if self.busy {
            return;
        }
        self.status = "Detecting device...".into();
        self.logs.clear();
        let selected = self.selected_device;
        self.start_task(move |tx| {
            tx.send(Msg::Status("Detecting ADB interface".into())).ok();
            let maybe_dev = match selected {
                Some((bus, addr)) => {
                    usb::open_by_location(bus, addr).or_else(|_| usb::find_first_adb())
                }
                None => usb::find_first_adb(),
            };
            let mut dev = match maybe_dev {
                Ok(d) => d,
                Err(e) => {
                    tx.send(Msg::Error(format!("{}", e))).ok();
                    return;
                }
            };
            let mut t = adb::AdbTransport {
                dev: &mut dev,
                timeout_ms: 5_000,
            };
            if let Err(e) = t.connect() {
                tx.send(Msg::Error(format!("ADB connect failed: {}", e)))
                    .ok();
                return;
            }
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
                match t.simple_command(query) {
                    Ok(val) => *field = val,
                    Err(e) => {
                        tx.send(Msg::Error(format!("ADB query {} failed: {}", query, e)))
                            .ok();
                        return;
                    }
                }
            }
            tx.send(Msg::DeviceInfo(info)).ok();
            tx.send(Msg::Status("Device detected".into())).ok();
        });
    }

    fn action_list_roms(&mut self) {
        if self.busy {
            return;
        }
        let info = match &self.device_info {
            Some(i) => i.clone(),
            None => {
                self.push_log("No device info. Detect first.");
                return;
            }
        };
        self.start_task(move |tx| {
            tx.send(Msg::Status("Querying Xiaomi ROM listings".into()))
                .ok();
            let v = match validate::Validator::new().and_then(|val| val.validate(&info, "", false))
            {
                Ok(validate::ValidationResult::Listing(val)) => val,
                Ok(_) => {
                    tx.send(Msg::Error("Unexpected response (expected listing)".into()))
                        .ok();
                    return;
                }
                Err(e) => {
                    tx.send(Msg::Error(format!("Validation failed: {}", e)))
                        .ok();
                    return;
                }
            };
            let pretty =
                serde_json::to_string_pretty(&v).unwrap_or_else(|_| "<unable to format>".into());
            tx.send(Msg::Roms(pretty)).ok();
            tx.send(Msg::Status("Done".into())).ok();
        });
    }

    fn action_pick_file(&mut self) {
        #[cfg(feature = "ui")]
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ZIP", &["zip"])
            .pick_file()
        {
            self.selected_file = Some(path.display().to_string());
        }
    }

    fn action_get_token(&mut self) {
        if self.busy {
            return;
        }
        let info = match &self.device_info {
            Some(i) => i.clone(),
            None => {
                self.push_log("No device info. Detect first.");
                return;
            }
        };
        let file = match &self.selected_file {
            Some(f) => f.clone(),
            None => {
                self.push_log("Pick a ROM .zip file first.");
                return;
            }
        };
        self.start_task(move |tx| {
            tx.send(Msg::Status("Computing MD5".into())).ok();
            let md5sum = match md5::md5_file(&file) {
                Ok(s) => s,
                Err(e) => {
                    tx.send(Msg::Error(format!("MD5 failed: {}", e))).ok();
                    return;
                }
            };
            tx.send(Msg::Log(format!("md5: {}", md5sum))).ok();
            let res =
                match validate::Validator::new().and_then(|v| v.validate(&info, &md5sum, true)) {
                    Ok(r) => r,
                    Err(e) => {
                        tx.send(Msg::Error(format!("Validation failed: {}", e)))
                            .ok();
                        return;
                    }
                };
            match res {
                validate::ValidationResult::FlashToken { token, erase } => {
                    let _ = tx.send(Msg::Token { token, erase });
                    let _ = tx.send(Msg::Status("Token ready".to_string()));
                }
                _ => {
                    let _ = tx.send(Msg::Error("Expected flash token".to_string()));
                }
            }
        });
    }

    fn action_flash(&mut self) {
        if self.busy {
            return;
        }
        let file = match &self.selected_file {
            Some(f) => f.clone(),
            None => {
                self.push_log("Pick a ROM .zip file first.");
                return;
            }
        };
        // If erase is indicated and not yet confirmed, open confirm dialog
        if matches!(self.erase_flag, Some(true)) && !self.erase_confirmed {
            self.confirm_erase_dialog = true;
            return;
        }
        let token_opt = self.validate_token.clone();
        let info_opt = self.device_info.clone();
        let selected = self.selected_device;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_token = Some(cancel.clone());
        self.start_task(move |tx| {
            tx.send(Msg::Status("Preparing to flash".into())).ok();
            let maybe_dev = match selected {
                Some((bus, addr)) => {
                    usb::open_by_location(bus, addr).or_else(|_| usb::find_first_adb())
                }
                None => usb::find_first_adb(),
            };
            let mut dev = match maybe_dev {
                Ok(d) => d,
                Err(e) => {
                    let _ = tx.send(Msg::Error(format!("{}", e)));
                    return;
                }
            };
            let mut t = adb::AdbTransport {
                dev: &mut dev,
                timeout_ms: 30_000,
            };
            if let Err(e) = t.connect() {
                tx.send(Msg::Error(format!("ADB connect failed: {}", e)))
                    .ok();
                return;
            }
            let token = if let Some(tok) = token_opt {
                tok
            } else if let Some(info) = info_opt {
                let md5sum = match md5::md5_file(&file) {
                    Ok(s) => s,
                    Err(e) => {
                        tx.send(Msg::Error(format!("MD5 failed: {}", e))).ok();
                        return;
                    }
                };
                match validate::Validator::new().and_then(|v| v.validate(&info, &md5sum, true)) {
                    Ok(validate::ValidationResult::FlashToken { token, .. }) => token,
                    Ok(_) => {
                        tx.send(Msg::Error("Expected flash token".into())).ok();
                        return;
                    }
                    Err(e) => {
                        tx.send(Msg::Error(format!("Validation failed: {}", e)))
                            .ok();
                        return;
                    }
                }
            } else {
                tx.send(Msg::Error("No device info. Detect first.".into()))
                    .ok();
                return;
            };
            // Use resumable sideload; progress isn't callback-based now, so we approximate by polling state file size.
            let res = sideload::sideload_resumable_with_progress(
                &mut t,
                &file,
                &token,
                &cancel,
                false,
                |sent, total| {
                    let _ = tx.send(Msg::Progress { sent, total });
                },
            );
            match res {
                Ok(()) => {
                    tx.send(Msg::FlashDone(Ok(()))).ok();
                    tx.send(Msg::Status("Flash done".into())).ok();
                }
                Err(e) => {
                    tx.send(Msg::FlashDone(Err(e.to_string()))).ok();
                    tx.send(Msg::Error(format!("Flash failed: {}", e))).ok();
                }
            }
        });
    }
}

#[cfg(feature = "ui")]
impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(s) = serde_json::to_string(&self.persist) {
            storage.set_string("miassistant_gui", s);
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // dark theme polish
        ctx.set_visuals(egui::Visuals::dark());
        // Apply pending window size from persisted state (once)
        if let Some(size) = self.pending_window_size.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }
        // Track window size for persistence
        let screen = ctx.input(|i| i.screen_rect());
        self.persist.window_size = Some([screen.width(), screen.height()]);

        // Handle messages from background without holding immutable borrow during mutation
        let mut drained = Vec::new();
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(m) => drained.push(m),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
        }
        for msg in drained {
            match msg {
                Msg::Status(s) => self.status = s,
                Msg::Log(s) => self.push_log(s),
                Msg::Error(e) => {
                    self.last_error = Some(e.clone());
                    self.push_log(format!("Error: {}", e));
                    self.status = "Error".into();
                    self.busy = false;
                }
                Msg::DeviceInfo(info) => {
                    self.device_info = Some(info);
                    self.busy = false;
                }
                Msg::Roms(s) => {
                    self.roms = Some(s);
                    self.busy = false;
                }
                Msg::Token { token, erase } => {
                    self.validate_token = Some(token);
                    self.erase_flag = Some(erase);
                    self.busy = false;
                }
                Msg::Progress { sent, total } => {
                    self.progress = Some((sent, total));
                }
                Msg::FlashDone(res) => {
                    self.busy = false;
                    self.erase_confirmed = false;
                    self.cancel_token = None;
                    self.last_flash_failed = res.is_err();
                    if let Err(e) = res {
                        self.last_error = Some(e);
                    }
                }
            }
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.heading("MAF – MiAssistantFork");
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if let Some(err) = &self.last_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("  {}", err));
                }
                ui.separator();
                egui::ComboBox::from_label("")
                    .selected_text(match self.language {
                        Language::En => "EN",
                        Language::Es => "ES",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.language, Language::En, "EN");
                        ui.selectable_value(&mut self.language, Language::Es, "ES");
                    });
            });
        });

        egui::SidePanel::left("left")
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Actions");
                ui.add_enabled_ui(!self.busy, |ui| {
                    if ui.button(self.t("Refresh Devices")).clicked() {
                        match usb::list_adb_devices() {
                            Ok(list) => {
                                self.devices = list;
                            }
                            Err(e) => self.push_log(format!("USB error: {}", e)),
                        }
                    }
                    if !self.devices.is_empty() {
                        ui.label(self.t("Select device:"));
                        let mut idx = self
                            .selected_device
                            .and_then(|(bus, addr)| {
                                self.devices
                                    .iter()
                                    .position(|d| d.bus == bus && d.address == addr)
                            })
                            .unwrap_or(0);
                        let labels: Vec<String> = self
                            .devices
                            .iter()
                            .map(|d| {
                                format!(
                                    "Bus {} Addr {} - {:04x}:{:04x}",
                                    d.bus, d.address, d.vendor_id, d.product_id
                                )
                            })
                            .collect();
                        egui::ComboBox::from_label("")
                            .selected_text(
                                labels
                                    .get(idx)
                                    .cloned()
                                    .unwrap_or_else(|| "<select>".into()),
                            )
                            .show_ui(ui, |ui| {
                                for (i, l) in labels.iter().enumerate() {
                                    if ui.selectable_label(i == idx, l).clicked() {
                                        idx = i;
                                    }
                                }
                            });
                        if let Some(sel) = self.devices.get(idx) {
                            self.selected_device = Some((sel.bus, sel.address));
                        }
                    }
                    if ui.button(self.t("Detect Device")).clicked() {
                        self.action_detect();
                    }
                    if ui.button(self.t("Read Info")).clicked() {
                        self.action_detect();
                    }
                    if ui.button(self.t("List ROMs")).clicked() {
                        self.action_list_roms();
                    }
                    if ui.button(self.t("Pick ROM File")).clicked() {
                        self.action_pick_file();
                    }
                    if ui.button(self.t("Get Token")).clicked() {
                        self.action_get_token();
                    }
                    if ui.button(self.t("Flash")).clicked() {
                        self.action_flash();
                    }
                    if !self.busy {
                        if let Some(f) = &self.selected_file {
                            if std::path::Path::new(&format!("{}.sideload.state", f)).exists()
                                && ui.button("Resume Flash").clicked()
                            {
                                self.erase_confirmed = true;
                                self.action_flash();
                            }
                        }
                    }
                    if self.last_flash_failed
                        && !self.busy
                        && ui.button(self.t("Retry Flash")).clicked()
                    {
                        self.last_flash_failed = false;
                        self.action_flash();
                    }
                    if self.cancel_token.is_some()
                        && self.busy
                        && ui.button(self.t("Cancel Flash")).clicked()
                    {
                        if let Some(c) = &self.cancel_token {
                            c.store(true, Ordering::Relaxed);
                        }
                    }
                    if ui.button(self.t("Format Data")).clicked() {
                        // quick inline action: do not block UI
                        let selected = self.selected_device;
                        self.start_task(move |tx| {
                            let maybe_dev = match selected {
                                Some((bus, addr)) => usb::open_by_location(bus, addr)
                                    .or_else(|_| usb::find_first_adb()),
                                None => usb::find_first_adb(),
                            };
                            let mut dev = match maybe_dev {
                                Ok(d) => d,
                                Err(e) => {
                                    tx.send(Msg::Error(format!("{}", e))).ok();
                                    return;
                                }
                            };
                            let mut t = adb::AdbTransport {
                                dev: &mut dev,
                                timeout_ms: 5_000,
                            };
                            if let Err(e) = t.connect() {
                                tx.send(Msg::Error(format!("ADB connect failed: {}", e)))
                                    .ok();
                                return;
                            }
                            match t.simple_command("format-data:") {
                                Ok(_) => {
                                    let _ = t.simple_command("reboot:");
                                    let _ = tx.send(Msg::Status("Format requested".to_string()));
                                }
                                Err(e) => {
                                    let _ = tx.send(Msg::Error(format!("Format failed: {}", e)));
                                }
                            }
                        });
                    }
                    if ui.button(self.t("Reboot")).clicked() {
                        let selected = self.selected_device;
                        self.start_task(move |tx| {
                            let maybe_dev = match selected {
                                Some((bus, addr)) => usb::open_by_location(bus, addr)
                                    .or_else(|_| usb::find_first_adb()),
                                None => usb::find_first_adb(),
                            };
                            let mut dev = match maybe_dev {
                                Ok(d) => d,
                                Err(e) => {
                                    tx.send(Msg::Error(format!("{}", e))).ok();
                                    return;
                                }
                            };
                            let mut t = adb::AdbTransport {
                                dev: &mut dev,
                                timeout_ms: 5_000,
                            };
                            if let Err(e) = t.connect() {
                                tx.send(Msg::Error(format!("ADB connect failed: {}", e)))
                                    .ok();
                                return;
                            }
                            match t.simple_command("reboot:") {
                                Ok(_) => {
                                    let _ = tx.send(Msg::Status("Reboot requested".to_string()));
                                }
                                Err(e) => {
                                    let _ = tx.send(Msg::Error(format!("Reboot failed: {}", e)));
                                }
                            }
                        });
                    }
                });

                ui.separator();
                ui.label(self.t("Selected file:"));
                ui.monospace(
                    self.selected_file
                        .clone()
                        .unwrap_or_else(|| "<none>".into()),
                );
                if let Some(token) = &self.validate_token {
                    ui.label(self.t("Token:"));
                    ui.monospace(token);
                }
                if let Some(erase) = self.erase_flag {
                    if erase {
                        ui.colored_label(egui::Color32::RED, self.t("EraseYes"));
                    } else {
                        ui.label(self.t("EraseNo"));
                    }
                }
                if let Some((sent, total)) = self.progress {
                    let frac = if total > 0 {
                        sent as f32 / total as f32
                    } else {
                        0.0
                    };
                    ui.add(egui::ProgressBar::new(frac).show_percentage());
                    ui.label(format!("{}/{} bytes", sent, total));
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(self.t("Device Info"));
            if let Some(info) = &self.device_info {
                ui.monospace(serde_json::to_string_pretty(info).unwrap_or_default());
            } else {
                ui.label("<not detected>");
            }
            ui.separator();
            ui.heading(self.t("ROMs"));
            if let Some(r) = &self.roms {
                ui.monospace(r);
            } else {
                ui.label("<none>");
            }
            ui.separator();
            ui.heading(self.t("Logs"));
            egui::ScrollArea::vertical().show(ui, |ui| {
                for l in &self.logs {
                    ui.monospace(l);
                }
            });

            // Confirm erase dialog
            if self.confirm_erase_dialog {
                egui::Window::new(self.t("Confirm Data Wipe"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.colored_label(egui::Color32::RED, self.t("WipeMsg"));
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button(self.t("Cancel")).clicked() {
                                self.confirm_erase_dialog = false;
                            }
                            if ui.button(self.t("Proceed")).clicked() {
                                self.confirm_erase_dialog = false;
                                self.erase_confirmed = true;
                                // Call flash again; erase flag has been acknowledged
                                self.action_flash();
                            }
                        });
                    });
            }
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}
