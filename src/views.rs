use crate::{
    model::{category, Record, CATEGORIES},
    presentation::{capacity, humanize},
    theme::*,
    Explorer, Inventory, Snapshot,
};
use eframe::egui::{self, RichText};

impl Explorer {
    fn navigate(&mut self, category: usize) {
        self.category = category;
        self.browsing = true;
        self.query.clear();
        self.selected = None;
        self.tab = 0;
    }
    fn open_dump(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open SMBIOS binary dump")
            .pick_file()
        {
            self.load(ctx.clone(), move || {
                Snapshot::decode(Inventory::from_dump(path).map_err(|e| e.to_string())?)
            });
        }
    }
    fn authenticate(&mut self, ctx: &egui::Context) {
        self.load(ctx.clone(), || {
            let output = std::process::Command::new("pkexec")
                .arg(std::env::current_exe().map_err(|e| e.to_string())?)
                .arg("--read-system")
                .output()
                .map_err(|e| format!("Cannot start the authentication service: {e}"))?;
            if !output.status.success() {
                return Err(format!(
                    "Reading was cancelled or could not complete. {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
        });
    }
    fn ssh_dialog(&mut self, ctx: &egui::Context) {
        if !self.ssh_open {
            return;
        }
        let mut open = true;
        let mut connect = false;
        let mut close = false;
        egui::Window::new("Connect over SSH")
            .open(&mut open).collapsible(false).resizable(false).default_width(470.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Read a Linux server's hardware inventory directly over SSH.");
                muted(ui, "No remote decoder, installation, or dump file required.");
                ui.add_space(8.0);
                ui.label("Host or SSH alias");
                ui.add(egui::TextEdit::singleline(&mut self.ssh_target).hint_text("admin@server  or  production-host").desired_width(f32::INFINITY));
                ui.label("Port (optional)");
                ui.add(egui::TextEdit::singleline(&mut self.ssh_port).hint_text("Use SSH configuration").desired_width(190.0));
                ui.checkbox(&mut self.ssh_sudo, "Use sudo to read firmware on the remote host");
                muted(ui, "Uses your local OpenSSH configuration, keys, and agent. Connect once in a terminal to verify an unfamiliar host's key.");
                if self.ssh_sudo { muted(ui, "Sudo must allow the firmware read without a password. Interactive password prompts are not supported."); }
                if let Some(error) = &self.ssh_error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    connect = ui.add_enabled(self.pending.is_none(), egui::Button::new("Connect and read")).clicked();
                    close = ui.button("Cancel").clicked();
                });
            });
        if !open || close {
            self.ssh_open = false;
        }
        if connect {
            match crate::ssh::Connection::new(&self.ssh_target, &self.ssh_port, self.ssh_sudo) {
                Ok(connection) => {
                    self.connect_ssh(ctx.clone(), connection);
                    self.ssh_open = false;
                }
                Err(error) => self.ssh_error = Some(error),
            }
        }
    }
    pub(super) fn ui(&mut self, ctx: &egui::Context) {
        self.poll();
        self.ssh_dialog(ctx);
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(SIDE)
                    .inner_margin(egui::Margin::symmetric(22, 14)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("DMI Explorer").size(19.0).strong());
                    ui.add_space(22.0);
                    let search = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .id_source("global-search")
                            .hint_text("Search hardware…")
                            .desired_width((ui.available_width() - 400.0).clamp(160.0, 320.0))
                            .margin(egui::vec2(12.0, 9.0)),
                    );
                    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
                        search.request_focus();
                    }
                    if search.changed() && !self.query.is_empty() {
                        self.browsing = true;
                        self.category = 0;
                    }
                    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                        self.query.clear();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(self.snapshot.is_some(), egui::Button::new("Export…"))
                            .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name("smbios.json")
                                .save_file()
                            {
                                match std::fs::write(&path, &self.snapshot.as_ref().unwrap().json) {
                                    Ok(()) => self.notice = "Inventory exported".into(),
                                    Err(e) => self.error = Some(e.to_string()),
                                }
                            }
                        }
                        ui.add_enabled_ui(self.pending.is_none(), |ui| {
                            if ui.button("Open dump…").clicked() {
                                self.open_dump(ctx);
                            }
                            ui.menu_button("Read system", |ui| {
                                if ui.button("Connect over SSH…").clicked() {
                                    self.ssh_open = true;
                                    self.ssh_error = None;
                                    ui.close();
                                }
                                ui.separator();
                                if ui.button("Read local firmware").clicked() {
                                    self.load(ctx.clone(), Snapshot::live);
                                    ui.close();
                                }
                                #[cfg(target_os = "linux")]
                                if ui.button("Read with administrator access…").clicked() {
                                    self.authenticate(ctx);
                                    ui.close();
                                }
                            });
                        });
                    });
                });
            });
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(SIDE)
                    .inner_margin(egui::Margin::symmetric(22, 9)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.pending.is_some() {
                        ui.spinner();
                        muted(ui, &self.loading_label);
                        if let Some(cancel) = &self.ssh_cancel {
                            if ui.small_button("Cancel").clicked() {
                                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                                self.loading_label = "Cancelling SSH read…".into();
                            }
                        }
                    } else if let Some(s) = &self.snapshot {
                        ui.label(RichText::new("•").color(GREEN));
                        ui.add(
                            egui::Label::new(
                                RichText::new(if s.source.starts_with("SSH:") {
                                    &s.source
                                } else {
                                    "Snapshot loaded"
                                })
                                .size(12.0)
                                .color(MUTED),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&s.source);
                        ui.label(
                            RichText::new(format!("·  {} records", s.records.len()))
                                .size(12.0)
                                .color(MUTED),
                        );
                    } else {
                        muted(ui, "No inventory loaded");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(if self.notice.is_empty() {
                                "Ctrl+F  Search    ·    Right-click a value to copy"
                            } else {
                                &self.notice
                            })
                            .size(12.0)
                            .color(MUTED),
                        );
                    });
                });
            });
        egui::SidePanel::left("navigation")
            .exact_width(202.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(SIDE).inner_margin(14))
            .show(ctx, |ui| {
                ui.add_space(12.0);
                if nav(ui, "Overview", !self.browsing, None, 0).clicked() {
                    self.browsing = false;
                    self.query.clear();
                }
                ui.add_space(24.0);
                eyebrow(ui, "COMPONENTS");
                ui.add_space(4.0);
                for (i, label) in CATEGORIES.iter().enumerate().skip(1).take(6) {
                    let count = self.snapshot.as_ref().map_or(0, |s| {
                        s.records.iter().filter(|r| category(r.kind) == i).count()
                    });
                    let label = match i {
                        2 => "Firmware",
                        4 => "Processors",
                        6 => "Expansion",
                        _ => label,
                    };
                    if nav(
                        ui,
                        label,
                        self.browsing && self.category == i,
                        Some(count),
                        i,
                    )
                    .clicked()
                    {
                        self.navigate(i);
                    }
                }
                ui.add_space(24.0);
                eyebrow(ui, "INSPECTOR");
                ui.add_space(4.0);
                if nav(
                    ui,
                    "All records",
                    self.browsing && self.category == 0,
                    None,
                    7,
                )
                .clicked()
                {
                    self.navigate(0);
                }
                if nav(
                    ui,
                    "Other & OEM",
                    self.browsing && self.category == 7,
                    None,
                    7,
                )
                .clicked()
                {
                    self.navigate(7);
                }
            });
        let snapshot = self.snapshot.take();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(28)).show(ctx, |ui| {
            if let Some(error) = self.error.clone() {
                egui::Frame::new().fill(egui::Color32::from_rgb(53,39,37)).corner_radius(8).inner_margin(16).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| { ui.strong("The inventory could not be loaded"); if ui.small_button("Dismiss").clicked() { self.error = None; } });
                    if snapshot.is_some() { muted(ui, "Your previous snapshot is still available below."); }
                    ui.collapsing("Details", |ui| { ui.label(&error); });
                });
                ui.add_space(16.0);
            }
            let Some(s) = &snapshot else {
                ui.add_space(60.0);
                card().show(ui, |ui| {
                    ui.set_width((ui.available_width() - 40.0).min(630.0));
                    eyebrow(ui, "HARDWARE INVENTORY");
                    ui.heading("Get to know your machine");
                    muted(ui, "Inspect your system, processors, firmware, and memory in one place.");
                    ui.add_space(16.0);
                    muted(ui, "Reading firmware may require administrator access. You can also open a saved SMBIOS dump.");
                    ui.add_space(12.0);
                    ui.add_enabled_ui(self.pending.is_none(), |ui| {
                        ui.horizontal(|ui| {
                            #[cfg(target_os = "linux")]
                            if ui.button("Read with administrator access…").clicked() { self.authenticate(ctx); }
                            #[cfg(not(target_os = "linux"))]
                            if ui.button("Read system").clicked() { self.load(ctx.clone(), Snapshot::live); }
                            if ui.button("Open dump…").clicked() { self.open_dump(ctx); }
                            if ui.button("Connect over SSH…").clicked() { self.ssh_open = true; self.ssh_error = None; }
                        });
                    });
                });
                return;
            };
            if !self.browsing { egui::ScrollArea::vertical().id_salt("overview").show(ui, |ui| self.overview(ui, s)); }
            else { self.browser(ui, s); }
        });
        self.snapshot = snapshot;
    }
    fn overview(&mut self, ui: &mut egui::Ui, s: &Snapshot) {
        let system = s.records.iter().find(|r| r.kind == 1);
        eyebrow(ui, "SYSTEM OVERVIEW");
        ui.heading(system.map_or("Hardware inventory", Record::name));
        muted(
            ui,
            format!(
                "{}  ·  Configuration reported by firmware",
                system.map_or("System", |r| r.value("manufacturer"))
            ),
        );
        ui.add_space(16.0);
        let cpu = s.records.iter().find(|r| r.kind == 4);
        let bios = s.records.iter().find(|r| r.kind == 0);
        let devices: Vec<_> = s.records.iter().filter(|r| r.kind == 17).collect();
        let known: u64 = devices.iter().filter_map(|r| r.capacity_kib).sum();
        let unknown = devices.iter().filter(|r| r.capacity_kib.is_none()).count();
        ui.columns(3, |cols| {
            let metrics = [
                (
                    "PROCESSOR",
                    cpu.map_or("Not reported".to_owned(), |r| r.name().to_owned()),
                    cpu.map_or("No processor information".to_owned(), |r| {
                        format!(
                            "{} cores · {} threads",
                            r.value("core_count"),
                            r.value("thread_count")
                        )
                    }),
                    4,
                ),
                (
                    "INSTALLED MEMORY",
                    if devices.is_empty() {
                        "Not reported".into()
                    } else if unknown > 0 {
                        format!("{} known", capacity(known))
                    } else {
                        capacity(known)
                    },
                    format!(
                        "{} occupied · {} empty · {} unknown",
                        devices
                            .iter()
                            .filter(|r| r.capacity_kib.is_some_and(|n| n > 0))
                            .count(),
                        devices.iter().filter(|r| r.capacity_kib == Some(0)).count(),
                        unknown
                    ),
                    5,
                ),
                (
                    "FIRMWARE",
                    bios.map_or("Not reported", |r| r.value("version"))
                        .to_owned(),
                    bios.map_or("No BIOS information".to_owned(), |r| {
                        r.value("vendor").to_owned()
                    }),
                    2,
                ),
            ];
            for (col, (label, value, sub, destination)) in cols.iter_mut().zip(metrics) {
                card().show(col, |ui| {
                    ui.set_min_height(144.0);
                    ui.set_width(ui.available_width());
                    eyebrow(ui, label);
                    ui.add_space(6.0);
                    ui.label(RichText::new(value).size(20.0).strong());
                    muted(ui, sub);
                    ui.add_space(6.0);
                    if ui.link("View details").clicked() {
                        self.navigate(destination);
                    }
                });
            }
        });
        ui.add_space(16.0);
        ui.columns(2, |cols| {
            card().show(&mut cols[0], |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(150.0);
                ui.label(RichText::new("System identity").size(18.0).strong());
                ui.add_space(8.0);
                if let Some(r) = system {
                    facts(
                        ui,
                        [
                            ("Manufacturer", r.value("manufacturer")),
                            ("Product", r.value("product_name")),
                            ("Family", r.value("family")),
                            ("Serial number", r.value("serial_number")),
                        ],
                    );
                } else {
                    muted(ui, "System identity was not reported.");
                }
            });
            card().show(&mut cols[1], |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(150.0);
                ui.label(RichText::new("Motherboard & firmware").size(18.0).strong());
                ui.add_space(8.0);
                let board = s.records.iter().find(|r| r.kind == 2);
                facts(
                    ui,
                    [
                        ("Motherboard", board.map_or("Not reported", Record::name)),
                        (
                            "Manufacturer",
                            board.map_or("Not reported", |r| r.value("manufacturer")),
                        ),
                        (
                            "BIOS version",
                            bios.map_or("Not reported", |r| r.value("version")),
                        ),
                        (
                            "Release date",
                            bios.map_or("Not reported", |r| r.value("release_date")),
                        ),
                    ],
                );
            });
        });
        ui.add_space(16.0);
        self.memory_slots(ui, s);
    }
    fn memory_slots(&mut self, ui: &mut egui::Ui, s: &Snapshot) {
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Memory slots").size(18.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.browsing && ui.link("Inspect memory").clicked() {
                        self.navigate(5);
                    }
                });
            });
            muted(ui, "Slot names and capacities reported by firmware");
            ui.add_space(8.0);
            let devices: Vec<_> = s
                .records
                .iter()
                .enumerate()
                .filter(|(_, r)| r.kind == 17)
                .collect();
            if devices.is_empty() {
                muted(ui, "No memory-device records were reported.");
                return;
            }
            let columns = if ui.available_width() >= 760.0 {
                4
            } else if ui.available_width() >= 360.0 {
                2
            } else {
                1
            };
            for row in devices.chunks(columns) {
                ui.columns(columns, |cols| {
                    for (col, &(index, r)) in cols.iter_mut().zip(row) {
                        let color = if r.capacity_kib == Some(0) {
                            MUTED
                        } else if r.capacity_kib.is_some() {
                            GREEN
                        } else {
                            ACCENT
                        };
                        egui::Frame::new()
                            .fill(SIDE)
                            .stroke(egui::Stroke::new(1.0_f32, LINE))
                            .corner_radius(6)
                            .inner_margin(14)
                            .show(col, |ui| {
                                ui.set_width(ui.available_width());
                                ui.set_min_height(96.0);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 4.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 2, color);
                                if ui.link(r.name()).clicked() {
                                    self.navigate(5);
                                    self.selected = Some(index);
                                }
                                ui.label(
                                    RichText::new(
                                        r.memory.as_deref().unwrap_or("Capacity unknown"),
                                    )
                                    .size(19.0)
                                    .color(color),
                                );
                                muted(
                                    ui,
                                    if r.capacity_kib == Some(0) {
                                        "Available"
                                    } else {
                                        r.value("memory_type")
                                    },
                                );
                            });
                    }
                });
                ui.add_space(6.0);
            }
        });
    }
    fn browser(&mut self, ui: &mut egui::Ui, s: &Snapshot) {
        let query = self.query.to_lowercase();
        let visible: Vec<_> = s
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                (self.category == 0 || category(r.kind) == self.category)
                    && r.search.contains(&query)
            })
            .map(|(i, _)| i)
            .collect();
        if !self.selected.is_some_and(|i| visible.contains(&i)) {
            self.selected = visible.first().copied();
            self.tab = 0;
        }
        eyebrow(
            ui,
            if self.category == 0 {
                "RECORD INSPECTOR"
            } else {
                "HARDWARE COMPONENTS"
            },
        );
        ui.heading(if self.query.is_empty() {
            CATEGORIES[self.category]
        } else {
            "Search results"
        });
        ui.horizontal(|ui| {
            muted(
                ui,
                format!(
                    "{} records{}",
                    visible.len(),
                    if self.query.is_empty() {
                        " reported by firmware"
                    } else {
                        " matching your search"
                    }
                ),
            );
            if !self.query.is_empty() && ui.small_button("Clear search").clicked() {
                self.query.clear();
            }
        });
        ui.add_space(14.0);
        if visible.is_empty() {
            card().show(ui, |ui| {
                ui.strong("No matching hardware");
                muted(ui, "Try another search or choose a different component.");
            });
            return;
        }
        // The record picker is only present when there is a choice to make.
        if visible.len() > 1 && matches!(self.category, 0 | 7) && ui.available_width() > 800.0 {
            egui::SidePanel::left("record-list")
                .exact_width(245.0)
                .resizable(false)
                .frame(egui::Frame::new().inner_margin(egui::Margin {
                    right: 16,
                    ..Default::default()
                }))
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("record-list-scroll")
                        .show(ui, |ui| {
                            for &i in &visible {
                                let r = &s.records[i];
                                let label =
                                    format!("{}\n{} · 0x{:04X}", r.name(), r.title, r.handle);
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 64.0],
                                        egui::Button::selectable(self.selected == Some(i), label)
                                            .wrap(),
                                    )
                                    .clicked()
                                {
                                    self.selected = Some(i);
                                    self.tab = 0;
                                }
                            }
                        });
                });
        } else if visible.len() > 1 {
            egui::ComboBox::from_id_salt("record-picker")
                .width(ui.available_width().min(620.0))
                .selected_text(self.selected.map_or("Select a record".into(), |i| {
                    format!(
                        "{}  ·  {}  ·  0x{:04X}",
                        s.records[i].name(),
                        s.records[i].title,
                        s.records[i].handle
                    )
                }))
                .show_ui(ui, |ui| {
                    for &i in &visible {
                        let r = &s.records[i];
                        if ui
                            .selectable_value(
                                &mut self.selected,
                                Some(i),
                                format!("{} · {} · 0x{:04X}", r.name(), r.title, r.handle),
                            )
                            .changed()
                        {
                            self.tab = 0;
                        }
                    }
                });
            ui.add_space(10.0);
        }
        let Some(index) = self.selected else {
            return;
        };
        let r = &s.records[index];
        egui::ScrollArea::vertical()
            .id_salt(("record", index, self.tab))
            .show(ui, |ui| {
                if self.category == 5 && self.query.is_empty() {
                    self.memory_slots(ui, s);
                    ui.add_space(16.0);
                }
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(r.name()).size(22.0).strong());
                        if let Some(memory) = &r.memory {
                            badge(
                                ui,
                                memory,
                                if r.capacity_kib == Some(0) {
                                    MUTED
                                } else {
                                    GREEN
                                },
                            );
                        }
                    });
                    muted(ui, r.subtitle());
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut self.tab, 0, "Summary");
                        ui.selectable_value(&mut self.tab, 1, "All properties");
                        ui.selectable_value(&mut self.tab, 2, "Raw record");
                        if ui.button("Copy summary").clicked() {
                            ui.ctx().copy_text(format!(
                                "{}\n{}",
                                r.name(),
                                r.values
                                    .iter()
                                    .map(|(k, v)| format!("{}: {v}", humanize(k)))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            ));
                            self.notice = "Summary copied".into();
                        }
                    });
                });
                ui.add_space(16.0);
                match self.tab {
                    0 => {
                        let groups = r.groups();
                        if groups.is_empty() {
                            property_card(ui, r);
                        } else {
                            for (name, rows) in groups {
                                card().show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(RichText::new(name).size(17.0).strong());
                                    ui.add_space(8.0);
                                    facts(ui, rows);
                                });
                                ui.add_space(14.0);
                            }
                        }
                    }
                    1 => property_card(ui, r),
                    _ => {
                        card().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            eyebrow(ui, "SMBIOS RECORD");
                            muted(ui, format!("Type {} · Handle 0x{:04X}", r.kind, r.handle));
                            if ui.button("Copy JSON").clicked() {
                                ui.ctx().copy_text(r.json.clone());
                                self.notice = "Record JSON copied".into();
                            }
                            egui::CollapsingHeader::new("Decoded JSON")
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(&r.json).monospace())
                                            .wrap()
                                            .selectable(true),
                                    );
                                });
                            ui.collapsing("Bytes & strings", |ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&r.raw).monospace())
                                        .wrap()
                                        .selectable(true),
                                );
                            });
                            if let Some(oem) = &r.oem {
                                ui.collapsing("Vendor-specific decode", |ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(oem).monospace())
                                            .wrap()
                                            .selectable(true),
                                    );
                                });
                            }
                        });
                    }
                }
            });
    }
}
fn property_card(ui: &mut egui::Ui, r: &Record) {
    card().show(ui, |ui| {
        ui.set_width(ui.available_width()); ui.label(RichText::new("Reported properties").size(17.0).strong());
        muted(ui,"Unreported values are omitted. The complete decoded record is available under Raw record."); ui.add_space(12.0);
        facts(ui,r.values.iter().map(|(k,v)|(humanize(k),v.as_str())));
        if r.values.is_empty() { muted(ui,"This record has no displayable properties."); }
    });
}
