use async_trait::async_trait;
use futures::future::select_all;
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
pub struct MyRequestHandler {
    pub records: Arc<HashMap<Name, RData>>,
    pub forwarders: Arc<Vec<(String, Client)>>,
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
        let mut final_records = Vec::new();
        let mut current_name = query.name().clone();
        let mut recursion_limit = 10;

        loop {
            if recursion_limit == 0 {
                return Err(anyhow::anyhow!("CNAME recursion limit reached"));
            }
            recursion_limit -= 1;

            let rdata = self
                .records
                .get(&current_name)
                .ok_or_else(|| anyhow::anyhow!("Record for {} not found locally", current_name))?;

            let record = Record::from_rdata(current_name.clone(), 3600, rdata.clone());
            final_records.push(record);

            match rdata {
                RData::CNAME(cname)
                    if matches!(query.query_type(), RecordType::A | RecordType::AAAA) =>
                {
                    current_name = cname.0.clone();
                }

                _ if rdata.record_type() == query.query_type() => {
                    return Ok(final_records);
                }

                _ => {
                    return Err(anyhow::anyhow!(
                        "Found a record for {} but its type ({:?}) does not match the final query type ({:?})",
                        current_name,
                        rdata.record_type(),
                        query.query_type()
                    ));
                }
            }
        }
    }

    async fn handle_forwarding(&self, query: Query) -> Result<Vec<Record>, anyhow::Error> {
        info!(
            "Record not found locally, forwarding query to all upstreams: {}",
            query.name()
        );

        let lookups = self.forwarders.iter().map(|(addr, client)| {
            let client = client.clone();
            let query = query.clone();
            let addr = addr.clone();

            Box::pin(async move {
                let result = client.lookup(query, Default::default()).next().await;
                (addr, result)
            })
        });

        let ((fastest_server_addr, first_result), _, _) = select_all(lookups).await;
        let res = first_result.ok_or(anyhow!("All upstream servers returned an error."))?;
        match res {
            Ok(dns_response) => {
                info!(
                    "Got fastest response from upstream server: {}",
                    fastest_server_addr
                );
                let message = dns_response.into_message();
                let answers = message.answers().to_vec();
                if answers.is_empty() && message.response_code() == ResponseCode::NoError {
                    info!("Fastest response was an empty answer, treating as NXDomain.");
                }
                Ok(answers)
            }
            Err(e) => Err(anyhow::anyhow!(
                "The fastest responding server ({}) returned an error: {}",
                fastest_server_addr,
                e
            )),
        }
    }
}
