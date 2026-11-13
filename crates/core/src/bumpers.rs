use crate::Station;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub enabled: bool,
    pub folder: String,
    pub method: String,
    pub min: f64,
    pub max: f64,
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.folder.len() > 32768
            || !self.min.is_finite()
            || !self.max.is_finite()
            || self.min > self.max
        {
            return Err("Invalid bumper folder or interval range".into());
        }
        match self.method.as_str() {
            "songs" if self.min>=1.0 && self.max<=1000.0 && self.min.fract()==0.0 && self.max.fract()==0.0 => {},
            "time" if self.min>=0.1 && self.max<=360.0 => {},
            _ => return Err("Bumpers need 1–1000 songs or 0.1–360 minutes, with minimum no greater than maximum".into()),
        }
        if self.enabled && self.folder.trim().is_empty() {
            return Err("Choose a bumper folder".into());
        }
        Ok(())
    }
    pub fn from_station(station: &Station) -> Result<Option<Self>, String> {
        if station.extra.get("mode").and_then(|v| v.as_str()) != Some("tracks") {
            return Ok(None);
        }
        let Some(value) = station.extra.get("bumpers").filter(|v| !v.is_null()) else {
            return Ok(None);
        };
        let settings: Self = serde_json::from_value(value.clone())
            .map_err(|e| format!("Invalid bumper settings: {e}"))?;
        settings.validate()?;
        Ok(settings.enabled.then_some(settings))
    }
}
pub fn folder_grant(path: &str) -> String {
    format!("bumper-folder\0{path}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_ranges_without_requiring_a_folder_for_disabled_bumpers() {
        let mut value = Settings {
            enabled: false,
            folder: String::new(),
            method: "songs".into(),
            min: 2.0,
            max: 5.0,
        };
        assert!(value.validate().is_ok());
        value.enabled = true;
        assert!(value.validate().is_err());
        value.folder = "clips".into();
        assert!(value.validate().is_ok());
        value.min = 5.1;
        assert!(value.validate().is_err());
        value.method = "time".into();
        value.min = 10.0;
        value.max = 15.0;
        assert!(value.validate().is_ok());
        value.max = 9.0;
        assert!(value.validate().is_err());
    }
    #[test]
    fn unselected_folder_cannot_be_smuggled_in_disabled_then_enabled() {
        use crate::{
            library::{self, StationEdit},
            Config, Track,
        };
        use std::collections::HashSet;
        let mut config = Config::default();
        let request = |enabled: bool, mode: &str| {
            StationEdit{key:"a".into(),name:"A".into(),mode:mode.into(),tracks:vec![Track{path:"music.wav".into(),name:"Music".into(),extra:Default::default()}],logo_path:None,
            options:serde_json::from_value(serde_json::json!({"bumpers":{"enabled":enabled,"folder":"private/clips","method":"songs","min":2,"max":5}})).unwrap()}
        };
        let mut grants = HashSet::from(["music.wav".into()]);
        assert!(library::upsert(
            &mut config,
            request(false, "tracks"),
            &grants,
            &HashSet::new()
        )
        .is_err());
        grants.insert(folder_grant("private/clips"));
        library::upsert(
            &mut config,
            request(true, "tracks"),
            &grants,
            &HashSet::new(),
        )
        .unwrap();
        grants.clear();
        library::upsert(
            &mut config,
            request(true, "tracks"),
            &grants,
            &HashSet::new(),
        )
        .unwrap();
        assert!(library::upsert(
            &mut config,
            request(true, "longset"),
            &grants,
            &HashSet::new()
        )
        .is_err());
        config.custom[0]
            .extra
            .insert("mode".into(), "longset".into());
        assert!(Settings::from_station(&config.custom[0]).unwrap().is_none());
    }
}
