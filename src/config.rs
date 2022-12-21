use std::{collections::HashSet, fs::File, path::Path};

use serde::{Deserialize, Serialize};

use crate::core::ProgramPath;

mod managed_apps_bridge {
    use std::collections::HashSet;

    use itertools::Itertools;
    use serde::{Deserialize, Deserializer, Serializer};
    use unicase::UniCase;

    use crate::core::ProgramPath;

    pub fn serialize<S: Serializer>(
        managed_apps: &HashSet<ProgramPath>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let managed_apps = managed_apps
            .iter()
            .sorted()
            .map(|program_path| program_path.as_str());
        s.collect_seq(managed_apps)
    }

    pub fn deserialize<'a, D: Deserializer<'a>>(d: D) -> Result<HashSet<ProgramPath>, D::Error> {
        let managed_apps: Vec<String> = Deserialize::deserialize(d)?;
        let managed_apps: HashSet<ProgramPath> = managed_apps
            .into_iter()
            .map(|app_name| UniCase::new(app_name.into()))
            .collect();
        Ok(managed_apps)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AnnieConfig {
    pub enabled: bool,
    #[serde(with = "managed_apps_bridge")]
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
        let file = File::open(path)?;
        let config = serde_json::from_reader(file)?;
        Ok(config)
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let file = File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }
}
