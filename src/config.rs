use std::{collections::HashSet, fmt::Write, fs, path::Path};

use flexstr::SharedStr;
use itertools::Itertools;
use serde::{Deserialize, Deserializer, Serialize};
use toml::ser::ValueSerializer;
use unicase::UniCase;

use crate::core::ProgramPath;

#[derive(Clone, Deserialize, Debug)]
pub struct AnnieConfig {
    pub enabled: bool,
    #[serde(deserialize_with = "deserialize_managed_apps")]
    pub managed_apps: HashSet<ProgramPath>,
    pub max_recent_apps: usize,
}

impl AnnieConfig {
    pub fn new_empty() -> Self {
        AnnieConfig {
            enabled: true,
            managed_apps: Default::default(),
            max_recent_apps: 10,
        }
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let payload = fs::read_to_string(path)?;
        let config = toml::from_str(&payload)?;
        Ok(config)
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let payload = serialize_toml_config(self)?;
        fs::write(path, payload)?;
        Ok(())
    }
}

fn deserialize_managed_apps<'a, D: Deserializer<'a>>(
    d: D,
) -> Result<HashSet<ProgramPath>, D::Error> {
    let managed_apps: Vec<String> = Deserialize::deserialize(d)?;
    let managed_apps: HashSet<ProgramPath> = managed_apps
        .into_iter()
        .map(|app_name| UniCase::new(SharedStr::from(app_name)))
        .collect();
    Ok(managed_apps)
}

fn serialize_toml_config(config: &AnnieConfig) -> anyhow::Result<String> {
    fn write_field<V: Serialize>(writer: &mut String, value: &V) -> anyhow::Result<()> {
        Serialize::serialize(value, ValueSerializer::new(writer))?;
        Ok(())
    }

    fn write_array_field<V: Serialize>(
        writer: &mut String,
        values: impl IntoIterator<Item = V>,
    ) -> anyhow::Result<()> {
        writeln!(writer, "[")?;

        for value in values.into_iter() {
            write!(writer, "    ")?;
            Serialize::serialize(&value, ValueSerializer::new(writer))?;
            writeln!(writer, ",")?;
        }

        write!(writer, "]")?;
        Ok(())
    }

    let managed_apps = config
        .managed_apps
        .iter()
        .sorted()
        .map(|program_path| program_path.as_str())
        .collect_vec();

    let mut enabled_seri = String::new();
    write_field(&mut enabled_seri, &config.enabled)?;

    let mut managed_apps_seri = String::new();
    write_array_field(&mut managed_apps_seri, &managed_apps)?;

    let mut max_recent_apps_seri = String::new();
    write_field(&mut max_recent_apps_seri, &config.max_recent_apps)?;

    let serialized = format!(
        include_str!("../resource/config-template"),
        enabled = enabled_seri,
        managed_apps = managed_apps_seri,
        max_recent_apps = max_recent_apps_seri,
    );

    Ok(serialized)
}
