use crate::{
    game::{Game, Position},
    storage::{Library, Location},
    support,
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};
pub enum Command {
    Capture,
    Return,
    TravelPosition(Position),
    Save(String),
    Travel(String),
    Edit(Location),
    Remove(String),
    Import(PathBuf),
    Export(PathBuf),
    ManualSave(String, Position),
    Reconnect,
    Filter(String),
    Page(usize),
    SupportLog(PathBuf),
    Stop,
}
#[derive(Clone)]
pub struct View {
    pub status: String,
    pub quick: String,
    pub current: Option<Position>,
    pub rows: Vec<Location>,
    pub total: usize,
    pub filtered: usize,
    pub page: usize,
    pub pages: usize,
    pub busy: bool,
    pub connected: bool,
    pub has_quick: bool,
    pub notice: String,
    pub diagnostics: String,
}
impl Default for View {
    fn default() -> Self {
        Self {
            status: "Connecting to Dark Souls…".into(),
            quick: "No quick checkpoint captured".into(),
            current: None,
            rows: vec![],
            total: 0,
            filtered: 0,
            page: 0,
            pages: 1,
            busy: false,
            connected: false,
            has_quick: false,
            notice: "F5: Capture checkpoint    F9: Return    Select a record to travel.".into(),
            diagnostics: "Connecting…".into(),
        }
    }
}
pub enum Event {
    Snapshot(View),
}
pub fn spawn() -> (Sender<Command>, Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let (events, out) = mpsc::channel();
    thread::spawn(move || worker(rx, events));
    (tx, out)
}
struct State {
    library: Library,
    game: Option<Game>,
    view: View,
    filtered: Vec<usize>,
    query: String,
    error: String,
    loaded: bool,
    last_attach: Instant,
}
impl State {
    fn filter(&mut self) {
        let q = self.query.to_lowercase();
        self.filtered = self
            .library
            .data
            .locations
            .iter()
            .enumerate()
            .filter(|(_, x)| q.is_empty() || x.name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        self.filtered
            .sort_by_cached_key(|i| self.library.data.locations[*i].name.to_lowercase());
        self.view.total = self.library.data.locations.len();
        self.view.filtered = self.filtered.len();
        self.view.pages = self.filtered.len().div_ceil(50).max(1);
        self.view.page = self.view.page.min(self.view.pages - 1);
        self.rows();
    }
    fn rows(&mut self) {
        self.view.rows = self
            .filtered
            .iter()
            .skip(self.view.page * 50)
            .take(50)
            .map(|i| self.library.data.locations[*i].clone())
            .collect();
    }
    fn diagnostics(&self) -> String {
        let status = if self.view.connected {
            "Connected to Dark Souls Remastered.".to_owned()
        } else {
            friendly(&self.view.status)
        };
        let error = if self.error.is_empty() {
            String::new()
        } else {
            format!("\r\n\r\nLast error\r\n{}", friendly(&self.error))
        };
        format!(
            "{status}\r\n\r\n{} saved {} loaded.{error}",
            self.view.total,
            if self.view.total == 1 {
                "location"
            } else {
                "locations"
            }
        )
    }
    fn position(&self) -> Result<Position, String> {
        self.game
            .as_ref()
            .ok_or("Open the game and load your character.")?
            .snapshot()
            .map(|x| x.position)
    }
    fn travel(&self, p: Position) -> Result<(), String> {
        support::log(format!("Travel requested: {p:?}"));
        self.game
            .as_ref()
            .ok_or("Open the game and load your character.")?
            .travel(p)?;
        support::log("Teleport confirmed");
        Ok(())
    }
    fn add(&mut self, name: String, position: Position) -> Result<(), String> {
        if !position.valid() {
            return Err("Enter valid coordinates.".into());
        }
        let item = Location {
            id: uuid::Uuid::new_v4().to_string(),
            name: self.library.name(&name, None),
            position,
            created: support::timestamp(),
        };
        self.library.data.locations.push(item);
        if let Err(e) = self.library.save() {
            self.library.data.locations.pop();
            return Err(e);
        }
        self.view.page = 0;
        self.query.clear();
        self.filter();
        support::log(format!(
            "Location saved: total={} visible={} file={}",
            self.view.total,
            self.view.rows.len(),
            self.library.path.display()
        ));
        Ok(())
    }
    fn command(&mut self, command: Command) -> Result<String, String> {
        match command {
            Command::Capture => {
                let position = self.position()?;
                let before = self.library.data.quick;
                self.library.data.quick = Some(position);
                if let Err(e) = self.library.save() {
                    self.library.data.quick = before;
                    return Err(e);
                }
                self.quick();
                Ok("Checkpoint captured. Press F9 to return.".into())
            }
            Command::TravelPosition(p) => {
                self.travel(p)?;
                Ok("Teleport complete.".into())
            }
            Command::Return => {
                self.travel(
                    self.library
                        .data
                        .quick
                        .ok_or("Capture a checkpoint first.")?,
                )?;
                Ok("Teleport complete.".into())
            }
            Command::Save(name) => {
                let p = self.position()?;
                self.add(name, p)?;
                Ok("Location saved.".into())
            }
            Command::ManualSave(name, p) => {
                self.add(name, p)?;
                Ok("Coordinates saved.".into())
            }
            Command::Travel(id) => {
                let p = self
                    .library
                    .data
                    .locations
                    .iter()
                    .find(|x| x.id == id)
                    .ok_or("Select a location to travel.")?
                    .position;
                self.travel(p)?;
                Ok("Teleport complete.".into())
            }
            Command::Edit(mut item) => {
                let i = self
                    .library
                    .data
                    .locations
                    .iter()
                    .position(|x| x.id == item.id)
                    .ok_or("Select a location to edit.")?;
                item.name = self.library.name(&item.name, Some(&item.id));
                let previous = std::mem::replace(&mut self.library.data.locations[i], item);
                if let Err(e) = self.library.save() {
                    self.library.data.locations[i] = previous;
                    return Err(e);
                }
                self.filter();
                Ok("Location updated.".into())
            }
            Command::Remove(id) => {
                let i = self
                    .library
                    .data
                    .locations
                    .iter()
                    .position(|x| x.id == id)
                    .ok_or("Select a location to remove.")?;
                let item = self.library.data.locations.remove(i);
                if let Err(e) = self.library.save() {
                    self.library.data.locations.insert(i, item);
                    return Err(e);
                }
                self.filter();
                Ok("Location removed.".into())
            }
            Command::Import(path) => {
                let n = self.library.import(&path)?;
                self.filter();
                Ok(format!("{n} locations imported."))
            }
            Command::Export(path) => {
                self.library.export(&path)?;
                Ok("Locations exported.".into())
            }
            Command::SupportLog(path) => {
                support::log(format!("Support snapshot: game_pid={:?} game_path={:?} current={:?} locations={} file={} last_error={}",self.game.as_ref().map(|g|g.pid),self.game.as_ref().map(|g|&g.path),self.view.current,self.view.total,self.library.path.display(),self.error));
                support::export(&path, &self.diagnostics())?;
                Ok("Support log saved.".into())
            }
            Command::Reconnect => {
                self.game = None;
                self.last_attach = Instant::now() - Duration::from_secs(10);
                self.poll();
                Ok("Reconnecting…".into())
            }
            Command::Filter(query) => {
                self.query = query;
                self.view.page = 0;
                self.filter();
                Ok("".into())
            }
            Command::Page(page) => {
                self.view.page = page.min(self.view.pages - 1);
                self.rows();
                Ok("".into())
            }
            Command::Stop => Ok("".into()),
        }
    }
    fn quick(&mut self) {
        self.view.has_quick = self.library.data.quick.is_some();
        self.view.quick = self
            .library
            .data
            .quick
            .map(|x| x.coordinates())
            .unwrap_or("No quick checkpoint captured".into());
    }
    fn poll(&mut self) {
        if self.game.as_ref().is_some_and(|g| !g.alive()) {
            self.game = None;
        }
        if self.game.is_none() {
            if self.last_attach.elapsed() < Duration::from_secs(4) {
                return;
            }
            self.last_attach = Instant::now();
            match Game::connect() {
                Ok(g) => self.game = Some(g),
                Err(e) => {
                    self.view.connected = false;
                    self.view.current = None;
                    self.view.status = e;
                    return;
                }
            }
        }
        match self.game.as_ref().unwrap().snapshot() {
            Ok(s) => {
                self.view.current = Some(s.position);
                self.view.connected = true;
                self.view.status = "●  Connected and ready".into()
            }
            Err(e) => {
                self.view.connected = false;
                self.view.current = None;
                self.view.status = e;
            }
        }
    }
}
fn worker(rx: Receiver<Command>, events: Sender<Event>) {
    let mut state = State {
        library: Library::new(support::folder().join("checkpoints.json")),
        game: None,
        view: View::default(),
        filtered: vec![],
        query: String::new(),
        error: String::new(),
        loaded: false,
        last_attach: Instant::now() - Duration::from_secs(10),
    };
    let _ = events.send(Event::Snapshot(state.view.clone()));
    match state.library.load() {
        Ok(()) => {
            state.loaded = true;
            state.filter();
            state.quick();
            support::log(format!(
                "Locations loaded: total={} file={}",
                state.view.total,
                state.library.path.display()
            ));
        }
        Err(e) => {
            support::log(format!("Load error: {e}"));
            state.error = e;
            state.view.notice = "The locations file could not be loaded. Open Diagnostics.".into();
        }
    }
    let mut last_poll = Instant::now() - Duration::from_secs(3);
    loop {
        if last_poll.elapsed() >= Duration::from_millis(600) {
            state.poll();
            last_poll = Instant::now();
            state.view.diagnostics = state.diagnostics();
            if events.send(Event::Snapshot(state.view.clone())).is_err() {
                break;
            }
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Ok(command) => {
                let safe_when_failed = matches!(
                    command,
                    Command::Reconnect
                        | Command::SupportLog(_)
                        | Command::Filter(_)
                        | Command::Page(_)
                );
                if !state.loaded && !safe_when_failed {
                    state.view.notice =
                        "Saved locations could not be loaded. Check Diagnostics before saving."
                            .into();
                } else {
                    state.view.busy = true;
                    let _ = events.send(Event::Snapshot(state.view.clone()));
                    match state.command(command) {
                        Ok(message) => {
                            if !message.is_empty() {
                                state.view.notice = message;
                            }
                        }
                        Err(e) => {
                            support::log(format!(
                                "Action failed: {e}\n{}",
                                std::backtrace::Backtrace::force_capture()
                            ));
                            state.error = e.clone();
                            state.view.notice =
                                "The action failed. Open Diagnostics for details.".into();
                        }
                    }
                    state.view.busy = false;
                }
                state.view.diagnostics = state.diagnostics();
                let _ = events.send(Event::Snapshot(state.view.clone()));
            }
        }
    }
}
pub fn self_test(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let clock = Instant::now();
    let point = Position {
        area: 102000,
        x: 1.5,
        y: 2.,
        z: 3.,
        angle: 0.5,
        map: Some(0x0a020000),
    };
    let mut library = Library::new(dir.join("checkpoints.json"));
    if library.name(" ", None) != "Location 1" {
        return Err("Optional name failed".into());
    }
    library.data.quick = Some(point);
    for i in 0..100000 {
        library.data.locations.push(Location {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("Location {i:06}"),
            position: point,
            created: support::timestamp(),
        });
    }
    library.save()?;
    let created = clock.elapsed().as_millis();
    let clock = Instant::now();
    library.load()?;
    let load = clock.elapsed().as_millis();
    if library.data.locations.len() != 100000 || library.data.quick.unwrap().map != point.map {
        return Err("Large library lost records".into());
    }
    library.export(&dir.join("export.json"))?;
    if library.import(&dir.join("export.json"))? != 0 {
        return Err("Import duplicated records".into());
    }
    let clock = Instant::now();
    let found: Vec<_> = library
        .data
        .locations
        .iter()
        .filter(|x| x.name.contains("099999"))
        .collect();
    if found.len() != 1 {
        return Err("Last record search failed".into());
    }
    let filter_ms = clock.elapsed().as_millis();
    support::export(&dir.join("support.log"), "Self-test")?;
    let mut state = State {
        library,
        game: None,
        view: View::default(),
        filtered: vec![],
        query: String::new(),
        error: String::new(),
        loaded: true,
        last_attach: Instant::now(),
    };
    let large_library = std::mem::replace(
        &mut state.library,
        Library::new(dir.join("save-three.json")),
    );
    for name in ["First", "Second", ""] {
        state.command(Command::ManualSave(name.into(), point))?;
    }
    if state.view.total != 3 || state.view.rows.len() != 3 {
        return Err("Saving a third location cleared the visible list.".into());
    }
    state.library.load()?;
    state.filter();
    if state.view.total != 3 || state.view.rows.len() != 3 {
        return Err("Reopening lost saved locations.".into());
    }
    state.library = large_library;
    state.filter();
    if state.view.rows.len() != 50 || state.view.pages != 2000 {
        return Err("Large-library pagination failed".into());
    }
    state.command(Command::Page(1999))?;
    if state.view.rows.last().map(|x| x.name.as_str()) != Some("Location 099999") {
        return Err("Last page lost records".into());
    }
    state.command(Command::Filter("099999".into()))?;
    if state.view.rows.len() != 1 || state.view.pages != 1 {
        return Err("Filtered page is incorrect".into());
    }
    let legacy = Position {
        area: 151000,
        map: None,
        ..point
    };
    let legacy_path = dir.join("legacy-version.json");
    let mut old = Library::new(legacy_path.clone());
    old.data.locations.push(Location {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Legacy location".into(),
        position: legacy,
        created: support::timestamp(),
    });
    old.save()?;
    old.load()?;
    if old.data.locations[0].position.map != Some(0x0f010000) {
        return Err("Legacy location map was not recovered".into());
    }
    let mut unnamed = Library::new(dir.join("unnamed.json"));
    unnamed.data.locations.push(Location {
        id: uuid::Uuid::new_v4().to_string(),
        name: unnamed.name("", None),
        position: point,
        created: support::timestamp(),
    });
    unnamed.save()?;
    unnamed.load()?;
    if unnamed.data.locations[0].name != "Location 1" {
        return Err("Blank name round trip failed".into());
    }
    std::fs::write(dir.join("result.txt"),format!("PASS\n100000 locations, save/load, import/export, duplicate handling, optional names, map persistence, last-record search and support log\nCreate: {created} ms\nLoad: {load} ms\nFind: {filter_ms} ms\n")).map_err(|e|e.to_string())?;
    Ok(())
}

fn friendly(error: &str) -> String {
    if error.contains("read 0x") || error.contains("not ready yet") {
        "The character is not ready yet. Wait for the loading screen to finish, then try Reconnect."
            .into()
    } else if error.contains("os error 5") {
        "Windows denied access. Check the folder permissions and match the game's administrator permissions.".into()
    } else {
        error.into()
    }
}
