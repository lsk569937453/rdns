use super::plugin::{Plugin, PluginAction};
use crate::config::new_config::FastForward;
use crate::impl_plugin_as_any;
use async_trait::async_trait;
use futures::future::select_all;
use futures_util::StreamExt;
use hickory_client::client::Client;
use hickory_proto::DnsHandle;
use hickory_proto::h2::HttpsClientStreamBuilder;
use hickory_proto::op::Message;
use hickory_proto::op::ResponseCode;
use hickory_proto::runtime::TokioRuntimeProvider;
use hickory_proto::rustls::tls_client_connect;
use hickory_proto::udp::UdpClientStream;
use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls::crypto::ring::default_provider;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use url::Url;

pub struct FastForwardPlugin {
    config: FastForward,
    pub forwarders: Arc<Vec<(String, Client)>>,
}
const ALPN_H2: &[u8] = b"h2";

impl FastForwardPlugin {
    pub async fn new(config: FastForward) -> Result<Self, anyhow::Error> {
        let mut forwarders = Vec::new();
        for upstream_str in config.upstreams.clone() {
            let client = if upstream_str.starts_with("https://") {
                let url = Url::parse(&upstream_str)?;
                let server_name = url
                    .host_str()
                    .ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::InvalidInput, "URL中缺少主机名")
                    })?
                    .to_string();
                let path = url.path().to_string();
                let mut root_store = RootCertStore::empty();
                root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                let mut client_config =
                    ClientConfig::builder_with_provider(Arc::new(default_provider()))
                        .with_safe_default_protocol_versions()?
                        .with_root_certificates(root_store)
                        .with_no_client_auth();
                client_config.alpn_protocols.push(ALPN_H2.to_vec());

                let client_config = Arc::new(client_config);

                let provider = TokioRuntimeProvider::new();
                let https_builder =
                    HttpsClientStreamBuilder::with_client_config(client_config, provider);
                let mut addrs = (server_name.clone(), 443).to_socket_addrs()?;

                // to_socket_addrs 返回一个迭代器，我们取第一个解析成功的地址
                let addr = addrs.next().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "无法将主机名解析为有效的IP地址",
                    )
                })?;
                info!("a,{},{},{}", addr, server_name, path);
                let mp = https_builder.build(addr, server_name, path);
                let (client, bg) = Client::connect(mp).await?;
                tokio::spawn(bg);
                client
            } else if upstream_str.starts_with("tls://") {
                info!("a1");

                let server_name = upstream_str.trim_start_matches("tls://");
                info!("a2");

                let mut addrs = (server_name, 853).to_socket_addrs()?;

                let addr = addrs.next().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "无法将主机名解析为有效的IP地址",
                    )
                })?;

                let mut root_store = RootCertStore::empty();
                root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                let mut client_config =
                    ClientConfig::builder_with_provider(Arc::new(default_provider()))
                        .with_safe_default_protocol_versions()?
                        .with_root_certificates(root_store)
                        .with_no_client_auth();
                client_config.alpn_protocols.push(ALPN_H2.to_vec());
                let client_config = Arc::new(client_config);
                let provider = TokioRuntimeProvider::new();
                let (stream, sender) = tls_client_connect(
                    addr,
                    server_name.to_string(),
                    client_config,
                    provider.clone(),
                );
                let (client, bg) = Client::new(stream, sender, None).await?;
                tokio::spawn(bg);
                client
            } else if upstream_str.ends_with(":53") {
                let upstream_addr: SocketAddr = upstream_str.parse()?;
                let conn = UdpClientStream::builder(upstream_addr, TokioRuntimeProvider::default())
                    .build();
                let (client, bg) = Client::connect(conn).await?;
                tokio::spawn(bg);
                client
            } else {
                return Err(anyhow::anyhow!(
                    "Unsupported upstream format: {}",
                    upstream_str
                ));
            };
            forwarders.push((upstream_str.clone(), client));
            info!("Upstream DNS server configured: {}", upstream_str);
        }

        if forwarders.is_empty() {
            return Err(anyhow::anyhow!("No upstream servers configured!"));
        }
        Ok(Self {
            config,
            forwarders: Arc::new(forwarders),
        })
    }
}

#[async_trait]
impl Plugin for FastForwardPlugin {
    fn name(&self) -> &'static str {
        "FastForward"
    }
    impl_plugin_as_any!();

    async fn handle_request(&self, request: Message) -> Result<PluginAction, ResponseCode> {
        let query = request.query().ok_or(ResponseCode::ServFail)?;
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
        let res = first_result.ok_or(ResponseCode::ServFail)?;
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
                Ok(PluginAction::Response(answers))
            }
            Err(e) => {
                error!("Error during lookup: {}", e);
                Err(ResponseCode::ServFail)
            }
        }
    }
}
