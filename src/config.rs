use std::{collections::HashSet, fmt::Write, fs, path::Path};

use flexstr::SharedStr;
use itertools::Itertools;
use serde::{Deserialize, Deserializer, Serialize};
use toml::ser::ValueSerializer;
use unicase::UniCase;

use crate::core::ProgramPath;

const HEADER: &str = "\
    Annie config file\n\
    \n\
    Managed apps can be added or removed using the \"Recent apps\" context menu action.\n\
    If manual edits are required, after saving the file, reload the configuration using the \"Reload config from file\" context menu action.\n\
    This will prevent annie from overwriting this file with the program's internal state.\
";
const DESC_ENABLED: &str = "Whether to do any muting/unmuting. Setting this to false is equivalent to the annie process not running.";
const DESC_MANAGED_APPS: &str = "Programs managed by annie. Only programs specified here are automatically muted/unmuted by annie.";
const DESC_MAX_RECENT_APPS: &str =
    "Maximum number of items to be shown in the \"Recent apps\" menu.";

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
    fn write_comment(writer: &mut String, comment_text: &str) -> anyhow::Result<()> {
        for line in comment_text.split('\n') {
            if line.is_empty() {
                writeln!(writer)?
            } else {
                writeln!(writer, "# {}", line)?
            }
        }

        Ok(())
    }

    fn write_field<V: Serialize>(writer: &mut String, key: &str, value: &V) -> anyhow::Result<()> {
        write!(writer, "{key} = ")?;
        Serialize::serialize(value, ValueSerializer::new(writer))?;
        writeln!(writer)?;
        Ok(())
    }

    fn write_array_field<V: Serialize>(
        writer: &mut String,
        key: &str,
        values: impl IntoIterator<Item = V>,
    ) -> anyhow::Result<()> {
        writeln!(writer, "{key} = [")?;

        for value in values.into_iter() {
            write!(writer, "    ")?;
            Serialize::serialize(&value, ValueSerializer::new(writer))?;
            writeln!(writer, ",")?;
        }

        writeln!(writer, "]")?;
        Ok(())
    }

    let mut writer = String::new();

    let managed_apps = config
        .managed_apps
        .iter()
        .sorted()
        .map(|program_path| program_path.as_str())
        .collect_vec();

    write_comment(&mut writer, HEADER)?;

    writeln!(&mut writer)?;
    write_comment(&mut writer, DESC_ENABLED)?;
    write_field(&mut writer, "enabled", &config.enabled)?;

    writeln!(&mut writer)?;
    write_comment(&mut writer, DESC_MANAGED_APPS)?;
    write_array_field(&mut writer, "managed_apps", &managed_apps)?;

    writeln!(&mut writer)?;
    write_comment(&mut writer, DESC_MAX_RECENT_APPS)?;
    write_field(&mut writer, "max_recent_apps", &config.max_recent_apps)?;

    Ok(writer)
}
