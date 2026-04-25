use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui::{
    Align2, CentralPanel, Color32, Context, FontId, Id, LayerId, Order, RichText, ScrollArea,
    SidePanel, TopBottomPanel, Ui,
};
use egui_plot::{Line, Plot, PlotPoints};
use walkers::{lon_lat, HttpTiles, Map, MapMemory};

use crate::fit_data::{ActivityData, Highlights, Lap, RawMessage};
use crate::gps::GpsPlugin;
use crate::map::{LayerSource, MapLayer};

#[derive(PartialEq, Clone, Copy, Default)]
enum Tab {
    #[default]
    Overview,
    Map,
    Stats,
    Raw,
}

#[derive(Clone, Copy, PartialEq, Default)]
enum Viewing {
    #[default]
    None,
    File(usize),
    Merged,
}

#[derive(Default)]
struct Precomputed {
    use_distance_x: bool,
    hr_pts: Vec<[f64; 2]>,
    elev_pts: Vec<[f64; 2]>,
    speed_pts: Vec<[f64; 2]>,
    power_pts: Vec<[f64; 2]>,
    cadence_pts: Vec<[f64; 2]>,
    resp_pts: Vec<[f64; 2]>,
    gps_positions: Vec<walkers::Position>,
    gps_center: Option<walkers::Position>,
    has_hr: bool,
    has_power: bool,
    has_cadence: bool,
    has_resp: bool,
    has_gps: bool,
}

impl Precomputed {
    fn build(act: &ActivityData) -> Self {
        let mut p = Self::default();

        p.use_distance_x = act
            .records
            .last()
            .map(|r| r.distance_m > 1.0)
            .unwrap_or(false);

        let mut lat_sum = 0.0f64;
        let mut lat_min = f64::MAX;
        let mut lat_max = f64::MIN;
        let mut lon_min = f64::MAX;
        let mut lon_max = f64::MIN;
        let mut gps_count = 0usize;

        for rec in &act.records {
            let x = if p.use_distance_x {
                rec.distance_m / 1000.0
            } else {
                rec.elapsed_s / 60.0
            };

            if let Some(hr) = rec.heart_rate {
                p.hr_pts.push([x, hr as f64]);
                p.has_hr = true;
            }
            if let Some(alt) = rec.altitude_m {
                p.elev_pts.push([x, alt]);
            }
            if let Some(spd) = rec.speed_ms {
                p.speed_pts.push([x, spd * 3.6]);
            }
            if let Some(pwr) = rec.power_w {
                if pwr > 0 {
                    p.power_pts.push([x, pwr as f64]);
                    p.has_power = true;
                }
            }
            if let Some(cad) = rec.cadence {
                if cad > 0 {
                    p.cadence_pts.push([x, cad as f64]);
                    p.has_cadence = true;
                }
            }
            if let Some(rr) = rec.respiration_rate {
                if rr > 0.0 {
                    p.resp_pts.push([x, rr]);
                    p.has_resp = true;
                }
            }
            if let (Some(lat), Some(lon)) = (rec.lat, rec.lon) {
                if lat.is_finite() && lon.is_finite() && (lat.abs() > 0.001 || lon.abs() > 0.001) {
                    p.gps_positions.push(lon_lat(lon, lat));
                    lat_sum += lat;
                    lat_min = lat_min.min(lat);
                    lat_max = lat_max.max(lat);
                    lon_min = lon_min.min(lon);
                    lon_max = lon_max.max(lon);
                    gps_count += 1;
                    p.has_gps = true;
                }
            }
        }

        if gps_count > 0 {
            let center_lon = (lon_min + lon_max) / 2.0;
            let center_lat = lat_sum / gps_count as f64;
            p.gps_center = Some(lon_lat(center_lon, center_lat));
        }

        p
    }
}

pub struct FittyApp {
    loaded: Vec<ActivityData>,
    selected: Vec<bool>,
    viewing: Viewing,
    merged: Option<ActivityData>,

    tab: Tab,
    error: Option<String>,
    pre: Precomputed,

    tiles: HttpTiles,
    map_memory: MapMemory,

    dark_mode: bool,
    map_layer: MapLayer,
    tiles_layer: MapLayer,
    #[allow(dead_code)]
    pending_open: Arc<Mutex<Option<(Vec<u8>, String)>>>,
    info_filter: String,
}

