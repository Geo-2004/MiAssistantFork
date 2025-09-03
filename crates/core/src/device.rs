use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct DeviceInfo {
    pub device: String,
    pub version: String,
    pub sn: String,
    pub codebase: String,
    pub branch: String,
    pub language: String,
    pub region: String,
    pub romzone: String,
}

impl DeviceInfo {
    pub fn unknown() -> Self {
        Self {
            device: "unknown".into(),
            version: "unknown".into(),
            sn: "unknown".into(),
            codebase: "unknown".into(),
            branch: "unknown".into(),
            language: "unknown".into(),
            region: "unknown".into(),
            romzone: "unknown".into(),
        }
    }
}
