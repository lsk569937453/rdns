use super::plugin::{Plugin, PluginAction};
use crate::config::new_config::Blackhole;
use crate::config::new_config::BlackholeStrategy;
use crate::impl_plugin_as_any;
use async_trait::async_trait;
use hickory_proto::op::{Message, ResponseCode};
use std::collections::HashSet;
pub struct BlackholePlugin {
    config: Blackhole,
    domains: HashSet<String>,
}

impl BlackholePlugin {
    pub fn new(config: Blackhole) -> Self {
        let domains = config.domains.iter().cloned().collect();
        Self { config, domains }
    }
}

#[async_trait]
impl Plugin for BlackholePlugin {
    fn name(&self) -> &'static str {
        "blackhole"
    }
    impl_plugin_as_any!();

    async fn handle_request(&self, request: Message) -> Result<PluginAction, ResponseCode> {
        let name_str = request
            .query()
            .ok_or(ResponseCode::ServFail)?
            .name
            .to_string();

        if self.domains.contains(&name_str) {
            tracing::info!("Blackhole HIT for {}", name_str);
            let rescode = match self.config.strategy {
                BlackholeStrategy::Drop => ResponseCode::Refused,
                BlackholeStrategy::Empty => ResponseCode::NoError,
                BlackholeStrategy::CustomIp => ResponseCode::NXDomain,
            };

            return Err(rescode);
        }

        Ok(PluginAction::Continue(request))
    }
}