impl FittyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let tiles = HttpTiles::new(LayerSource(MapLayer::default()), cc.egui_ctx.clone());
        Self {
            loaded: vec![],
            selected: vec![],
            viewing: Viewing::None,
            merged: None,
            tab: Tab::default(),
            error: None,
            pre: Precomputed::default(),
            tiles,
            map_memory: MapMemory::default(),
            dark_mode: true,
            map_layer: MapLayer::default(),
            tiles_layer: MapLayer::default(),
            pending_open: Arc::new(Mutex::new(None)),
            info_filter: String::new(),
        }
    }

    pub fn load_bytes(&mut self, bytes: &[u8], filename: String) {
        match crate::fit_data::parse_fit(bytes, filename) {
            Ok(act) => {
                self.loaded.push(act);
                self.selected.push(false);
                let idx = self.loaded.len() - 1;
                self.set_viewing(Viewing::File(idx));
                self.merged = None;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn set_viewing(&mut self, v: Viewing) {
        self.viewing = v;
        let act = match v {
            Viewing::File(i) => self.loaded.get(i),
            Viewing::Merged => self.merged.as_ref(),
            Viewing::None => None,
        };
        if let Some(act) = act {
            self.pre = Precomputed::build(act);
            if let Some(center) = self.pre.gps_center {
                self.map_memory.center_at(center);
                let _ = self.map_memory.set_zoom(13.0);
            }
        } else {
            self.pre = Precomputed::default();
        }
    }

    fn do_merge(&mut self) {
        let refs: Vec<&ActivityData> = self
            .loaded
            .iter()
            .zip(self.selected.iter())
            .filter(|(_, sel)| **sel)
            .map(|(act, _)| act)
            .collect();
        if refs.len() >= 2 {
            println!("Not implemented...")
            // self.merged = Some(merge_activities(&refs));
            // self.set_viewing(Viewing::Merged);
        }
    }

    fn current_activity(&self) -> Option<&ActivityData> {
        match self.viewing {
            Viewing::File(i) => self.loaded.get(i),
            Viewing::Merged => self.merged.as_ref(),
            Viewing::None => None,
        }
    }

    fn show_files_panel(&mut self, ui: &mut Ui) {
        ui.add_space(6.0);
        ui.label(RichText::new("Files").strong().size(15.0));
        ui.add_space(4.0);

        if self.loaded.is_empty() {
            ui.label(
                RichText::new("No files loaded.\nDrop .fit files to start.")
                    .color(Color32::GRAY)
                    .small(),
            );
            return;
        }

        let selected_count = self.selected.iter().filter(|&&s| s).count();
        let mut merge_now = false;
        let mut remove_idx: Option<usize> = None;
        let mut view_idx: Option<usize> = None;

        // Show merged row at top when it exists
        if self.merged.is_some() {
            let is_viewing = self.viewing == Viewing::Merged;
            let m_name = format!(
                "Merged ({} files)",
                self.selected.iter().filter(|&&s| s).count()
            );
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(is_viewing, RichText::new(m_name).italics())
                    .clicked()
                {
                    self.set_viewing(Viewing::Merged);
                }
            });
            ui.add_space(2.0);
        }

        for i in 0..self.loaded.len() {
            let is_viewing = self.viewing == Viewing::File(i);
            let dist_km = self.loaded[i].total_distance_m / 1000.0;
            let label = format!("{}  ({:.1} km)", self.loaded[i].filename, dist_km);

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.selected[i], "");
                if ui.selectable_label(is_viewing, &label).clicked() {
                    view_idx = Some(i);
                }
                if ui
                    .button("Delete")
                    .on_hover_text("Remove this file")
                    .clicked()
                {
                    remove_idx = Some(i);
                }
            });
        }

        if let Some(i) = view_idx {
            self.set_viewing(Viewing::File(i));
        }

        if let Some(i) = remove_idx {
            self.loaded.remove(i);
            self.selected.remove(i);
            self.merged = None;
            match self.viewing {
                Viewing::File(j) if j == i => {
                    let new = if self.loaded.is_empty() {
                        Viewing::None
                    } else {
                        Viewing::File(i.min(self.loaded.len() - 1))
                    };
                    self.set_viewing(new);
                }
                Viewing::File(j) if j > i => {
                    self.set_viewing(Viewing::File(j - 1));
                }
                Viewing::Merged => self.set_viewing(Viewing::None),
                _ => {}
            }
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let can_merge = selected_count >= 2;
            let btn_label = if selected_count >= 2 {
                format!("Merge {} files", selected_count)
            } else {
                "Merge".to_string()
            };
            if ui
                .add_enabled(can_merge, egui::Button::new(btn_label))
                .on_disabled_hover_text("Select 2 or more files to merge")
                .clicked()
            {
                merge_now = true;
            }
            if ui.small_button("Clear all").clicked() {
                self.loaded.clear();
                self.selected.clear();
                self.merged = None;
                self.set_viewing(Viewing::None);
            }
        });

        if merge_now {
            self.do_merge();
        }
    }
}

