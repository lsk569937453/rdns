use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub port: i32,
    pub upstream_servers: Vec<String>,
    pub records: HashMap<String, RecordConfig>,
}

#[derive(Debug, Deserialize)]
pub struct RecordConfig {
    #[serde(rename = "type")]
    pub record_type: String,
    pub value: String,
}
