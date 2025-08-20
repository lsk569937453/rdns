use crate::config::new_config::Config;
use crate::plugins::cache::CachePlugin;
use crate::plugins::plugin::create_plugins;
use crate::plugins::plugin::{Plugin, PluginAction};
use async_trait::async_trait;
use hickory_proto::op::Header;
use hickory_proto::op::Query;
use hickory_proto::op::{Message, ResponseCode};
use hickory_proto::rr::RData;
use hickory_proto::rr::Record;
use hickory_proto::rr::RecordType;
use hickory_server::authority::MessageResponseBuilder;
use hickory_server::server::ResponseInfo;
use hickory_server::server::{Request, RequestHandler, ResponseHandler};
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
        info!("start store cache");
        for plugin in &self.plugins {
            info!("plugin name: {}", plugin.name());
            if let Some(cache_plugin) = plugin.as_any().downcast_ref::<CachePlugin>() {
                info!(
                    "Cache plugin found, storing records for query: {}",
                    query.name()
                );
                cache_plugin.store(query.clone(), records).await;
                info!("Cache plugin stored records for query: {}", query.name());
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
        let mut final_records: Vec<Record> = Vec::new();

        const MAX_CNAME_HOPS: u8 = 10;
        for hop_count in 0..MAX_CNAME_HOPS {
            for plugin in &self.plugins {
                let current_query_in_loop = message
                    .queries()
                    .first()
                    .ok_or(anyhow!("Not found query"))?;

                info!(
                    "[Hop {}] Executing plugin '{}' for query '{} {}'",
                    hop_count + 1,
                    plugin.name(),
                    current_query_in_loop.name(),
                    current_query_in_loop.query_type()
                );

                match plugin.handle_request(message.clone()).await {
                    Ok(PluginAction::Response(records)) => {
                        let current_query = message.query().ok_or(anyhow!(""))?;
                        let current_qname = current_query.name();
                        let is_chaseable_cname = if let Some(cname_record) =
                            records.iter().find(|r| {
                                r.record_type() == RecordType::CNAME && r.name() == current_qname
                            }) {
                            if let RData::CNAME(target_name) = cname_record.data() {
                                !records.iter().any(|r| {
                                    *r.name() == target_name.0
                                        && r.record_type() == query.query_type()
                                })
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        if is_chaseable_cname {
                            let cname_record = records.first().ok_or(anyhow!(""))?; // 我们知道它在这里
                            if let RData::CNAME(cname) = cname_record.data() {
                                info!("Plugin '{}' returned a CNAME. Chasing...", plugin.name());
                                final_records.push(cname_record.clone());
                                let mut new_query = current_query.clone();
                                new_query.set_name(cname.0.clone());
                                message = Message::new();
                                message.set_header(*request.header());
                                message.add_query(new_query);
                                break;
                            }
                        } else {
                            info!("Plugin '{}' generated a final response.", plugin.name());
                            final_records.extend(records);
                        }

                        let mut header = Header::response_from_request(request.header());
                        debug!("Plugin '{}' generated a final response.", plugin.name());
                        info!("Query successful, found {} records", final_records.len());
                        if final_records.is_empty() {
                            header.set_response_code(ResponseCode::NXDomain);
                        }
                        let response = MessageResponseBuilder::from_message_request(request).build(
                            header,
                            &final_records,
                            iter::empty(),
                            iter::empty(),
                            iter::empty(),
                        );
                        if plugin.name() != "cache" {
                            info!(
                                "Response generated by plugin '{}', storing result in cache for query: {}",
                                plugin.name(),
                                query.name()
                            );
                            self.store_cache(query.clone(), final_records.clone()).await;
                        } else {
                            info!(
                                "Response was served from cache, skipping store_cache operation for query: {}",
                                query.name()
                            );
                        }
                        return Ok(response_handler.send_response(response).await?);
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
                        return Ok(response_handler.send_response(response).await?);
                    }
                }
            }
        }
        info!("Max CNAME hops reached, returning NXDomain.");
        Ok(ResponseInfo::from(Header::new()))
        // let res = match final_response {
        //     // 有结果，再匹配里面的 Result
        //     Some(Ok(info)) => info,
        //     Some(Err(e)) => {
        //         error!("I/O error occurred while sending final response: {}", e);
        //         let mut err_header = Header::new();
        //         err_header.set_response_code(ResponseCode::ServFail);
        //         ResponseInfo::from(err_header)
        //     }
        //     // 没有结果，说明循环正常结束，但没有插件生成响应
        //     None => {
        //         error!("I/O error occurred while sending final response");
        //         let mut err_header = Header::new();
        //         err_header.set_response_code(ResponseCode::ServFail);
        //         ResponseInfo::from(err_header)
        //     }
        // };
        // Ok(res)
    }
}
