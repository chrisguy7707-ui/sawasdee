pub mod app;
pub mod audio;
pub mod data;
pub mod kit;
pub mod lesson;

use std::path::PathBuf;

use eframe::egui;

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// The desktop program (also used for the `--screenshot` check).
pub fn desktop_main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--list-speech") {
        for t in lesson::all_speech() {
            println!("{t}");
        }
        return Ok(());
    }
    if let Some(text) = arg_value(&args, "--speak-test") {
        // Plays one bundled recording and reports whether it was found.
        let found = audio::has_clip(&text);
        println!("recording for {text:?}: {}", if found { "found" } else { "missing" });
        let mut a = audio::Audio::new(false);
        a.speak(&text, false);
        std::thread::sleep(std::time::Duration::from_millis(2500));
        return Ok(());
    }
    if args.iter().any(|a| a == "--selftest") {
        match lesson::selftest() {
            Ok(msg) => {
                println!("selftest ok: {msg}");
                return Ok(());
            }
            Err(e) => {
                eprintln!("selftest failed: {e}");
                std::process::exit(1);
            }
        }
    }
    let shot = arg_value(&args, "--screenshot").map(PathBuf::from);
    let open = arg_value(&args, "--open");

    // `--size 390x780` opens a phone-sized window (to try the phone layout).
    let size = arg_value(&args, "--size").and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("사왓디 — 한국인을 위한 태국어")
        .with_inner_size(size.unwrap_or([430.0, 880.0]))
        .with_min_inner_size([320.0, 520.0]);

    let mut options = eframe::NativeOptions::default();
    if shot.is_some() {
        // Open quietly: don't take focus from what the person is doing.
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
        #[cfg(target_os = "macos")]
        {
            options.event_loop_builder = Some(Box::new(|builder| {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                builder.with_activate_ignoring_other_apps(false);
            }));
        }
        options.persist_window = false;
    }
    options.viewport = viewport;

    eframe::run_native(
        "sawasdee-thai",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, shot, open)))),
    )
}

/// Entry point on Android: the system starts the app through this function.
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android_app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("sawasdee"));
    log::info!("android_main started");
    let mut options = eframe::NativeOptions::default();
    // Where progress is saved (the app's private folder).
    if let Some(dir) = android_app.internal_data_path() {
        options.persistence_path = Some(dir.join("sawasdee_progress.ron"));
    }
    options.android_app = Some(android_app);
    let _ = eframe::run_native(
        "sawasdee-thai",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, None, None)))),
    );
}
