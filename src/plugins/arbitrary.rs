use super::plugin::{Plugin, PluginAction};
use crate::config::new_config::Arbitrary;
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
pub struct ArbitraryPlugin {
    config: Arbitrary,
    arbitrary_map: HashMap<(Name, RecordType), Vec<Record>>,
}

impl ArbitraryPlugin {
    pub fn new(config: Arbitrary) -> Self {
        let mut arbitrary_map = HashMap::new();

        for rule in &config.rules {
            if !rule.domain.ends_with('.') {
                panic!(
                    "Arbitrary Plugin: Configuration error in rule for domain '{}'. \
                     All domains in arbitrary rules must be fully qualified (ending with a '.'). \
                     Please change it to '{}.'",
                    rule.domain, rule.domain
                );
            }
            let name = match Name::from_str(&rule.domain) {
                Ok(n) => n,
                Err(e) => {
                    error!(
                        "Arbitrary Plugin: Invalid domain name '{}' in rule: {}. Skipping rule.",
                        rule.domain, e
                    );
                    continue;
                }
            };

            let qtype = match RecordType::from_str(&rule.qtype.to_uppercase()) {
                Ok(t) => t,
                Err(_) => {
                    error!(
                        "Arbitrary Plugin: Invalid qtype '{}' for domain '{}'. Skipping rule.",
                        rule.qtype, rule.domain
                    );
                    continue;
                }
            };

            let rdata = match build_rdata(qtype, &rule.value) {
                Ok(data) => data,
                Err(e) => {
                    error!(
                        "Arbitrary Plugin: Failed to build RData for domain '{}': {}. Skipping rule.",
                        rule.domain, e
                    );
                    continue;
                }
            };

            let record = Record::from_rdata(name.clone(), rule.ttl, rdata);
            let key = (name, qtype);
            info!("Arbitrary Plugin: Inserting key into map: {:?}", &key);
            arbitrary_map
                .entry(key)
                .or_insert_with(Vec::new)
                .push(record);
        }

        info!(
            "Arbitrary Plugin: Loaded {} rule(s) into the lookup map.",
            arbitrary_map.len()
        );
        Self {
            config,
            arbitrary_map,
        }
    }
}
pub fn build_rdata(qtype: RecordType, value: &str) -> Result<RData, String> {
    match qtype {
        RecordType::A => {
            let ip: Ipv4Addr = value
                .parse()
                .map_err(|e| format!("Invalid IPv4 address '{}': {}", value, e))?;
            Ok(RData::A(hickory_proto::rr::rdata::A(ip)))
        }
        RecordType::AAAA => {
            let ip: Ipv6Addr = value
                .parse()
                .map_err(|e| format!("Invalid IPv6 address '{}': {}", value, e))?;
            Ok(RData::AAAA(hickory_proto::rr::rdata::AAAA(ip)))
        }
        RecordType::CNAME => {
            let name = Name::from_str(value)
                .map_err(|e| format!("Invalid CNAME value '{}': {}", value, e))?;
            Ok(RData::CNAME(hickory_proto::rr::rdata::CNAME(name)))
        }
        RecordType::TXT => Ok(RData::TXT(hickory_proto::rr::rdata::TXT::new(vec![
            value.to_string(),
        ]))),
        _ => Err(format!(
            "Unsupported record type '{:?}' for arbitrary response",
            qtype
        )),
    }
}
#[async_trait]
impl Plugin for ArbitraryPlugin {
    fn name(&self) -> &'static str {
        "arbitrary"
    }
    impl_plugin_as_any!();

    async fn handle_request(&self, request: Message) -> Result<PluginAction, ResponseCode> {
        let query = if let Some(q) = request.queries().first() {
            q
        } else {
            return Ok(PluginAction::Continue(request));
        };
        let query_name = query.name();
        let query_type = query.query_type();
        let lookup_key = (query_name.clone(), query_type);
        info!(
            "Arbitrary Plugin: Attempting lookup with key: {:?}",
            &lookup_key
        );

        if let Some(records) = self.arbitrary_map.get(&lookup_key) {
            info!(
                "Arbitrary Plugin: Found direct match for '{} {}'.",
                query_name, query_type
            );
            return Ok(PluginAction::Response(records.clone()));
        }

        if matches!(query_type, RecordType::A | RecordType::AAAA) {
            let cname_lookup_key = (query_name.clone(), RecordType::CNAME);
            if let Some(cname_records) = self.arbitrary_map.get(&cname_lookup_key) {
                info!(
                    "Arbitrary Plugin: Found CNAME for '{} {}'.",
                    query_name, query_type
                );
                return Ok(PluginAction::Response(cname_records.clone()));
            }
        }

        info!(
            "Arbitrary Plugin: No match for '{} {}'. Passing to next plugin.",
            query_name, query_type
        );
        Ok(PluginAction::Continue(request))
    }
}
