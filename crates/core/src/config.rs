//! The TOML file format (BEH-40). Only what is listed here is ever written: no passwords, no amp
//! settings, no loaded preset.

use std::path::Path;
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::store::{Amp, AmpId, Ceilings, Protocol, Source, Switches};

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct File {
    #[serde(default)]
    pub switches: Switches,
    #[serde(default, rename = "amp")]
    pub amps: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Entry {
    name: String,
    protocol: Protocol,
    ip: String,
    source: Source,
    connected: bool,
    #[serde(default = "default_load")]
    load_ohms: [f64; 2],
    #[serde(default)]
    ceilings: Ceilings,
}

pub(crate) const fn default_load() -> [f64; 2] {
    [4.0, 4.0]
}

impl Entry {
    pub fn from_amp(amp: &Amp) -> Entry {
        Entry {
            name: amp.name.clone(),
            protocol: amp.protocol,
            ip: amp.ip.clone(),
            source: amp.source,
            connected: amp.connected,
            load_ohms: amp.load_ohms,
            ceilings: amp.ceilings,
        }
    }

    pub fn into_amp(self, id: AmpId) -> Amp {
        Amp {
            id,
            name: self.name,
            protocol: self.protocol,
            ip: self.ip,
            source: self.source,
            connected: self.connected,
            load_ohms: self.load_ohms,
            ceilings: self.ceilings,
            preset: None,
        }
    }
}

/// A missing file is an empty config; any other failure is an error.
pub(crate) fn read(path: &Path) -> Result<File> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(toml::from_str(&text)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(File::default()),
        Err(e) => Err(e.into()),
    }
}

/// Writes next to the target and renames, so a crash never leaves a half-written config.
pub(crate) fn write(path: &Path, file: &File) -> Result<()> {
    let text = toml::to_string_pretty(file)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
