use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    mem::{size_of, zeroed},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{
        Diagnostics::{
            Debug::{ReadProcessMemory, WriteProcessMemory},
            ToolHelp::*,
        },
        Threading::*,
    },
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Position {
    pub area: i32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub angle: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map: Option<u32>,
}
impl Position {
    pub fn valid(self) -> bool {
        [self.x, self.y, self.z, self.angle]
            .iter()
            .all(|v| v.is_finite())
            && self.map.is_none_or(valid_map)
    }
    pub fn coordinates(self) -> String {
        format!("X {:.2}   Y {:.2}   Z {:.2}", self.x, self.y, self.z)
    }
    pub fn distance(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2) + (self.z - other.z).powi(2))
            .sqrt()
    }
}
pub fn valid_map(map: u32) -> bool {
    [
        0x0a000000, 0x0a010000, 0x0a020000, 0x0b000000, 0x0c000000, 0x0c010000, 0x0d000000,
        0x0d010000, 0x0d020000, 0x0e000000, 0x0e010000, 0x0f000000, 0x0f010000, 0x10000000,
        0x11000000, 0x12000000, 0x12010000,
    ]
    .contains(&map)
}
pub fn map_from_area(area: i32) -> Option<u32> {
    if area <= 0 {
        return None;
    }
    let map = ((area as u32 / 10000) << 24) | (((area as u32 / 1000) % 10) << 16);
    valid_map(map).then_some(map)
}
pub fn map_name(map: u32) -> String {
    format!(
        "m{:02}_{:02}_{:02}_{:02}",
        map >> 24,
        (map >> 16) & 255,
        (map >> 8) & 255,
        map & 255
    )
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub struct Game {
    handle: Handle,
    pub pid: u32,
    pub base: usize,
    pub path: String,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Snapshot {
    pub position: Position,
    pub health: i32,
    pub max_health: i32,
    pub control: usize,
}
fn last_error(action: &str) -> String {
    format!("{action}: {}", std::io::Error::last_os_error())
}
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn from_wide(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|x| *x == 0).unwrap_or(s.len())])
}
impl Game {
    pub fn connect() -> Result<Self, String> {
        unsafe {
            let processes = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if processes == INVALID_HANDLE_VALUE {
                return Err(last_error("Could not list running games"));
            }
            let processes = Handle(processes);
            let mut entry: PROCESSENTRY32W = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(processes.0, &mut entry);
            let mut ids = Vec::new();
            while ok != 0 {
                if from_wide(&entry.szExeFile).eq_ignore_ascii_case("DarkSoulsRemastered.exe") {
                    ids.push(entry.th32ProcessID)
                }
                ok = Process32NextW(processes.0, &mut entry);
            }
            if ids.len() != 1 {
                return Err(if ids.is_empty() {
                    "Open Dark Souls Remastered and load your character."
                } else {
                    "More than one game is running. Close the extra copy."
                }
                .into());
            }
            let pid = ids[0];
            let modules = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
            if modules == INVALID_HANDLE_VALUE {
                return Err(last_error("The game is still starting"));
            }
            let modules = Handle(modules);
            let mut module: MODULEENTRY32W = zeroed();
            module.dwSize = size_of::<MODULEENTRY32W>() as u32;
            if Module32FirstW(modules.0, &mut module) == 0 {
                return Err(last_error("The game module is unavailable"));
            }
            let path = from_wide(&module.szExePath);
            let base = module.modBaseAddr as usize;
            let mut file = File::open(&path).map_err(|e| e.to_string())?;
            let mut sha = Sha256::new();
            let mut buf = [0u8; 65536];
            loop {
                let n = file.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                sha.update(&buf[..n]);
            }
            let digest = format!("{:x}", sha.finalize());
            if digest != "a45aaa36dd2f6cc151670a639ea5547043cf38ea79ff4178b963c6ed71f98d7b" {
                crate::support::log(format!(
                    "Unsupported game build: path={path} sha256={digest}"
                ));
                return Err(
                    "This game build is not supported. Save the support log from Diagnostics."
                        .into(),
                );
            }
            let handle = OpenProcess(
                PROCESS_QUERY_INFORMATION
                    | PROCESS_VM_READ
                    | PROCESS_VM_WRITE
                    | PROCESS_VM_OPERATION,
                0,
                pid,
            );
            if handle.is_null() {
                return Err(last_error("Windows blocked access to the game. Match the game's administrator permissions"));
            }
            let game = Self {
                handle: Handle(handle),
                pid,
                base,
                path,
            };
            let code = game.read(base + 0x7c4626, 14)?;
            if code[..3] != [0x48, 0x8b, 0x05]
                || code[7..] != [0x48, 0x8b, 0x48, 0x68, 0x48, 0x85, 0xc9]
            {
                return Err("The game code does not match the supported build.".into());
            }
            crate::support::log(format!(
                "Attached: pid={} base={:#x} path={} sha256={digest}",
                pid, base, game.path
            ));
            Ok(game)
        }
    }
    pub fn alive(&self) -> bool {
        let mut code = 0;
        unsafe { GetExitCodeProcess(self.handle.0, &mut code) != 0 && code == 259 }
    }
    fn read(&self, address: usize, size: usize) -> Result<Vec<u8>, String> {
        let mut data = vec![0u8; size];
        let mut count = 0;
        if unsafe {
            ReadProcessMemory(
                self.handle.0,
                address as _,
                data.as_mut_ptr() as _,
                size,
                &mut count,
            )
        } == 0
            || count != size
        {
            return Err(last_error(&format!(
                "Player data unavailable (read {address:#x}, {size} bytes)"
            )));
        }
        Ok(data)
    }
    fn write(&self, address: usize, data: &[u8]) -> Result<(), String> {
        let mut count = 0;
        if unsafe {
            WriteProcessMemory(
                self.handle.0,
                address as _,
                data.as_ptr() as _,
                data.len(),
                &mut count,
            )
        } == 0
            || count != data.len()
        {
            return Err(last_error(&format!(
                "The game rejected a position update (write {address:#x}, {} bytes)",
                data.len()
            )));
        }
        Ok(())
    }
    fn ptr(&self, address: usize) -> Result<usize, String> {
        let pointer = u64::from_le_bytes(self.read(address, 8)?.try_into().unwrap()) as usize;
        if !(65536..=0x7fff_ffff_ffff).contains(&pointer) {
            return Err(
                "The character is not ready yet. Wait for the loading screen to finish.".into(),
            );
        }
        Ok(pointer)
    }
    fn int(&self, address: usize) -> Result<i32, String> {
        Ok(i32::from_le_bytes(
            self.read(address, 4)?.try_into().unwrap(),
        ))
    }
    fn byte(&self, address: usize) -> Result<u8, String> {
        Ok(self.read(address, 1)?[0])
    }
    pub fn current_map(&self) -> Result<u32, String> {
        let manager = self.ptr(self.base + 0x1c7a058)?;
        let index = self.int(manager + 0x30)?;
        let array = self.ptr(manager + 0x18)?;
        let count = self.int(array + 0x18)?;
        if index < 0 || index >= count || count > 100 {
            return Err("The destination map is still loading.".into());
        }
        let data = self.ptr(array + 0x20)?;
        let map = self.int(data + index as usize * 0x70 + 8)? as u32;
        if !valid_map(map) {
            return Err("The current map could not be identified.".into());
        }
        Ok(map)
    }
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        let world = self.ptr(self.base + 0x1c77e50)?;
        let player = self.ptr(world + 0x68)?;
        let control = self.ptr(player + 0x68)?;
        let pose = self.ptr(control + 0x28)?;
        let data = self.read(pose, 28)?;
        let f = |i| f32::from_le_bytes(data[i..i + 4].try_into().unwrap());
        let area = self.int(player + 0x358)?;
        let map = self.current_map()?;
        let position = Position {
            area,
            x: f(16),
            y: f(20),
            z: f(24),
            angle: f(4),
            map: Some(map),
        };
        let health = self.int(player + 0x3e8)?;
        let max_health = self.int(player + 0x3ec)?;
        if !position.valid()
            || health <= 0
            || max_health < health
            || max_health > 100000
            || self.ptr(self.base + 0x1c77e50)? != world
            || self.ptr(world + 0x68)? != player
            || self.ptr(player + 0x68)? != control
            || self.ptr(control + 0x28)? != pose
            || self.current_map()? != map
        {
            return Err(
                "The character is not ready yet. Wait for the loading screen to finish.".into(),
            );
        }
        Ok(Snapshot {
            position,
            health,
            max_health,
            control,
        })
    }
    pub fn travel(&self, mut point: Position) -> Result<(), String> {
        if !point.valid() {
            return Err("Enter valid coordinates.".into());
        }
        if point.map.is_none() {
            point.map = map_from_area(point.area);
        }
        let before = self.snapshot()?;
        if point.map.is_some() && point.map != before.position.map {
            return self.cross_map(point);
        }
        self.local_travel(point)
    }
    fn local_travel(&self, point: Position) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let control = snapshot.control;
        if self.byte(control + 0x108)? != 0 {
            return Err("A teleport is already in progress.".into());
        }
        let mut payload = [0u8; 32];
        for (offset, value) in [
            (0, point.x),
            (4, point.y),
            (8, point.z),
            (12, 1.0),
            (20, point.angle % std::f32::consts::TAU),
        ] {
            payload[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        self.write(control + 0x110, &payload)?;
        if self.snapshot()?.control != control {
            return Err("The character changed. Try again.".into());
        }
        self.write(control + 0x108, &[1])?;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(3) {
            thread::sleep(Duration::from_millis(40));
            let after = self.snapshot()?;
            if after.control != control {
                break;
            }
            if self.byte(control + 0x108)? == 0 && after.position.distance(point) <= 0.5 {
                return Ok(());
            }
        }
        Err("The game did not confirm the teleport.".into())
    }
    fn cross_map(&self, point: Position) -> Result<(), String> {
        let target = point
            .map
            .ok_or("This location has no map saved. Capture it again in its original area.")?;
        let initial = self.snapshot()?;
        let manager = self.ptr(self.base + 0x1c74e08)?;
        if self.byte(manager + 0x19)? != 0
            || self.int(manager + 0x1c)? != -1
            || self.byte(manager + 0xb30)? != 0
            || self.byte(manager + 0xb32)? != 0
        {
            return Err("Another area transition is in progress. Wait for it to finish.".into());
        }
        let previous_stable = self.read(manager + 0xba0, 0x36)?;
        let previous_request = self.read(manager + 0x1c, 8)?;
        let mut pose = [0u8; 32];
        for (offset, value) in [
            (0, point.x),
            (4, point.y),
            (8, point.z),
            (12, 1.0),
            (20, point.angle % std::f32::consts::TAU),
        ] {
            pose[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        // BD5 stops the gameplay loop from replacing BA0/BB0 and BD0 with the
        // outgoing player's stable position. The loader clears it in 2c9b50.
        // Ghidra: 24f6b0 -> 2ca5a0 gates 2ca4b0/2ca4e0; 2514f0 consumes the pose.
        let staged = (|| {
            self.write(manager + 0xbd5, &[1])?;
            // Let an already-running stable-position update finish before staging.
            thread::sleep(Duration::from_millis(40));
            let mut stable = previous_stable.clone();
            stable[..32].copy_from_slice(&pose);
            stable[0x20] = 1;
            stable[0x30..0x34].copy_from_slice(&target.to_le_bytes());
            stable[0x34] = 1;
            stable[0x35] = 1;
            self.write(manager + 0xba0, &stable)?;
            let mut request = [0xffu8; 8];
            request[..4].copy_from_slice(&target.to_le_bytes());
            self.write(manager + 0x1c, &request)?;
            if self.snapshot()?.control != initial.control
                || self.ptr(self.base + 0x1c74e08)? != manager
                || self.byte(manager + 0xbd5)? != 1
                || self.read(manager + 0xba0, 32)? != pose
            {
                return Err("The character changed before the map request was sent.".to_owned());
            }
            self.write(manager + 0x19, &[1])
        })();
        if let Err(error) = staged {
            let _ = self.write(manager + 0x1c, &previous_request);
            let _ = self.write(manager + 0xba0, &previous_stable);
            return Err(error);
        }
        crate::support::log(format!(
            "Stage request: {} -> {} from={:?} to={point:?} manager={manager:#x}",
            map_name(initial.position.map.unwrap()),
            map_name(target),
            initial.position
        ));
        let start = Instant::now();
        let mut stable = 0;
        let mut last_trace = Instant::now() - Duration::from_secs(2);
        while start.elapsed() < Duration::from_secs(60) {
            thread::sleep(Duration::from_millis(100));
            if !self.alive() {
                return Err("Dark Souls Remastered closed while loading the destination.".into());
            }
            if let Ok(after) = self.snapshot() {
                if after.position.map == Some(target)
                    && point.y - after.position.y > 4.0
                    && self.byte(manager + 0x19).unwrap_or(1) == 0
                    && self.byte(manager + 0xbd4).unwrap_or(1) == 0
                {
                    crate::support::log(format!(
                        "Destination ground missing: target={point:?} actual={:?}",
                        after.position
                    ));
                    return Err("The destination ground did not finish loading.".into());
                }
                if last_trace.elapsed() >= Duration::from_secs(1) {
                    crate::support::log(format!(
                        "Stage progress: elapsed={} map={:?} position={:?} distance={} request={} override={} guard={}",
                        start.elapsed().as_millis(), after.position.map, after.position,
                        after.position.distance(point), self.byte(manager + 0x19).unwrap_or(255),
                        self.byte(manager + 0xbd4).unwrap_or(255), self.byte(manager + 0xbd5).unwrap_or(255)
                    ));
                    last_trace = Instant::now();
                }
                if after.position.map == Some(target)
                    && after.position.distance(point) <= 1.0
                    && self.byte(manager + 0x19).unwrap_or(1) == 0
                    && self.byte(manager + 0xbd4).unwrap_or(1) == 0
                {
                    stable += 1;
                    if stable >= 12 {
                        self.local_travel(point)?;
                        let confirmed = self.snapshot()?;
                        if confirmed.position.map == Some(target)
                            && confirmed.position.distance(point) <= 0.75
                        {
                            crate::support::log(format!(
                                "Stage travel confirmed after {} ms: {confirmed:?}",
                                start.elapsed().as_millis()
                            ));
                            return Ok(());
                        }
                    }
                } else {
                    stable = 0;
                }
            }
        }
        Err("The destination did not finish loading. Save the support log from Diagnostics.".into())
    }
}
