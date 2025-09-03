#[cfg(feature = "ui")]
fn main() {
    eframe::run_native(
        "MiAssistantTool v2 GUI",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Box::new(App::default()))
    ).unwrap();
}

#[cfg(not(feature = "ui"))]
fn main() {
    eprintln!("GUI feature not enabled. Rebuild with --features ui");
}

#[cfg(feature = "ui")]
#[derive(Default)]
struct App;

#[cfg(feature = "ui")]
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("MiAssistantTool v2");
            ui.label("Experimental GUI placeholder");
        });
    }
}
