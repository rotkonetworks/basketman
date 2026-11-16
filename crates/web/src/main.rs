//! Desktop and browser frontend. Same code for both: `cargo run -p
//! basketman-web` natively, `trunk serve` in this directory for wasm.

mod agenda;
mod app;
mod arena;
mod board;
mod calendar;
mod club_ui;
mod coach;
mod fame_ui;
mod film;
mod finances;
mod history;
mod inbox;
mod lineup;
mod market;
mod matchview;
mod nav;
mod pages;
mod perf;
mod persist;
mod profile;
#[path = "view3d/scenes.rs"]
mod scenes;
mod screens;
mod search;
mod shell;
mod side;
mod signature_ui;
mod squadroom;
mod tactic_room;
mod tboard;
mod tcards;
mod tscreen;
mod theme;
mod title;
mod view3d;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let mut viewport = eframe::egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]);
    // A fixed-size window is left floating by tiling window managers, so
    // screenshots come out at the size asked for.
    // `SHOT_HEIGHT` makes it taller, to get a whole page in one picture.
    if std::env::args().any(|a| a == "--screenshot") {
        let height = std::env::var("SHOT_HEIGHT")
            .ok()
            .and_then(|h| h.parse().ok())
            .unwrap_or(900.0);
        // `SHOT_WIDTH` narrows it, to see the layouts for small windows.
        let width = std::env::var("SHOT_WIDTH")
            .ok()
            .and_then(|w| w.parse().ok())
            .unwrap_or(1440.0);
        viewport = viewport
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height]);
    }
    eframe::run_native(
        "Basketman",
        eframe::NativeOptions {
            viewport,
            // The career is saved; where the window sits is not.
            persist_window: false,
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    wasm_bindgen_futures::spawn_local(async {
        let canvas = eframe::web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("app"))
            .expect("missing #app element")
            .dyn_into::<eframe::web_sys::HtmlCanvasElement>()
            .expect("#app is not a canvas");
        eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
            )
            .await
            .expect("failed to start eframe");
    });
}
