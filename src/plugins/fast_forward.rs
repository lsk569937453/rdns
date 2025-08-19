use super::plugin::{Plugin, PluginAction, create_error_response};
use crate::config;
use crate::config::new_config::FastForward;
use async_trait::async_trait;

use hickory_client::client::Client;
use hickory_proto::op::Message;
use hickory_proto::op::ResponseCode;
use hickory_proto::xfer::DnsRequest;
use hickory_server::server::Request;

pub struct FastForwardPlugin {
    config: FastForward,
    // 在实际生产中，您可能需要一个更健壮的客户端池
    // 这里为简单起见，我们每次请求都可能新建连接
}

impl FastForwardPlugin {
    pub fn new(config: FastForward) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Plugin for FastForwardPlugin {
    fn name(&self) -> &'static str {
        "fast_forward"
    }

    async fn handle_request(&self, request: Message) -> Result<PluginAction, ResponseCode> {
        Err(ResponseCode::ServFail)
    }
}
