mod model;
mod presentation;
mod ssh;
mod theme;
mod views;
use dmidecode_rs::Inventory;
use eframe::egui;
use model::Snapshot;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // The elevated subprocess only reads and prints data; it never starts a GUI.
    if args.len() == 1 && args[0] == "--read-system" {
        match Snapshot::live().and_then(|s| serde_json::to_string(&s).map_err(|e| e.to_string())) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--help" || a == "-h") {
        println!("dmidecode-gui [DUMP_FILE]\ndmidecode-gui --ssh USER@HOST [--port PORT] [--sudo]\nOpen a dump, read the local system, or fetch Linux firmware over SSH.");
        return Ok(());
    }
    let source = parse_source(&args).unwrap_or_else(|error| {
        eprintln!("{error}\nUse --help for usage.");
        std::process::exit(2);
    });
    #[cfg(feature = "screenshots")]
    let window_width = std::env::var("DMI_SCREENSHOT_WIDTH")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(1280.0)
        .max(900.0);
    #[cfg(not(feature = "screenshots"))]
    let window_width = 1280.0;
    eframe::run_native(
        "DMI Explorer",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([window_width, 900.0])
                .with_min_inner_size([900.0, 600.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            theme::setup(&cc.egui_ctx);
            let mut app = Explorer::default();
            #[cfg(feature = "screenshots")]
            if std::env::var_os("DMI_SCREENSHOT_TO").is_some() {
                if let Source::Dump(path) = &source {
                    match Inventory::from_dump(path)
                        .map_err(|e| e.to_string())
                        .and_then(Snapshot::decode)
                    {
                        Ok(snapshot) => app.snapshot = Some(snapshot),
                        Err(error) => app.error = Some(error),
                    }
                }
                if let Ok(page) = std::env::var("DMI_SCREENSHOT_CATEGORY") {
                    app.category = page.parse::<usize>().unwrap_or(0).min(7);
                    app.browsing = true;
                }
                app.ssh_open = std::env::var_os("DMI_SCREENSHOT_SSH").is_some();
                cc.egui_ctx.request_repaint();
                return Ok(Box::new(app));
            }
            match source {
                Source::Ssh(connection) => app.connect_ssh(cc.egui_ctx.clone(), connection),
                Source::Dump(path) => app.load(cc.egui_ctx.clone(), move || {
                    Snapshot::decode(Inventory::from_dump(path).map_err(|e| e.to_string())?)
                }),
                Source::Local => app.load(cc.egui_ctx.clone(), Snapshot::live),
            }
            Ok(Box::new(app))
        }),
    )
}

enum Source {
    Local,
    Dump(PathBuf),
    Ssh(ssh::Connection),
}
fn parse_source(args: &[std::ffi::OsString]) -> Result<Source, String> {
    if args.is_empty() {
        return Ok(Source::Local);
    }
    if args[0] != "--ssh" {
        if args.len() == 1 && !args[0].to_string_lossy().starts_with('-') {
            return Ok(Source::Dump(PathBuf::from(&args[0])));
        }
        return Err("Expected a dump filename or --ssh USER@HOST [--port PORT] [--sudo].".into());
    }
    let target = args
        .get(1)
        .and_then(|s| s.to_str())
        .ok_or("--ssh requires a host or user@host.")?;
    let mut port = None;
    let mut sudo = false;
    let mut i = 2;
    while i < args.len() {
        match args[i].to_str() {
            Some("--sudo") if !sudo => sudo = true,
            Some("--port") if port.is_none() => {
                i += 1;
                port = Some(
                    args.get(i)
                        .and_then(|s| s.to_str())
                        .ok_or("--port requires a number.")?,
                );
            }
            _ => {
                return Err("Unknown or repeated SSH option. Use --port PORT and/or --sudo.".into())
            }
        }
        i += 1;
    }
    Ok(Source::Ssh(ssh::Connection::new(
        target,
        port.unwrap_or(""),
        sudo,
    )?))
}

