//! Compact mutation results. Preserve unrelated renderer objects and extensions.
use crate::{Config, Station};
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Map<String, Value>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stations: Vec<Station>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed_stations: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed_groups: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub logos: Vec<LogoUpdate>,
}
#[derive(Serialize)]
pub struct LogoUpdate {
    pub key: String,
    pub removed: bool,
    pub value: Option<Value>,
}
impl LibraryUpdate {
    pub fn settings(config: &Config) -> Self {
        Self {
            settings: Some(config.settings.clone()),
            ..Default::default()
        }
    }
    pub fn station(before: &Config, after: &Config, key: &str) -> Self {
        let logos = if before.logos.get(key) != after.logos.get(key) {
            vec![LogoUpdate {
                key: key.into(),
                removed: !after.logos.contains_key(key),
                value: after.logos.get(key).cloned(),
            }]
        } else {
            vec![]
        };
        Self {
            stations: after
                .custom
                .iter()
                .filter(|s| s.key == key)
                .cloned()
                .collect(),
            groups: changed_groups(before, after),
            logos,
            ..Default::default()
        }
    }
    pub fn removed(before: &Config, after: &Config, keys: Vec<String>) -> Self {
        Self {
            removed_stations: keys,
            groups: changed_groups(before, after),
            ..Default::default()
        }
    }
    pub fn groups(before: &Config, after: &Config) -> Self {
        let remaining: std::collections::HashSet<_> = after
            .groups
            .iter()
            .filter_map(|g| g["id"].as_str())
            .collect();
        Self {
            groups: changed_groups(before, after),
            removed_groups: before
                .groups
                .iter()
                .filter_map(|g| g["id"].as_str())
                .filter(|id| !remaining.contains(id))
                .map(str::to_owned)
                .collect(),
            ..Default::default()
        }
    }
}
fn changed_groups(before: &Config, after: &Config) -> Vec<Value> {
    let previous: std::collections::HashMap<_, _> = before
        .groups
        .iter()
        .filter_map(|group| Some((group["id"].as_str()?, group)))
        .collect();
    after
        .groups
        .iter()
        .filter(|group| {
            group["id"]
                .as_str()
                .is_none_or(|id| previous.get(id).copied() != Some(*group))
        })
        .cloned()
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mutation_results_include_only_affected_entities_and_preserve_extensions() {
        let mut before = Config::default();
        before.custom.push(Station {
            key: "a".into(),
            station: "A".into(),
            tracks: vec![],
            extra: serde_json::from_value(serde_json::json!({"futureStation":7})).unwrap(),
        });
        before.custom.push(Station {
            key: "b".into(),
            station: "B".into(),
            tracks: vec![],
            extra: Default::default(),
        });
        before.groups = vec![
            serde_json::json!({"id":"g","name":"G","members":["a","b"],"futureGroup":7}),
            serde_json::json!({"id":"other","name":"Other","members":["b"]}),
        ];
        before.settings.insert("futureSetting".into(), 7.into());
        let mut after = before.clone();
        after.custom[0].station = "Edited".into();
        after.groups[0]["members"] = serde_json::json!(["b"]);
        after.logos.insert("a".into(), Value::Null);
        let update = LibraryUpdate::station(&before, &after, "a");
        let value = serde_json::to_value(update).unwrap();
        assert_eq!(value["stations"].as_array().unwrap().len(), 1);
        assert_eq!(value["stations"][0]["futureStation"], 7);
        assert_eq!(value["groups"].as_array().unwrap().len(), 1);
        assert_eq!(value["groups"][0]["futureGroup"], 7);
        assert_eq!(value["logos"][0]["removed"], false);
        assert!(value["logos"][0]["value"].is_null());
        assert!(value.get("custom").is_none());
        assert!(value.get("root").is_none());
        let settings = serde_json::to_value(LibraryUpdate::settings(&after)).unwrap();
        assert_eq!(settings["settings"]["futureSetting"], 7);
        assert!(settings.get("stations").is_none());
        let removal =
            serde_json::to_value(LibraryUpdate::removed(&before, &after, vec!["a".into()]))
                .unwrap();
        assert_eq!(removal["removedStations"], serde_json::json!(["a"]));
    }
}
