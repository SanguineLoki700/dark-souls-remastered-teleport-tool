#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod game;
mod storage;
mod support;
mod theme;
mod ui;
pub const VERSION: &str = "2.3.4";
fn main() {
    support::log(format!("Started {VERSION}"));
    std::panic::set_hook(Box::new(|info| {
        support::log(format!(
            "PANIC: {info}\n{}",
            std::backtrace::Backtrace::force_capture()
        ))
    }));
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|x| x == "--dialog-test") {
        if let Err(e) = ui::dialog_test() {
            support::log(e);
            std::process::exit(1)
        }
        return;
    }
    if args.get(1).is_some_and(|x| x == "--render-hover-test") {
        let path = args
            .get(2)
            .map(std::path::PathBuf::from)
            .unwrap_or(support::folder().join("hover-preview.png"));
        if let Err(e) = ui::render_hover_test(&path) {
            support::log(e);
            std::process::exit(1)
        }
        return;
    }
    if args.get(1).is_some_and(|x| x == "--render-test") {
        let path = args
            .get(2)
            .map(std::path::PathBuf::from)
            .unwrap_or(support::folder().join("preview.png"));
        if let Err(e) = ui::render_test(&path) {
            support::log(e);
            std::process::exit(1)
        }
        return;
    }
    if args.get(1).is_some_and(|x| x == "--map-test") {
        let path = args
            .get(2)
            .map(std::path::PathBuf::from)
            .expect("Report path");
        let target_path = args
            .get(3)
            .map(std::path::PathBuf::from)
            .expect("Targets path");
        let result = (|| -> Result<serde_json::Value, String> {
            let game = game::Game::connect()?;
            let origin = game.snapshot()?;
            let targets: Vec<game::Position> = serde_json::from_reader(
                std::fs::File::open(target_path).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let mut arrivals = Vec::new();
            let mut problem = None;
            for target in targets {
                if let Err(e) = game.travel(target) {
                    problem = Some(e);
                    break;
                }
                arrivals.push(game.snapshot()?);
            }
            let returned = game.travel(origin.position).and_then(|_| game.snapshot());
            Ok(
                serde_json::json!({"Origin":origin,"Arrivals":arrivals,"Error":problem,"Returned":returned.as_ref().ok(),"ReturnError":returned.as_ref().err()}),
            )
        })();
        let text = match result {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap(),
            Err(e) => e,
        };
        let _ = std::fs::write(path, text);
        return;
    }
    if args.get(1).is_some_and(|x| x == "--probe") {
        let result = game::Game::connect()
            .and_then(|g| g.snapshot())
            .and_then(|x| serde_json::to_string_pretty(&x).map_err(|e| e.to_string()));
        let path = args
            .get(2)
            .map(std::path::PathBuf::from)
            .unwrap_or(support::folder().join("probe.json"));
        let _ = std::fs::write(path, result.unwrap_or_else(|e| e));
        return;
    }
    if args.get(1).is_some_and(|x| x == "--self-test") {
        let dir = args
            .get(2)
            .map(std::path::PathBuf::from)
            .unwrap_or(support::folder().join("checks"));
        if let Err(e) = app::self_test(&dir) {
            support::log(&e);
            let _ = std::fs::write(dir.join("failure.txt"), e);
            std::process::exit(1)
        }
        return;
    }
    if let Err(e) = ui::run() {
        support::log(&e);
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                game::wide(&e).as_ptr(),
                game::wide("Dark Souls Teleport").as_ptr(),
                0,
            );
        }
    }
}