#[derive(Default)]
struct Explorer {
    snapshot: Option<Snapshot>,
    pending: Option<Receiver<Result<Snapshot, String>>>,
    error: Option<String>,
    notice: String,
    category: usize,
    query: String,
    selected: Option<usize>,
    tab: usize,
    browsing: bool,
    ssh_open: bool,
    ssh_target: String,
    ssh_port: String,
    ssh_sudo: bool,
    ssh_error: Option<String>,
    ssh_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    loading_label: String,
    #[cfg(feature = "screenshots")]
    screenshot_frames: u32,
}
impl Explorer {
    fn connect_ssh(&mut self, ctx: egui::Context, connection: ssh::Connection) {
        self.ssh_target = connection.target.clone();
        self.ssh_port = connection.port.map(|p| p.to_string()).unwrap_or_default();
        self.ssh_sudo = connection.sudo;
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let label = format!("Reading {} over SSH…", connection.target);
        self.load(ctx, move || connection.fetch(worker_cancel));
        self.ssh_cancel = Some(cancel);
        self.loading_label = label;
    }
    fn load(
        &mut self,
        ctx: egui::Context,
        job: impl FnOnce() -> Result<Snapshot, String> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.error = None;
        self.notice.clear();
        self.loading_label = "Reading firmware…".into();
        std::thread::spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
    }
    fn poll(&mut self) {
        if let Some(rx) = &self.pending {
            let result = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("The reader stopped unexpectedly.".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                self.pending = None;
                self.ssh_cancel = None;
                self.loading_label.clear();
                match result {
                    Ok(snapshot) => {
                        self.snapshot = Some(snapshot);
                        self.selected = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
    }
}
impl Drop for Explorer {
    fn drop(&mut self) {
        if let Some(cancel) = &self.ssh_cancel {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}
impl eframe::App for Explorer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
        #[cfg(feature = "screenshots")]
        if let Some(path) = std::env::var_os("DMI_SCREENSHOT_TO") {
            self.screenshot_frames += 1;
            if self.screenshot_frames == 10 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let pixels: Vec<_> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                    if let Err(error) = image::save_buffer(
                        &path,
                        &pixels,
                        image.size[0] as u32,
                        image.size[1] as u32,
                        image::ColorType::Rgba8,
                    ) {
                        eprintln!("Screenshot failed: {error}");
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}

#[cfg(test)]
mod ui_tests {
    use super::*;
    #[test]
    fn parses_ssh_launch_options_and_keeps_dump_paths() {
        let args = |s: &[&str]| s.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
        assert!(
            matches!(parse_source(&args(&["--ssh","admin@rack","--sudo","--port","2222"])).unwrap(), Source::Ssh(c) if c.sudo && c.port == Some(2222) && c.target == "admin@rack")
        );
        assert!(matches!(
            parse_source(&args(&["dump with spaces.bin"])).unwrap(),
            Source::Dump(_)
        ));
        for bad in [
            &["--ssh"][..],
            &["--ssh", "host", "--port"],
            &["--sudo"],
            &["--ssh", "host", "--sudo", "--sudo"],
        ] {
            assert!(parse_source(&args(bad)).is_err());
        }
    }
    #[test]
    fn renders_ssh_dialog_without_connecting() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let mut app = Explorer::default();
        app.ssh_open = true;
        app.ssh_sudo = true;
        let output = ctx.run(egui::RawInput::default(), |ctx| app.ui(ctx));
        assert!(!output.shapes.is_empty());
        assert!(app.pending.is_none());
    }
    #[test]
    fn renders_empty_error_and_filtered_record_views() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let mut app = Explorer::default();
        let render = |app: &mut Explorer| {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1180.0, 780.0),
                    )),
                    ..Default::default()
                },
                |ctx| app.ui(ctx),
            );
            assert!(!output.shapes.is_empty());
        };
        render(&mut app);
        app.error = Some("Permission denied".into());
        render(&mut app);
        app.snapshot = Some(
            Snapshot::decode(Inventory::from_bytes(
                vec![
                    1, 8, 0x34, 0x12, 1, 2, 0, 0, b'A', 0, b'B', 0, 0, 127, 4, 0xff, 0xff, 0, 0,
                ],
                None,
            ))
            .unwrap(),
        );
        app.browsing = true;
        for tab in 0..4 {
            app.tab = tab;
            render(&mut app);
        }
        assert_eq!(app.selected, Some(0));
        app.query = "no-such-record".into();
        render(&mut app);
        assert_eq!(app.selected, None);
        app.query = "0x1234".into();
        render(&mut app);
        assert_eq!(app.selected, Some(0));
        app.category = 5;
        render(&mut app);
        assert_eq!(app.selected, None);
    }
    #[test]
    fn workstation_pages_render_at_supported_widths() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let mut app = Explorer::default();
        app.snapshot = Some(
            Snapshot::decode(Inventory::from_bytes(
                include_bytes!("../tests/fixtures/workstation.bin").to_vec(),
                None,
            ))
            .unwrap(),
        );
        for width in [900.0, 1280.0] {
            for page in 0usize..=8 {
                app.browsing = page > 0;
                app.category = page.saturating_sub(1);
                for tab in 0..3 {
                    app.tab = tab;
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 900.0),
                            )),
                            ..Default::default()
                        },
                        |ctx| app.ui(ctx),
                    );
                    assert!(!output.shapes.is_empty());
                }
            }
        }
    }
    #[test]
    fn failed_load_preserves_previous_snapshot() {
        let mut app = Explorer::default();
        app.snapshot =
            Some(Snapshot::decode(Inventory::from_bytes(vec![127, 4, 0, 0, 0, 0], None)).unwrap());
        let (tx, rx) = mpsc::channel();
        tx.send(Err("Unreadable dump".into())).unwrap();
        app.pending = Some(rx);
        app.poll();
        assert!(app.snapshot.is_some());
        assert_eq!(app.error.as_deref(), Some("Unreadable dump"));
        assert!(app.pending.is_none());
    }
}
