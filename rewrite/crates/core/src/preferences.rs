use crate::Config;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Serialize, Deserialize, Clone)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub members: Vec<String>,
}
pub fn save_group(config: &mut Config, mut group: Group) -> Result<(), String> {
    if group.id.trim().is_empty() || group.id == "all" || group.name.trim().is_empty() {
        return Err("Group name is required".into());
    }
    let valid: std::collections::HashSet<String> = config
        .custom
        .iter()
        .flat_map(|s| {
            std::iter::once(s.key.clone())
                .chain(s.tracks.iter().map(|t| format!("{}|{}", s.key, t.path)))
        })
        .collect();
    if group.members.iter().any(|m| !valid.contains(m)) {
        return Err("Group contains an unavailable station or track".into());
    }
    group.name = group.name.trim().into();
    group.members.sort();
    group.members.dedup();
    let value = serde_json::to_value(&group).map_err(|e| e.to_string())?;
    if let Some(existing) = config
        .groups
        .iter_mut()
        .find(|g| g["id"].as_str() == Some(&group.id))
    {
        if let Some(object) = existing.as_object_mut() {
            for (k, v) in value.as_object().unwrap() {
                object.insert(k.clone(), v.clone());
            }
        }
    } else {
        config.groups.push(value);
    }
    Ok(())
}
pub fn save_settings(config: &mut Config, patch: Map<String, Value>) -> Result<(), String> {
    for (key, value) in &patch {
        let valid = match key.as_str() {
            "accent" => value
                .as_str()
                .map(|s| {
                    s.len() == 7
                        && s.starts_with('#')
                        && s[1..].chars().all(|c| c.is_ascii_hexdigit())
                })
                .unwrap_or(false),
            "crossfade" => value
                .as_f64()
                .map(|n| n.is_finite() && (0.0..=30000.0).contains(&n))
                .unwrap_or(false),
            "showProgress" | "reduceMotion" | "randomNext" | "closeToTray" | "tuningTransition"
            | "showScanlines" | "matchBannerAccent" => value.is_boolean(),
            "tuningMinMs" | "tuningMaxMs" => value.as_u64().map(|n| n <= 1500).unwrap_or(false),
            "staticPath" => value.is_string(),
            "listDensity" => matches!(value.as_str(), Some("compact" | "comfortable")),
            "bannerLayout" => matches!(value.as_str(), Some("row" | "column")),
            "bounceSpeed" => value
                .as_f64()
                .map(|n| n.is_finite() && (10.0..=200.0).contains(&n))
                .unwrap_or(false),
            _ => false,
        };
        if !valid {
            return Err(format!("Invalid setting: {key}"));
        }
    }
    let minimum = patch
        .get("tuningMinMs")
        .or_else(|| config.settings.get("tuningMinMs"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let maximum = patch
        .get("tuningMaxMs")
        .or_else(|| config.settings.get("tuningMaxMs"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1500);
    if minimum > maximum {
        return Err("Minimum tuning delay exceeds maximum".into());
    }
    config.settings.extend(patch);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_are_validated_atomically_and_unknown_saved_values_survive() {
        let mut config = Config::default();
        config.settings.insert("future".into(), true.into());
        assert!(save_settings(
            &mut config,
            serde_json::from_str(r##"{"accent":"bad","showProgress":false}"##).unwrap()
        )
        .is_err());
        assert!(!config.settings.contains_key("showProgress"));
        save_settings(
            &mut config,
            serde_json::from_str(r##"{"accent":"#123abc","crossfade":5000}"##).unwrap(),
        )
        .unwrap();
        assert_eq!(config.settings["future"], true);
    }
    #[test]
    fn groups_reject_unknown_members() {
        let mut config = Config::default();
        assert!(save_group(
            &mut config,
            Group {
                id: "g".into(),
                name: "Group".into(),
                members: vec!["unknown".into()]
            }
        )
        .is_err());
        save_group(
            &mut config,
            Group {
                id: "g".into(),
                name: "Empty".into(),
                members: vec![],
            },
        )
        .unwrap();
        assert_eq!(config.groups.len(), 1);
    }
    #[test]
    fn electron_appearance_settings_validate_atomically() {
        let mut config = Config::default();
        save_settings(&mut config,serde_json::from_str(r#"{"bannerLayout":"column","showScanlines":false,"matchBannerAccent":true,"bounceSpeed":25}"#).unwrap()).unwrap();
        assert_eq!(config.settings["bannerLayout"], "column");
        assert!(save_settings(
            &mut config,
            serde_json::from_str(r#"{"bannerLayout":"row","bounceSpeed":0}"#).unwrap()
        )
        .is_err());
        assert_eq!(config.settings["bannerLayout"], "column");
        assert_eq!(config.settings["bounceSpeed"], 25);
    }
}