impl eframe::App for FittyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Collect result from async file-open dialog (WASM)
        #[cfg(target_arch = "wasm32")]
        {
            let pending = self.pending_open.lock().unwrap().take();
            if let Some((bytes, name)) = pending {
                self.load_bytes(&bytes, name);
            }
        }

        let dropped = ctx.input(|i| i.raw.dropped_files.first().cloned());
        if let Some(file) = dropped {
            let mut name = file.name.clone();

            if name.is_empty() {
                name = file.path.clone().unwrap().file_name().unwrap().to_ascii_lowercase().into_string().unwrap();
            }
            if let Some(bytes) = file.bytes {
                self.load_bytes(&bytes, name);
            } else {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(path) = file.path {
                    if let Ok(bytes) = std::fs::read(&path) {
                        self.load_bytes(&bytes, name);
                    }
                }
            }
        }

        #[cfg(feature = "dev")]
        if self.current_activity().is_none() {
            let test_file = "test_activity.fit";
            self.load_bytes(
                &std::fs::read(&test_file).unwrap(),
                test_file.parse().unwrap(),
            );
        }

        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        let act_header = self.current_activity().map(|act| {
            let label = match self.viewing {
                Viewing::Merged => format!(
                    "Merged ({} files)",
                    self.selected.iter().filter(|&&s| s).count()
                ),
                _ => act.filename.clone(),
            };
            (label, act.sport.clone(), act.clone())
        });

        TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("FIT File Analyzer");

                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("Open File").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("FIT", &["fit", "FIT"])
                        .pick_file()
                    {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if let Ok(bytes) = std::fs::read(&path) {
                            self.load_bytes(&bytes, name);
                        }
                    }
                }

                #[cfg(target_arch = "wasm32")]
                if ui.button("Open File").clicked() {
                    let pending = self.pending_open.clone();
                    let ctx2 = ctx.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Some(file) = rfd::AsyncFileDialog::new()
                            .add_filter("FIT", &["fit", "FIT"])
                            .pick_file()
                            .await
                        {
                            let bytes = file.read().await;
                            let name = file.file_name();
                            *pending.lock().unwrap() = Some((bytes, name));
                            ctx2.request_repaint();
                        }
                    });
                }

                if let Some((label, sport, _)) = &act_header {
                    ui.separator();
                    ui.label(RichText::new(label).strong());
                    if let Some(s) = sport {
                        ui.label(RichText::new(s).color(Color32::from_rgb(120, 180, 255)));
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let icon = if self.dark_mode { "☀" } else { "🌙" };
                    if ui.button(icon).on_hover_text("Toggle theme").clicked() {
                        self.dark_mode = !self.dark_mode;
                        ctx.set_visuals(if self.dark_mode {
                            egui::Visuals::dark()
                        } else {
                            egui::Visuals::light()
                        });
                    }

                    if let Some((_, _, _act)) = &act_header {
                        if ui.button("Export FIT").clicked() {
                            println!("Not implemented...")
                        }
                    }
                });
            });
            ui.add_space(2.0);
        });

        if self.current_activity().is_some() {
            TopBottomPanel::top("tab_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Overview, "Overview");
                    ui.selectable_value(&mut self.tab, Tab::Map, "Map");
                    ui.selectable_value(&mut self.tab, Tab::Stats, "Stats");
                    ui.selectable_value(&mut self.tab, Tab::Raw, "Raw");
                });
            });
        }

        SidePanel::left("sidebar")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    self.show_files_panel(ui);

                    if self.current_activity().is_none() {
                        ui.set_width(240.0);
                    }

                    if self.current_activity().is_some() {
                        ui.separator();
                        if let Some(act) = self.current_activity() {
                            let act = act.clone();
                            show_summary(ui, &act);
                            if !act.laps.is_empty() {
                                ui.separator();
                                show_laps(ui, &act.laps);
                            }
                        }
                    }
                });
            });

        CentralPanel::default().show(ctx, |ui| {
            if self.current_activity().is_none() {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("Drop a .fit file to visualize your activity")
                            .size(20.0)
                            .color(Color32::from_gray(80)),
                    );
                });
                return;
            }

            let tab = self.tab;
            match tab {
                Tab::Map => {
                    let center = self.pre.gps_center.unwrap_or(lon_lat(0.0, 0.0));
                    let has_gps = self.pre.has_gps;
                    let points = self.pre.gps_positions.clone();

                    if !has_gps {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                RichText::new("No GPS data in this activity")
                                    .color(Color32::GRAY)
                                    .size(16.0),
                            );
                        });
                        return;
                    }

                    // Rebuild tiles when the selected layer changes.
                    if self.map_layer != self.tiles_layer {
                        self.tiles = HttpTiles::new(LayerSource(self.map_layer), ctx.clone());
                        self.tiles_layer = self.map_layer;
                    }

                    let map = Map::new(Some(&mut self.tiles), &mut self.map_memory, center)
                        .with_plugin(GpsPlugin {
                            points,
                            color: Color32::from_rgb(64, 160, 255),
                        });

                    ui.add(map);

                    let rect = ui.max_rect();
                    let mut btn_x = rect.right() - 8.0;
                    let btn_y = rect.top() + 8.0;
                    let is_dark = ui.visuals().dark_mode;
                    let layers = [MapLayer::Satellite, MapLayer::Topo, MapLayer::Streets];
                    for layer in layers {
                        let selected = self.map_layer == layer;
                        let (fill, text_color) = if selected {
                            (
                                Color32::from_rgba_premultiplied(40, 100, 200, 230),
                                Color32::WHITE,
                            )
                        } else if is_dark {
                            (
                                Color32::from_rgba_premultiplied(30, 30, 30, 210),
                                Color32::from_gray(210),
                            )
                        } else {
                            (
                                Color32::from_rgba_premultiplied(255, 255, 255, 230),
                                Color32::from_gray(20),
                            )
                        };
                        let text = RichText::new(layer.label()).small().color(text_color);
                        let btn = egui::Button::new(if selected { text.strong() } else { text })
                            .fill(fill);
                        let size = egui::vec2(64.0, 20.0);
                        btn_x -= size.x;
                        let btn_rect = egui::Rect::from_min_size(egui::pos2(btn_x, btn_y), size);
                        if ui.put(btn_rect, btn).clicked() {
                            self.map_layer = layer;
                        }
                        btn_x -= 4.0;
                    }

                    let attribution = match self.map_layer {
                        MapLayer::Streets => "© OpenStreetMap contributors",
                        MapLayer::Satellite => "© Esri, DigitalGlobe, GeoEye",
                        MapLayer::Topo => "© OpenTopoMap contributors",
                    };
                    ui.painter().text(
                        rect.right_bottom() - egui::vec2(4.0, 4.0),
                        Align2::RIGHT_BOTTOM,
                        attribution,
                        FontId::proportional(10.0),
                        Color32::from_rgba_premultiplied(200, 200, 200, 200),
                    );
                }
                Tab::Overview => {
                    show_charts(
                        ui,
                        &self.pre.hr_pts,
                        &self.pre.elev_pts,
                        &self.pre.speed_pts,
                        &self.pre.power_pts,
                        &self.pre.cadence_pts,
                        &self.pre.resp_pts,
                        self.pre.has_hr,
                        self.pre.has_power,
                        self.pre.has_cadence,
                        self.pre.has_resp,
                        self.pre.use_distance_x,
                    );
                }
                Tab::Stats => {
                    let act_data = self.current_activity().cloned();
                    if let Some(act) = act_data {
                        show_stats(ui, &act);
                    }
                }
                Tab::Raw => {
                    let info_data = self
                        .current_activity()
                        .map(|act| (act.highlights.clone(), act.info_messages.clone()));
                    if let Some((highlights, msgs)) = info_data {
                        show_info(ui, &highlights, &msgs, &mut self.info_filter);
                    }
                }
            }
        });

        if hovering {
            let painter =
                ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("drop_overlay")));
            let rect = painter.clip_rect();
            painter.rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(0, 0, 0, 170));
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Drop a .fit file",
                FontId::proportional(28.0),
                Color32::WHITE,
            );
        }

        if let Some(err) = self.error.clone() {
            egui::Window::new("Parsing or some other generic error")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(&err);
                    if ui.button("Close").clicked() {
                        self.error = None;
                    }
                });
        }
    }
}

