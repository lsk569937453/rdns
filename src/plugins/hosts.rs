use super::plugin::{Plugin, PluginAction};
use crate::config::new_config::Hosts;
use crate::impl_plugin_as_any;
use async_trait::async_trait;
use hickory_proto::op::{Message, ResponseCode};

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
    impl_plugin_as_any!();

    async fn handle_request(&self, request: Message) -> Result<PluginAction, ResponseCode> {
        Ok(PluginAction::Continue(request))
    }
}
