use super::plugin::{Plugin, PluginAction};
use crate::config;
use crate::config::new_config::Hosts;
use async_trait::async_trait;
use hickory_proto::op::{Message, ResponseCode};
use hickory_proto::rr::{Name, RData, Record, RecordType};
use hickory_server::server::Request;
use std::net::IpAddr;

pub struct HostsPlugin {
    config: Hosts,
}

impl HostsPlugin {
    pub fn new(config: Hosts) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Plugin for HostsPlugin {
    fn name(&self) -> &'static str {
        "hosts"
    }

    async fn handle_request(&self, request: Message) -> Result<PluginAction, ResponseCode> {
        Err(ResponseCode::ServFail)
    }
}