fn show_summary(ui: &mut Ui, act: &ActivityData) {
    ui.add_space(6.0);
    ui.label(RichText::new("Summary").strong().size(15.0));
    ui.add_space(4.0);

    let total_secs = act.total_time_s as u64;
    let total_duration = calculate_duration(total_secs);

    let timer_secs = act.total_timer_time as u64;
    let timer_duration = calculate_duration(timer_secs);

    egui::Grid::new("summary")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            item(ui, "Elapsed time", &total_duration);
            item(ui, "Moving time", &timer_duration);
            item(
                ui,
                "Distance",
                &format!("{:.2} km", act.total_distance_m / 1000.0),
            );
            if act.total_ascent_m > 0 {
                item(ui, "Ascent", &format!("{} m", act.total_ascent_m));
            }
            if act.avg_hr_bpm > 0 {
                item(
                    ui,
                    "Avg / Max HR",
                    &format!("{} / {} bpm", act.avg_hr_bpm, act.max_hr_bpm),
                );
            }
            if act.avg_speed_ms > 0.0 {
                item(
                    ui,
                    "Avg Speed",
                    &format!("{:.1} km/h", act.avg_speed_ms * 3.6),
                );
                item(ui, "Avg Pace", &pace_str(act.avg_speed_ms));
            }
            if act.avg_power_w > 0 {
                item(
                    ui,
                    "Avg / Max Power",
                    &format!("{} / {} W", act.avg_power_w, act.max_power_w),
                );
            }
            if let Some(vo2) = act.highlights.vo2_max {
                item(ui, "VO₂ Max", &format!("{:.1} mL/kg/min", vo2));
            }
            item(ui, "Records", &format!("{}", act.records.len()));
        });
}

