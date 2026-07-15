//! Golf Terrain Studio — wgpu + egui, two viewers in one app:
//!
//! - **Explore**: one seed at a time over the 2 km × 2 km world. Parameters
//!   come from the seed via the atlas-fit sampler (or manual sliders for
//!   hand-tuning). Shows total relief, the high/low points, and the
//!   slope-grade histogram.
//! - **Match**: the seed-searching viewer. Runs the two-stage search of N
//!   seeds against every surveyed atlas course, then shows the best-matching
//!   seeds side-by-side with the real course heightmap.
//!
//! Run with `--release` — terrain rasters are CPU-generated.

mod atlas_tab;
mod explore;
mod hole_view;
mod match_tab;

use eframe::egui;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 940.0])
            .with_title("Golf Terrain Studio — 2 km × 2 km"),
        ..Default::default()
    };
    eframe::run_native(
        "golf-viewer",
        native_options,
        Box::new(|_cc| Ok(Box::new(ViewerApp::new()))),
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Explore,
    Match,
    Hole,
    Atlas,
}

struct ViewerApp {
    tab: Tab,
    explore: explore::ExploreTab,
    matcher: match_tab::MatchTab,
    hole: hole_view::HoleViewTab,
    atlas: atlas_tab::AtlasTab,
}

impl ViewerApp {
    fn new() -> Self {
        ViewerApp {
            tab: Tab::Explore,
            explore: explore::ExploreTab::new(),
            matcher: match_tab::MatchTab::new(),
            hole: hole_view::HoleViewTab::new(),
            atlas: atlas_tab::AtlasTab::new(),
        }
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.strong("Golf Terrain Studio");
                ui.separator();
                ui.selectable_value(&mut self.tab, Tab::Explore, "Explore seeds");
                ui.selectable_value(&mut self.tab, Tab::Match, "Match courses");
                ui.selectable_value(&mut self.tab, Tab::Hole, "Hole viewer");
                ui.selectable_value(&mut self.tab, Tab::Atlas, "Parkland Atlas");
            });
            ui.add_space(2.0);
        });
        match self.tab {
            Tab::Explore => self.explore.ui(ctx),
            Tab::Match => self.matcher.ui(ctx),
            Tab::Hole => self.hole.ui(ctx, self.explore.seed()),
            Tab::Atlas => self.atlas.ui(ctx),
        }
    }
}

pub(crate) fn to_color_image(img: &image::RgbaImage) -> egui::ColorImage {
    let (w, h) = img.dimensions();
    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw())
}
