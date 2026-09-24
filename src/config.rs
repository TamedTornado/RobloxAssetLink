use crate::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PartSettings {
    pub anchored: bool,
    pub can_collide: bool,
    pub can_touch: bool,
    pub can_query: bool,
    pub transparency: Option<f32>,
    pub double_sided: bool,
    pub collision_fidelity: CollisionFidelity,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum CollisionFidelity {
    Box,
    Hull,
    Default,
    PreciseConvexDecomposition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    pub name_prefix: String,
    pub settings: PartSettings,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Destination {
    Workspace,
    ServerStorage,
    ReplicatedStorage,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub project_id: String,
    pub metres_per_stud: f32,
    pub poll_seconds: f64,
    pub destination: Destination,
    pub container_name: String,
    pub default_part: PartSettings,
    /// First matching prefix wins. Empty means match every mesh.
    pub rules: Vec<Rule>,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.project_id.trim().is_empty() || self.container_name.trim().is_empty() {
            return Err("projectId and containerName must be nonempty".into());
        }
        if !self.metres_per_stud.is_finite()
            || self.metres_per_stud <= 0.
            || !self.poll_seconds.is_finite()
            || self.poll_seconds <= 0.
        {
            return Err("metresPerStud and pollSeconds must be positive and finite".into());
        }
        for settings in
            std::iter::once(&self.default_part).chain(self.rules.iter().map(|r| &r.settings))
        {
            if settings
                .transparency
                .is_some_and(|v| !v.is_finite() || !(0. ..=1.).contains(&v))
            {
                return Err("transparency must be null or between zero and one".into());
            }
        }
        Ok(())
    }

    pub fn settings_for(&self, name: &str) -> PartSettings {
        self.rules
            .iter()
            .find(|r| name.starts_with(&r.name_prefix))
            .map_or(&self.default_part, |r| &r.settings)
            .clone()
    }
}

pub fn read(path: &Path) -> Result<Config> {
    let config: Config = serde_json::from_slice(&fs::read(path)?)?;
    config.validate()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> Config {
        serde_json::from_str(include_str!("../examples/building-kit.json")).unwrap()
    }

    #[test]
    fn rules_are_not_built_in() {
        let mut config = example();
        config.validate().unwrap();
        assert_eq!(config.settings_for("COL_ramp").transparency, Some(1.));
        config.rules[0].name_prefix = "physics_".into();
        assert_eq!(config.settings_for("COL_ramp").transparency, None);
        assert_eq!(config.settings_for("physics_ramp").transparency, Some(1.));
        config.rules.clear();
        assert_eq!(config.settings_for("physics_ramp").transparency, None);
    }

    #[test]
    fn invalid_values_and_unknown_keys_are_rejected() {
        let mut config = example();
        config.poll_seconds = 0.;
        assert!(config.validate().is_err());
        config.poll_seconds = 2.;
        config.default_part.transparency = Some(2.);
        assert!(config.validate().is_err());
        let mut value = serde_json::to_value(example()).unwrap();
        value["typo"] = true.into();
        assert!(serde_json::from_value::<Config>(value).is_err());
    }
}
