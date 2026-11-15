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

/// Entry point on iPhone/iPad: the Xcode project's `main()` calls this.
/// It starts the app and never returns (the system owns the run loop).
#[cfg(target_os = "ios")]
#[no_mangle]
pub extern "C" fn sawasdee_ios_main() {
    // Write problems into Documents/sawasdee.log: an iPhone app has no terminal, and a
    // crash during start-up would otherwise leave no trace.
    let log_path = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Documents").join("sawasdee.log"));
    if let Some(path) = log_path.clone() {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(&path, "start\n");
        std::panic::set_hook(Box::new(move |info| {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(&path) {
                let _ = writeln!(f, "PANIC: {info}");
            }
        }));
    }
    static LOGGER: FileLogger = FileLogger;
    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(log::LevelFilter::Warn));
    let mut options = eframe::NativeOptions::default();
    // Ask the graphics chip for exactly what it offers. eframe's default asks for a few
    // limits (e.g. 16 shader variables) that some chips, such as the Simulator's, do not
    // have, and then refuses to start.
    let mut gpu = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    gpu.device_descriptor = std::sync::Arc::new(|adapter| eframe::wgpu::DeviceDescriptor {
        label: Some("sawasdee device"),
        required_limits: adapter.limits(),
        ..Default::default()
    });
    options.wgpu_options.wgpu_setup = eframe::egui_wgpu::WgpuSetup::CreateNew(gpu);
    // Where progress is saved (the app's private Documents folder).
    if let Some(home) = std::env::var_os("HOME") {
        let dir = PathBuf::from(home).join("Documents");
        let _ = std::fs::create_dir_all(&dir);
        options.persistence_path = Some(dir.join("sawasdee_progress.ron"));
    }
    let result = eframe::run_native(
        "sawasdee-thai",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, None, None)))),
    );
    dlog(&format!("run_native returned: {:?}", result.as_ref().map(|_| "Ok")));
    if let Err(e) = result {
        eprintln!("sawasdee: the app stopped: {e}");
        if let Some(path) = log_path {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(path) {
                let _ = writeln!(f, "ERROR: {e}");
            }
        }
    }
}

/// Appends one line to Documents/sawasdee.log on iPhone (no-op elsewhere): start-up trace.
pub fn dlog(msg: &str) {
    #[cfg(target_os = "ios")]
    if let Some(h) = std::env::var_os("HOME") {
        use std::io::Write;
        let p = PathBuf::from(h).join("Documents").join("sawasdee.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(p) {
            let _ = writeln!(f, "{msg}");
        }
    }
    #[cfg(not(target_os = "ios"))]
    let _ = msg;
}

/// Sends the `log` messages of eframe/wgpu/winit into the same start-up trace (iPhone only).
#[cfg(target_os = "ios")]
struct FileLogger;

#[cfg(target_os = "ios")]
impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            dlog(&format!("{} {}: {}", r.level(), r.target(), r.args()));
        }
    }
    fn flush(&self) {}
}

#[cfg(target_os = "ios")]
extern "C" {
    fn sawasdee_safe_insets(top: *mut f32, bottom: *mut f32);
}

/// (top, bottom) space in points that the iPhone's notch and home bar take. (0, 0) elsewhere.
pub fn safe_insets() -> (f32, f32) {
    #[cfg(target_os = "ios")]
    {
        let (mut t, mut b) = (0.0f32, 0.0f32);
        // SAFETY: plain function from ios/Sawasdee/main.m that writes two floats.
        unsafe { sawasdee_safe_insets(&mut t, &mut b) };
        (t, b)
    }
    #[cfg(not(target_os = "ios"))]
    (0.0, 0.0)
}
