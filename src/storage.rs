use crate::game::Position;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Location {
    pub id: String,
    pub name: String,
    pub position: Position,
    pub created: String,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Data {
    #[serde(default = "version")]
    pub version: u32,
    #[serde(default)]
    pub quick: Option<Position>,
    #[serde(default)]
    pub locations: Vec<Location>,
}
fn version() -> u32 {
    1
}
pub struct Library {
    pub data: Data,
    pub path: PathBuf,
}
impl Library {
    pub fn new(path: PathBuf) -> Self {
        Self {
            data: Data {
                version: 1,
                ..Default::default()
            },
            path,
        }
    }
    pub fn load(&mut self) -> Result<(), String> {
        if !self.path.exists() {
            return Ok(());
        }
        let mut result: Data = serde_json::from_reader(BufReader::new(
            File::open(&self.path).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| format!("The locations file could not be read: {e}"))?;
        validate(&result)?;
        for item in &mut result.locations {
            if item.position.map.is_none() {
                item.position.map = crate::game::map_from_area(item.position.area);
            }
        }
        if let Some(ref mut point) = result.quick {
            if point.map.is_none() {
                point.map = crate::game::map_from_area(point.area);
            }
        }
        self.data = result;
        Ok(())
    }
    pub fn name(&self, title: &str, except: Option<&str>) -> String {
        let names: HashSet<_> = self
            .data
            .locations
            .iter()
            .filter(|x| Some(x.id.as_str()) != except)
            .map(|x| x.name.to_lowercase())
            .collect();
        let title = if title.trim().is_empty() {
            format!("Location {}", self.data.locations.len() + 1)
        } else {
            title.trim().to_owned()
        };
        let mut result = title.clone();
        let mut suffix = 1;
        while names.contains(&result.to_lowercase()) {
            result = format!("{title} ({suffix})");
            suffix += 1;
        }
        result
    }
    pub fn save(&self) -> Result<(), String> {
        save(&self.path, &self.data)
    }
    pub fn export(&self, path: &Path) -> Result<(), String> {
        if path == self.path {
            return Err("Choose a different file for the export.".into());
        }
        save(path, &self.data)
    }
    pub fn import(&mut self, path: &Path) -> Result<usize, String> {
        let value: serde_json::Value =
            serde_json::from_reader(BufReader::new(File::open(path).map_err(|e| e.to_string())?))
                .map_err(|e| format!("This file is not valid JSON: {e}"))?;
        let incoming: Data = if value.get("Version").is_some() || value.get("Locations").is_some() {
            serde_json::from_value(value).map_err(|e| e.to_string())?
        } else {
            let dict = value
                .as_object()
                .ok_or("The file must contain saved locations.")?;
            let mut data = Data {
                version: 1,
                ..Default::default()
            };
            for (name, values) in dict {
                let v = values
                    .as_array()
                    .filter(|v| v.len() == 3)
                    .ok_or("Expected X, Y and Z coordinates.")?;
                let mut xyz = [0f32; 3];
                for i in 0..3 {
                    xyz[i] = v[i].as_f64().ok_or("Expected numeric coordinates.")? as f32;
                }
                data.locations.push(Location {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: name.clone(),
                    position: Position {
                        area: 0,
                        x: xyz[0],
                        y: xyz[1],
                        z: xyz[2],
                        angle: 0.,
                        map: None,
                    },
                    created: crate::support::timestamp(),
                });
            }
            data
        };
        validate(&incoming)?;
        let old_len = self.data.locations.len();
        let mut ids: HashSet<_> = self.data.locations.iter().map(|x| x.id.clone()).collect();
        let mut names: HashSet<_> = self
            .data
            .locations
            .iter()
            .map(|x| x.name.to_lowercase())
            .collect();
        let mut suffixes = HashMap::<String, u32>::new();
        for mut item in incoming.locations {
            if !ids.insert(item.id.clone()) {
                continue;
            }
            let base = item.name.clone();
            let key = base.to_lowercase();
            let suffix = suffixes.entry(key).or_default();
            while !names.insert(item.name.to_lowercase()) {
                *suffix += 1;
                item.name = format!("{base} ({suffix})");
            }
            self.data.locations.push(item);
        }
        if let Err(e) = self.save() {
            self.data.locations.truncate(old_len);
            return Err(e);
        }
        Ok(self.data.locations.len() - old_len)
    }
}
fn validate(data: &Data) -> Result<(), String> {
    let mut ids = HashSet::new();
    if data.version != 1
        || data.quick.is_some_and(|x| !x.valid())
        || data.locations.iter().any(|x| {
            x.name.trim().is_empty()
                || uuid::Uuid::parse_str(&x.id).is_err()
                || !x.position.valid()
                || !ids.insert(&x.id)
        })
    {
        return Err("This locations file is invalid or uses an unsupported format.".into());
    }
    Ok(())
}
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn save(path: &Path, data: &Data) -> Result<(), String> {
    validate(data)?;
    let parent = path.parent().ok_or("Invalid destination folder.")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = path.with_file_name(format!(
        "{}.{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        crate::support::unique_suffix()
    ));
    let result = (|| {
        let mut writer = BufWriter::new(File::create(&temporary).map_err(|e| e.to_string())?);
        serde_json::to_writer_pretty(&mut writer, data).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        writer.get_ref().sync_all().map_err(|e| e.to_string())?;
        drop(writer);
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                wide(&temporary).as_ptr(),
                wide(path).as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(format!(
                "Could not save locations: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
