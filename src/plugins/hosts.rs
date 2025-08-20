use super::plugin::{Plugin, PluginAction};
use crate::config::new_config::HostRecord;
use crate::config::new_config::Hosts;
use crate::impl_plugin_as_any;
use async_trait::async_trait;
use hickory_proto::op::{Message, ResponseCode};
use hickory_proto::rr::Name;
use hickory_proto::rr::RData;
use hickory_proto::rr::Record;
use hickory_proto::rr::RecordType;

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::str::FromStr;
const DEFAULT_HOSTS_TTL: u32 = 300;
pub struct HostsPlugin {
    config: Hosts,
    host_map: HashMap<(Name, RecordType), Vec<Record>>,
}

impl HostsPlugin {
    pub fn new(config: Hosts) -> Self {
        let mut host_map: HashMap<(Name, RecordType), Vec<Record>> = HashMap::new();

        for (domain_str, host_record) in &config.records {
            if !domain_str.ends_with('.') {
                panic!(
                    "Arbitrary Plugin: Configuration error in rule for domain '{}'. \
                     All domains in arbitrary rules must be fully qualified (ending with a '.'). \
                     Please change it to '{}.'",
                    domain_str, domain_str
                );
            }
            let name = match Name::from_str(domain_str) {
                Ok(n) => n,
                Err(e) => {
                    error!(
                        "Hosts Plugin: Invalid domain name '{}': {}. Skipping entry.",
                        domain_str, e
                    );
                    continue;
                }
            };

            // 2. 将 Single 和 Multiple 两种情况都统一处理为 IP 字符串的 Vec
            let ip_strings = match host_record {
                HostRecord::Single(ip) => vec![ip.clone()],
                HostRecord::Multiple(ips) => ips.clone(),
            };

            for ip_str in ip_strings {
                let record = if let Ok(ipv4) = Ipv4Addr::from_str(&ip_str) {
                    let rdata = RData::A(hickory_proto::rr::rdata::A(ipv4));
                    Record::from_rdata(name.clone(), DEFAULT_HOSTS_TTL, rdata)
                } else if let Ok(ipv6) = Ipv6Addr::from_str(&ip_str) {
                    let rdata = RData::AAAA(hickory_proto::rr::rdata::AAAA(ipv6));
                    Record::from_rdata(name.clone(), DEFAULT_HOSTS_TTL, rdata)
                } else {
                    error!(
                        "Hosts Plugin: Invalid IP address '{}' for domain '{}'. Skipping this IP.",
                        ip_str, domain_str
                    );
                    continue;
                };

                let record_type = record.record_type();
                let key = (name.clone(), record_type);
                info!("Host Plugin: Inserting key into map: {:?}", &key);
                host_map.entry(key).or_insert_with(Vec::new).push(record);
            }
        }

        info!(
            "Hosts Plugin: Loaded {} domain entries into the lookup map.",
            host_map.len()
        );

        Self { config, host_map }
    }
}

#[async_trait]
impl Plugin for HostsPlugin {
    fn name(&self) -> &'static str {
        "hosts"
    }
    impl_plugin_as_any!();

    async fn handle_request(&self, request: Message) -> Result<PluginAction, ResponseCode> {
        let query = if let Some(q) = request.queries().first() {
            q
        } else {
            return Ok(PluginAction::Continue(request));
        };

        let lookup_key = (query.name().clone(), query.query_type());
        info!("Host Plugin: Attempting lookup with key: {:?}", &lookup_key);

        if let Some(record) = self.host_map.get(&lookup_key) {
            info!(
                "Host Plugin: Responding for query '{} {}' from pre-compiled map.",
                query.name(),
                query.query_type()
            );
            return Ok(PluginAction::Response(record.clone()));
        } else {
            info!(
                "Host Plugin: No matching record found for query '{} {}' from pre-compiled map.",
                query.name(),
                query.query_type()
            );
            Ok(PluginAction::Continue(request))
        }
    }
}
