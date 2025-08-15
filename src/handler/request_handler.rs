use async_trait::async_trait;
use futures_util::StreamExt;
use hickory_client::client::Client;
use hickory_proto::DnsHandle;
use hickory_proto::op::Query;

use hickory_server::authority::MessageResponseBuilder;
use hickory_server::proto::op::{Header, MessageType, OpCode, ResponseCode};
use hickory_server::proto::rr::{Name, RData, Record, RecordType};
use hickory_server::server::ResponseInfo;
use hickory_server::server::{Request, RequestHandler, ResponseHandler};
use std::collections::HashMap;
use std::iter;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
// 自定义请求处理器
pub struct MyRequestHandler {
    pub records: Arc<HashMap<Name, RData>>,
    pub forwarder: Client,
}
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

#[async_trait]
impl RequestHandler for MyRequestHandler {
    async fn handle_request<R: ResponseHandler>(
        &self,
        request: &Request,
        mut response_handler: R,
    ) -> ResponseInfo {
        let mut header = Header::response_from_request(request.header());
        header.set_message_type(MessageType::Response);

        if request.message_type() != MessageType::Query || request.op_code() != OpCode::Query {
            error!(
                "Received unsupported request type or opcode: {:?}, {:?}",
                request.message_type(),
                request.op_code()
            );
            header.set_response_code(ResponseCode::NotImp);
            let response =
                MessageResponseBuilder::from_message_request(request).build_no_records(header);
            return response_handler
                .send_response(response)
                .await
                .unwrap_or_else(|e| {
                    error!("Failed to send NotImp response: {}", e);
                    ResponseInfo::from(header)
                });
        }

        let lookup_result = match timeout(QUERY_TIMEOUT, self.lookup(request)).await {
            Ok(result) => result,
            Err(_) => {
                error!(
                    "Request processing timeout (exceeded {} seconds)",
                    QUERY_TIMEOUT.as_secs()
                );
                Err(ResponseCode::ServFail)
            }
        };

        let response_res = match lookup_result {
            Ok(records) => {
                info!("Query successful, found {} records", records.len());
                if records.is_empty() {
                    header.set_response_code(ResponseCode::NXDomain);
                }
                // 创建一个拥有的 Response
                let response = MessageResponseBuilder::from_message_request(request).build(
                    header,
                    &records,
                    iter::empty(),
                    iter::empty(),
                    iter::empty(),
                );

                response_handler.send_response(response).await
            }
            Err(code) => {
                error!("Error during lookup, response code: {}", code);
                header.set_response_code(code);
                let response =
                    MessageResponseBuilder::from_message_request(request).build_no_records(header);
                response_handler.send_response(response).await
            }
        };

        match response_res {
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

impl MyRequestHandler {
    async fn lookup(&self, request: &Request) -> Result<Vec<Record>, ResponseCode> {
        let low_query = request
            .request_info()
            .map_err(|_| ResponseCode::ServFail)?
            .query;
        let query = Query::query(low_query.name().into(), low_query.query_type());
        match self.handle_local_record(&query) {
            Ok(records) => {
                info!("Found record locally: {}", query.name());
                Ok(records)
            }
            Err(_) => self.handle_forwarding(query).await.map_err(|e| {
                error!("Error during forwarding: {}", e);
                ResponseCode::ServFail
            }),
        }
    }

    fn handle_local_record(&self, query: &Query) -> Result<Vec<Record>, anyhow::Error> {
        if let Some(rdata) = self.records.get(query.name()) {
            if query.query_type() == rdata.record_type() || query.query_type() == RecordType::ANY {
                let record = Record::from_rdata(query.name().clone(), 3600, rdata.clone());
                Ok(vec![record])
            } else {
                Err(anyhow::anyhow!("Name matches but type mismatch"))
            }
        } else {
            Err(anyhow::anyhow!("Record not found locally"))
        }
    }

    async fn handle_forwarding(&self, query: Query) -> Result<Vec<Record>, anyhow::Error> {
        info!(
            "Record not found locally, forwarding query: {}",
            query.name()
        );
        let mut response = self.forwarder.lookup(query, Default::default());
        let mut all_records = Vec::new();

        while let Some(response) = response.next().await {
            let dns_response = response?;
            let message = dns_response.into_message();
            let answers_slice = message.answers();
            for answer_record in answers_slice {
                all_records.push(answer_record.clone());
            }
        }

        let mut unique_records = Vec::new();
        for record in all_records {
            if !unique_records.contains(&record) {
                unique_records.push(record);
            }
        }

        Ok(unique_records)
    }
}
