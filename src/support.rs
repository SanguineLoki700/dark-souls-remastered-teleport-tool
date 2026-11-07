use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
static GATE: Mutex<()> = Mutex::new(());
static BASE: OnceLock<PathBuf> = OnceLock::new();
pub fn folder() -> PathBuf {
    BASE.get_or_init(|| {
        std::env::current_exe()
            .expect("Executable path unavailable")
            .parent()
            .unwrap()
            .to_owned()
    })
    .clone()
}
pub fn timestamp() -> String {
    unsafe {
        let mut t = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetSystemTime(&mut t);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
        )
    }
}
pub fn log(message: impl AsRef<str>) {
    let _guard = GATE.lock().unwrap_or_else(|e| e.into_inner());
    let dir = folder().join("logs");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("teleport.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() >= 2 * 1024 * 1024) {
        let _ = fs::remove_file(dir.join("teleport.2.log"));
        let _ = fs::rename(dir.join("teleport.1.log"), dir.join("teleport.2.log"));
        let _ = fs::rename(&path, dir.join("teleport.1.log"));
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} {}", timestamp(), message.as_ref());
    }
}
pub fn export(path: &std::path::Path, diagnostics: &str) -> Result<(), String> {
    let _guard = GATE.lock().unwrap_or_else(|e| e.into_inner());
    let mut text = format!(
        "Dark Souls Remastered Teleport {}\n{}\n\n{}\n\n",
        crate::VERSION,
        timestamp(),
        diagnostics
    );
    text.push_str(&format!(
        "Executable: {}\nArchitecture: x64\n\n",
        std::env::current_exe().unwrap_or_default().display()
    ));
    for file in ["teleport.2.log", "teleport.1.log", "teleport.log"] {
        if let Ok(data) = fs::read_to_string(folder().join("logs").join(file)) {
            text.push_str(&data);
        }
    }
    fs::write(path, text).map_err(|e| format!("Could not save the support log: {e}"))
}
pub fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