fn calculate_duration(seconds: u64) -> String {
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            (seconds % 3600) / 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

fn show_laps(ui: &mut Ui, laps: &[Lap]) {
    ui.add_space(6.0);
    ui.label(RichText::new("Laps").strong().size(15.0));
    ui.add_space(4.0);

    for lap in laps {
        let secs = lap.time_s as u64;
        let header = match lap.avg_speed_ms {
            Some(spd) => format!(
                "Lap {}  {:02}:{:02}  {:.2} km  {}",
                lap.index + 1,
                secs / 60,
                secs % 60,
                lap.distance_m / 1000.0,
                pace_str(spd)
            ),
            None => format!(
                "Lap {}  {:02}:{:02}  {:.2} km",
                lap.index + 1,
                secs / 60,
                secs % 60,
                lap.distance_m / 1000.0
            ),
        };

        egui::CollapsingHeader::new(header)
            .id_salt(lap.index)
            .show(ui, |ui| {
                egui::Grid::new(format!("lap_{}", lap.index))
                    .num_columns(2)
                    .spacing([8.0, 2.0])
                    .show(ui, |ui| {
                        item(
                            ui,
                            "Distance",
                            &format!("{:.2} km", lap.distance_m / 1000.0),
                        );
                        if let Some(hr) = lap.avg_hr {
                            item(ui, "Avg HR", &format!("{} bpm", hr));
                        }
                        if let Some(spd) = lap.avg_speed_ms {
                            item(ui, "Speed", &format!("{:.1} km/h", spd * 3.6));
                            item(ui, "Pace", &pace_str(spd));
                        }
                        if let Some(asc) = lap.ascent_m {
                            if asc > 0 {
                                item(ui, "Ascent", &format!("{} m", asc));
                            }
                        }
                        if let Some(pwr) = lap.avg_power_w {
                            if pwr > 0 {
                                item(ui, "Avg Power", &format!("{} W", pwr));
                            }
                        }
                    });
            });
    }
}

fn item(ui: &mut Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).color(Color32::from_gray(150)).small());
    ui.label(RichText::new(value).strong());
    ui.end_row();
}

/// Format speed in m/s as a pace string "M:SS /km".
fn pace_str(speed_ms: f64) -> String {
    if speed_ms <= 0.0 {
        return "—".to_string();
    }
    let secs_per_km = 1000.0 / speed_ms;
    let mins = (secs_per_km / 60.0) as u64;
    let secs = (secs_per_km % 60.0) as u64;
    format!("{}:{:02} /km", mins, secs)
}

