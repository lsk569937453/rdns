use crate::config::new_config::Config;
use crate::plugins::blackhole::BlackholePlugin;
use crate::plugins::cache::CachePlugin;
use crate::plugins::fast_forward::FastForwardPlugin;
use crate::plugins::hosts::HostsPlugin;
use async_trait::async_trait;
use hickory_proto::op::Message;
use hickory_proto::op::ResponseCode;
use hickory_proto::rr::Record;
/// 插件处理请求后的动作
pub enum PluginAction {
    /// 插件已生成响应，流水线应终止
    Response(Vec<Record>),
    /// 继续下一个插件，请求可能已被修改
    Continue(Message),
}
pub type PluginOutput = anyhow::Result<Option<Message>>;

/// 所有插件必须实现的 Trait
#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;
    async fn handle_request(&self, request: Message) -> Result<PluginAction, ResponseCode>;
}

/// 根据配置创建插件实例的工厂函数
pub async fn create_plugins(config: Config) -> Vec<Box<dyn Plugin>> {
    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();

    // 严格按照 pipeline 顺序创建插件
    for name in &config.pipeline {
        match name.as_str() {
            "cache" => {
                if let Some(ref conf) = config.cache
                    && conf.enabled
                {
                    plugins.push(Box::new(CachePlugin::new(conf.clone())));
                    info!("Plugin enabled: cache");
                }
            }
            "hosts" => {
                if let Some(ref conf) = config.hosts
                    && conf.enabled
                {
                    plugins.push(Box::new(HostsPlugin::new(conf.clone())));
                    info!("Plugin enabled: hosts");
                }
            }
            "blackhole" => {
                if let Some(ref conf) = config.blackhole
                    && conf.enabled
                {
                    plugins.push(Box::new(BlackholePlugin::new(conf.clone())));
                    info!("Plugin enabled: blackhole");
                }
            }
            "fast_forward" => {
                if let Some(ref conf) = config.fast_forward {
                    plugins.push(Box::new(
                        FastForwardPlugin::new(conf.clone()).await.unwrap(),
                    ));

                    info!("Plugin enabled: fast_forward");
                }
            }
            _ => {
                warn!("Plugin '{}' defined in pipeline but not implemented.", name);
            }
        }
    }
    plugins
}

/// 创建一个表示错误的 DNS 响应
pub fn create_error_response(request: &Message, code: ResponseCode) -> Message {
    let mut message = Message::new();
    message.set_id(request.id());
    message.set_header(*request.header());
    message.set_response_code(code);
    message
}
