mod model;
mod presentation;
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
        println!("dmidecode-gui [DUMP_FILE]\nOpen an SMBIOS dump or read the local system.");
        return Ok(());
    }
    if args.len() > 1 {
        eprintln!("Usage: dmidecode-gui [DUMP_FILE]");
        std::process::exit(2);
    }
    let path = args.first().map(PathBuf::from);
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
                if let Some(path) = &path {
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
                cc.egui_ctx.request_repaint();
                return Ok(Box::new(app));
            }
            app.load(cc.egui_ctx.clone(), move || match path {
                Some(path) => {
                    Snapshot::decode(Inventory::from_dump(path).map_err(|e| e.to_string())?)
                }
                None => Snapshot::live(),
            });
            Ok(Box::new(app))
        }),
    )
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
    #[cfg(feature = "screenshots")]
    screenshot_frames: u32,
}
impl Explorer {
    fn load(
        &mut self,
        ctx: egui::Context,
        job: impl FnOnce() -> Result<Snapshot, String> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.error = None;
        self.notice.clear();
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
        let mut app = Explorer {
            snapshot: Some(
                Snapshot::decode(Inventory::from_bytes(
                    include_bytes!("../tests/fixtures/workstation.bin").to_vec(),
                    None,
                ))
                .unwrap(),
            ),
            ..Default::default()
        };
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