fn show_stats(ui: &mut Ui, act: &ActivityData) {
    ScrollArea::vertical().show(ui, |ui| {
        let secs = act.total_time_s as u64;
        let duration = if secs >= 3600 {
            format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
        } else {
            format!("{:02}:{:02}", secs / 60, secs % 60)
        };

        stats_section(ui, "Activity");
        stats_grid(ui, "stats_act", |ui| {
            stat_row(ui, "Duration", &duration);
            stat_row(
                ui,
                "Distance",
                &format!("{:.2} km", act.total_distance_m / 1000.0),
            );
            if act.total_ascent_m > 0 {
                stat_row(ui, "Elevation Gain", &format!("{} m", act.total_ascent_m));
            }
            if let Some(cal) = act.highlights.total_calories {
                stat_row(ui, "Calories", &format!("{} kcal", cal));
            }
            if let Some(cyc) = act.highlights.total_cycles {
                stat_row(ui, "Total Cycles", &format!("{}", cyc));
            }
            if let Some(sport) = &act.sport {
                stat_row(ui, "Sport", sport);
            }
        });

        let has_perf = act.avg_hr_bpm > 0 || act.avg_speed_ms > 0.0 || act.avg_power_w > 0;
        if has_perf {
            stats_section(ui, "Performance");
            stats_grid(ui, "stats_perf", |ui| {
                if act.avg_hr_bpm > 0 {
                    stat_row(
                        ui,
                        "Avg / Max HR",
                        &format!("{} / {} bpm", act.avg_hr_bpm, act.max_hr_bpm),
                    );
                }
                if act.avg_speed_ms > 0.0 {
                    stat_row(
                        ui,
                        "Avg Speed",
                        &format!("{:.1} km/h", act.avg_speed_ms * 3.6),
                    );
                    stat_row(ui, "Avg Pace", &pace_str(act.avg_speed_ms));
                }
                if act.avg_power_w > 0 {
                    stat_row(
                        ui,
                        "Avg / Max Power",
                        &format!("{} / {} W", act.avg_power_w, act.max_power_w),
                    );
                }
            });
        }

        let h = &act.highlights;
        let has_training =
            h.vo2_max.is_some() || h.aerobic_te.is_some() || h.training_stress_score.is_some();
        if has_training {
            stats_section(ui, "Training Effect");
            stats_grid(ui, "stats_train", |ui| {
                if let Some(v) = h.vo2_max {
                    stat_row(ui, "VO₂ Max", &format!("{:.1} mL/kg/min", v));
                }
                if let Some(v) = h.aerobic_te {
                    stat_row(ui, "Aerobic Effect", &format!("{:.1} / 5.0", v));
                }
                if let Some(v) = h.anaerobic_te {
                    stat_row(ui, "Anaerobic Effect", &format!("{:.1} / 5.0", v));
                }
                if let Some(v) = h.training_stress_score {
                    stat_row(ui, "Training Stress Score", &format!("{:.0}", v));
                }
                if let Some(v) = h.intensity_factor {
                    stat_row(ui, "Intensity Factor", &format!("{:.3}", v));
                }
            });
        }

        if !act.laps.is_empty() {
            stats_section(ui, &format!("Laps ({})", act.laps.len()));
            egui::Grid::new("stats_laps")
                .num_columns(6)
                .spacing([12.0, 4.0])
                .striped(true)
                .min_col_width(44.0)
                .show(ui, |ui| {
                    for hdr in &["#", "Time", "Dist", "Avg HR", "Speed", "Pace"] {
                        ui.label(
                            RichText::new(*hdr)
                                .strong()
                                .small()
                                .color(Color32::from_gray(150)),
                        );
                    }
                    ui.end_row();
                    for lap in &act.laps {
                        let s = lap.time_s as u64;
                        ui.label(format!("{}", lap.index + 1));
                        ui.label(format!("{:02}:{:02}", s / 60, s % 60));
                        ui.label(format!("{:.2} km", lap.distance_m / 1000.0));
                        ui.label(
                            lap.avg_hr
                                .map(|h| format!("{} bpm", h))
                                .unwrap_or_else(|| "—".into()),
                        );
                        ui.label(
                            lap.avg_speed_ms
                                .map(|v| format!("{:.1} km/h", v * 3.6))
                                .unwrap_or_else(|| "—".into()),
                        );
                        ui.label(
                            lap.avg_speed_ms
                                .map(|v| pace_str(v))
                                .unwrap_or_else(|| "—".into()),
                        );
                        ui.end_row();
                    }
                });
        }

        if h.device_name.is_some() || h.software_version.is_some() {
            stats_section(ui, "Device");
            stats_grid(ui, "stats_dev", |ui| {
                if let Some(name) = &h.device_name {
                    stat_row(ui, "Device", name);
                }
                if let Some(ver) = &h.software_version {
                    stat_row(ui, "SW Version", ver);
                }
            });
        }
    });
}

