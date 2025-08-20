use crate::config::new_config::Config;
use crate::plugins::cache::CachePlugin;
use crate::plugins::plugin::create_plugins;
use crate::plugins::plugin::{Plugin, PluginAction};
use async_trait::async_trait;
use hickory_proto::op::Header;
use hickory_proto::op::Query;
use hickory_proto::op::{Message, ResponseCode};
use hickory_proto::rr::Record;
use hickory_server::authority::MessageResponseBuilder;
use hickory_server::server::ResponseInfo;
use hickory_server::server::{Request, RequestHandler, ResponseHandler};
use std::any::Any;
use std::iter;
pub struct PipelineHandler {
    pub plugins: Vec<Box<dyn Plugin>>,
    pub config: Config,
}
impl PipelineHandler {
    pub async fn new(config: Config) -> Self {
        PipelineHandler {
            plugins: create_plugins(config.clone()).await,
            config,
        }
    }
}

#[async_trait]
impl RequestHandler for PipelineHandler {
    async fn handle_request<R: ResponseHandler>(
        &self,
        request: &Request,
        response_handler: R,
    ) -> ResponseInfo {
        let res = self
            .handle_request_with_error(request, response_handler)
            .await;
        match res {
            Ok(info) => info,
            Err(e) => {
                error!("I/O error occurred while sending final response: {}", e);
                let mut err_header = Header::new();
                err_header.set_response_code(ResponseCode::ServFail);
                ResponseInfo::from(err_header)
            }
        }
    }
}
impl PipelineHandler {
    async fn store_cache(&self, query: Query, records: Vec<Record>) {
        for plugin in &self.plugins {
            if let Some(cache_plugin) = (plugin as &dyn Any).downcast_ref::<CachePlugin>() {
                cache_plugin.store(query, records).await;
                break;
            }
        }
    }
    async fn handle_request_with_error<R: ResponseHandler>(
        &self,
        request: &Request,
        mut response_handler: R,
    ) -> Result<ResponseInfo, anyhow::Error> {
        let low_query = request.request_info().map_err(|_| anyhow!(""))?.query;
        let query = Query::query(low_query.name().into(), low_query.query_type());

        let mut message = Message::new();
        message.set_header(*request.header());
        message.set_id(request.id());
        message.set_message_type(request.message_type());
        message.set_op_code(request.op_code());
        message.set_recursion_desired(request.recursion_desired());
        message.add_query(query.clone());
        let mut final_response = None;
        for plugin in &self.plugins {
            tracing::debug!("Executing plugin: {}", plugin.name());

            // 将当前请求的引用传递给插件
            match plugin.handle_request(message).await {
                Ok(PluginAction::Response(records)) => {
                    let mut header = Header::response_from_request(request.header());

                    // 插件生成了最终响应，流水线终止。
                    tracing::debug!("Plugin '{}' generated a final response.", plugin.name());
                    info!("Query successful, found {} records", records.len());
                    if records.is_empty() {
                        header.set_response_code(ResponseCode::NXDomain);
                    }
                    let response = MessageResponseBuilder::from_message_request(request).build(
                        header,
                        &records,
                        iter::empty(),
                        iter::empty(),
                        iter::empty(),
                    );
                    self.store_cache(query, records.clone()).await;
                    final_response = Some(response_handler.send_response(response).await);
                    break; // 中断循环
                }
                Ok(PluginAction::Continue(modified_request)) => {
                    // 插件继续流水线，并可能传递了一个修改过的请求。
                    // 更新 current_request 以便下一个插件使用。
                    debug!("Plugin '{}' continued the pipeline.", plugin.name());
                    message = modified_request;
                }
                Err(code) => {
                    let mut header = Header::response_from_request(request.header());

                    error!("Error during lookup, response code: {}", code);
                    header.set_response_code(code);
                    let response = MessageResponseBuilder::from_message_request(request)
                        .build_no_records(header);
                    final_response = Some(response_handler.send_response(response).await);
                    break; // 中断循环
                }
            }
        }

        let res = match final_response {
            // 有结果，再匹配里面的 Result
            Some(Ok(info)) => info,
            Some(Err(e)) => {
                error!("I/O error occurred while sending final response: {}", e);
                let mut err_header = Header::new();
                err_header.set_response_code(ResponseCode::ServFail);
                ResponseInfo::from(err_header)
            }
            // 没有结果，说明循环正常结束，但没有插件生成响应
            None => {
                error!("I/O error occurred while sending final response");
                let mut err_header = Header::new();
                err_header.set_response_code(ResponseCode::ServFail);
                ResponseInfo::from(err_header)
            }
        };
        Ok(res)
    }
}
