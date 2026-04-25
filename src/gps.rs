use egui::{Color32, Stroke, Ui};
use walkers::{MapMemory, Plugin, Projector};

pub(crate) struct GpsPlugin {
    pub(crate) points: Vec<walkers::Position>,
    pub(crate) color: Color32,
}

impl Plugin for GpsPlugin {
    fn run(
        self: Box<Self>,
        ui: &mut Ui,
        _response: &egui::Response,
        projector: &Projector,
        _memory: &MapMemory,
    ) {
        if self.points.len() < 2 {
            return;
        }
        let painter = ui.painter();
        let pts: Vec<egui::Pos2> = self
            .points
            .iter()
            .map(|&p| projector.project(p).to_pos2())
            .collect();

        for pair in pts.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(3.0, self.color));
        }
        if let Some(&first) = pts.first() {
            painter.circle_filled(first, 6.0, Color32::from_rgb(80, 220, 80));
        }
        if let Some(&last) = pts.last() {
            painter.circle_filled(last, 6.0, Color32::from_rgb(220, 60, 60));
        }
    }
}