fn stats_section(ui: &mut Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(
        RichText::new(title)
            .strong()
            .size(13.0)
            .color(Color32::from_rgb(100, 160, 240)),
    );
    ui.add_space(2.0);
}

fn stats_grid(ui: &mut Ui, id: &str, add_contents: impl FnOnce(&mut Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, add_contents);
}

fn stat_row(ui: &mut Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).color(Color32::from_gray(150)).small());
    ui.label(RichText::new(value).strong());
    ui.end_row();
}

#[allow(clippy::too_many_arguments)]
fn show_charts(
    ui: &mut Ui,
    hr: &[[f64; 2]],
    elev: &[[f64; 2]],
    speed: &[[f64; 2]],
    power: &[[f64; 2]],
    cadence: &[[f64; 2]],
    resp: &[[f64; 2]],
    has_hr: bool,
    has_power: bool,
    has_cadence: bool,
    has_resp: bool,
    use_distance: bool,
) {
    let x_label = if use_distance {
        "Distance (km)"
    } else {
        "Time (min)"
    };

    let visible = 2
        + if has_hr { 1 } else { 0 }
        + if has_power { 1 } else { 0 }
        + if has_cadence { 1 } else { 0 }
        + if has_resp { 1 } else { 0 };
    let chart_h = (ui.available_height() / visible as f32)
        .max(90.0)
        .min(260.0);

    ScrollArea::vertical().show(ui, |ui| {
        if has_hr {
            chart(
                ui,
                "Heart Rate",
                "hr_chart",
                hr,
                Color32::RED,
                "bpm",
                x_label,
                chart_h,
            );
        }
        chart(
            ui,
            "Elevation",
            "elev_chart",
            elev,
            Color32::GREEN,
            "m",
            x_label,
            chart_h,
        );
        chart(
            ui,
            "Speed",
            "speed_chart",
            speed,
            Color32::BLUE,
            "km/h",
            x_label,
            chart_h,
        );
        if has_power {
            chart(
                ui,
                "Power",
                "power_chart",
                power,
                Color32::ORANGE,
                "W",
                x_label,
                chart_h,
            );
        }
        if has_cadence {
            chart(
                ui,
                "Cadence",
                "cad_chart",
                cadence,
                Color32::PURPLE,
                "rpm",
                x_label,
                chart_h,
            );
        }
        if has_resp {
            chart(
                ui,
                "Respiration Rate",
                "resp_chart",
                resp,
                Color32::CYAN,
                "brpm",
                x_label,
                chart_h,
            );
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn chart(
    ui: &mut Ui,
    title: &str,
    id: &str,
    data: &[[f64; 2]],
    color: Color32,
    y_label: &str,
    x_label: &str,
    height: f32,
) {
    ui.add_space(2.0);
    ui.label(RichText::new(title).strong());

    let line = Line::new("", PlotPoints::from(data.to_vec()))
        .color(color)
        .fill(0.0)
        .width(1.5);

    Plot::new(id)
        .height(height)
        .x_axis_label(x_label)
        .y_axis_label(y_label)
        .show(ui, |p| {
            p.line(line);
        });
}

fn show_info(ui: &mut Ui, highlights: &Highlights, msgs: &[RawMessage], filter: &mut String) {
    ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(4.0);
        ui.label(RichText::new("Highlights").strong().size(15.0));
        ui.add_space(4.0);

        let has_highlights = highlights.vo2_max.is_some()
            || highlights.aerobic_te.is_some()
            || highlights.anaerobic_te.is_some()
            || highlights.training_stress_score.is_some()
            || highlights.intensity_factor.is_some()
            || highlights.total_calories.is_some()
            || highlights.total_cycles.is_some()
            || highlights.device_name.is_some()
            || highlights.software_version.is_some();

        if has_highlights {
            egui::Grid::new("info_highlights")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    if let Some(v) = highlights.vo2_max {
                        item(ui, "VO₂ Max", &format!("{:.1} mL/kg/min", v));
                    }
                    if let Some(v) = highlights.aerobic_te {
                        item(ui, "Aerobic TE", &format!("{:.1} / 5.0", v));
                    }
                    if let Some(v) = highlights.anaerobic_te {
                        item(ui, "Anaerobic TE", &format!("{:.1} / 5.0", v));
                    }
                    if let Some(v) = highlights.training_stress_score {
                        item(ui, "Training Stress Score", &format!("{:.0}", v));
                    }
                    if let Some(v) = highlights.intensity_factor {
                        item(ui, "Intensity Factor", &format!("{:.3}", v));
                    }
                    if let Some(v) = highlights.total_calories {
                        item(ui, "Calories", &format!("{} kcal", v));
                    }
                    if let Some(v) = highlights.total_cycles {
                        item(ui, "Total Cycles", &format!("{}", v));
                    }
                    if let Some(name) = &highlights.device_name {
                        item(ui, "Device", name);
                    }
                    if let Some(ver) = &highlights.software_version {
                        item(ui, "SW Version", ver);
                    }
                });
        } else {
            ui.label(
                RichText::new("No highlight data found in this file.")
                    .color(Color32::from_gray(120))
                    .small(),
            );
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        ui.label(RichText::new("All Messages").strong().size(15.0));
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(filter);
            if ui.small_button("Clear").on_hover_text("Clear").clicked() {
                filter.clear();
            }
        });
        ui.add_space(4.0);

        if msgs.is_empty() {
            ui.label(
                RichText::new("No messages in this file.")
                    .color(Color32::from_gray(120))
                    .small(),
            );
            return;
        }

        let mut grouped: BTreeMap<&str, Vec<&RawMessage>> = BTreeMap::new();
        for msg in msgs {
            grouped.entry(msg.kind.as_str()).or_default().push(msg);
        }

        let filter_lc = filter.to_lowercase();

        for (kind, group) in &grouped {
            let matching: Vec<&RawMessage> = if filter_lc.is_empty() {
                group.iter().copied().collect()
            } else {
                group
                    .iter()
                    .copied()
                    .filter(|msg| {
                        kind.to_lowercase().contains(&filter_lc)
                            || msg.fields.iter().any(|f| {
                            f.name.to_lowercase().contains(&filter_lc)
                                || f.value.to_lowercase().contains(&filter_lc)
                        })
                    })
                    .collect()
            };

            if matching.is_empty() {
                continue;
            }

            let header = format!("{} ({})", kind, matching.len());
            egui::CollapsingHeader::new(header)
                .id_salt(*kind)
                .show(ui, |ui| {
                    for (idx, msg) in matching.iter().enumerate() {
                        if matching.len() > 1 {
                            ui.label(
                                RichText::new(format!("#{}", idx + 1))
                                    .color(Color32::from_gray(130))
                                    .small(),
                            );
                        }
                        egui::Grid::new(format!("{}_{}", kind, idx))
                            .num_columns(3)
                            .spacing([8.0, 2.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for field in &msg.fields {
                                    ui.label(
                                        RichText::new(&field.name)
                                            .color(Color32::from_gray(160))
                                            .small(),
                                    );
                                    ui.label(RichText::new(&field.value).strong().small());
                                    ui.label(
                                        RichText::new(field.units.as_str())
                                            .color(Color32::from_gray(120))
                                            .small(),
                                    );
                                    ui.end_row();
                                }
                            });
                        if idx + 1 < matching.len() {
                            ui.add_space(4.0);
                        }
                    }
                });
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn trigger_download(bytes: &[u8], filename: &str) {
    use wasm_bindgen::JsCast;

    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    let document = match window.document() {
        Some(d) => d,
        None => return,
    };

    // Build a Uint8Array from raw bytes and wrap it in a JS Array for Blob.
    let uint8 = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    uint8.copy_from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&uint8);

    let blob = match web_sys::Blob::new_with_u8_array_sequence(&parts) {
        Ok(b) => b,
        Err(_) => return,
    };
    let url = match web_sys::Url::create_object_url_with_blob(&blob) {
        Ok(u) => u,
        Err(_) => return,
    };

    // Create a temporary <a download="..."> and click it.
    if let Ok(el) = document.create_element("a") {
        if let Ok(anchor) = el.dyn_into::<web_sys::HtmlAnchorElement>() {
            anchor.set_href(&url);
            anchor.set_download(filename);
            if let Some(body) = document.body() {
                let _ = body.append_child(&anchor);
                anchor.click();
                let _ = body.remove_child(&anchor);
            }
        }
    }

    let _ = web_sys::Url::revoke_object_url(&url);
}
