mod config;
mod handler;
mod plugins;
use crate::config::cli::Cli;
use crate::config::config::Config;
use crate::config::new_config::Config as NewConfig;

use crate::handler::pipline_handler::PipelineHandler;
use crate::handler::request_handler::MyRequestHandler;
use chrono::Utc;
use chrono_tz::Asia::Shanghai;
use clap::Parser;
use hickory_client::client::Client;
use hickory_proto::rr::Name;
use hickory_proto::rr::RData;
use hickory_proto::runtime::TokioRuntimeProvider;
use hickory_proto::udp::UdpClientStream;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tracing_appender::rolling;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
#[macro_use]
extern crate tracing;
#[macro_use]
extern crate anyhow;

struct ShanghaiTime;

impl FormatTime for ShanghaiTime {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        let now_shanghai = Utc::now().with_timezone(&Shanghai);
        write!(w, "{}", now_shanghai.format("%Y-%m-%d %H:%M:%S%.3f"))
    }
}
fn setup_logger() -> Result<(), anyhow::Error> {
    let app_file = rolling::daily("./logs", "access.log");

    let file_layer = tracing_subscriber::fmt::Layer::new()
        .with_timer(ShanghaiTime)
        .with_line_number(true)
        .with_target(true)
        .with_ansi(false)
        .with_writer(app_file)
        .with_filter(tracing_subscriber::filter::LevelFilter::INFO);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(tracing_subscriber::filter::LevelFilter::TRACE)
        .init();

    Ok(())
}
#[tokio::main]
async fn main() {
    if let Err(e) = main_with_new().await {
        error!("Error: {}", e);
        eprint!("{}", e);
    }
}
async fn main_with_error() -> Result<(), anyhow::Error> {
    setup_logger()?;
    let cli = Cli::parse();
    info!("Loading configuration from: {}", cli.config_file);
    let config_str = tokio::fs::read_to_string(&cli.config_file).await?;
    let config: Config = serde_yaml::from_str(&config_str)?;
    let mut records: HashMap<Name, RData> = HashMap::new();
    for (name_str, record_config) in config.records {
        let name = Name::from_utf8(&name_str)?;
        let rdata = match record_config.record_type.to_uppercase().as_str() {
            "A" => RData::A(record_config.value.parse::<Ipv4Addr>()?.into()),
            "CNAME" => RData::CNAME(hickory_proto::rr::rdata::CNAME(Name::from_ascii(
                &record_config.value,
            )?)),
            _ => {
                warn!(
                    "Unsupported record type '{}' for {}",
                    record_config.record_type, name_str
                );
                continue;
            }
        };
        info!("Loaded local record: {} -> {:?}", name_str, rdata);
        records.insert(name, rdata);
    }

    let mut forwarders = Vec::new();
    for upstream_str in config.upstream_servers {
        let upstream_addr: SocketAddr = upstream_str.parse()?;
        let conn = UdpClientStream::builder(upstream_addr, TokioRuntimeProvider::default()).build();
        let (client, bg) = Client::connect(conn).await.unwrap();

        tokio::spawn(bg);
        forwarders.push((upstream_str.clone(), client));
        info!("Upstream DNS server configured: {}", upstream_str);
    }

    if forwarders.is_empty() {
        return Err(anyhow::anyhow!("No upstream servers configured!"));
    }

    let handler = MyRequestHandler {
        records: Arc::new(records),
        forwarders: Arc::new(forwarders),
    };

    let addr = format!("0.0.0.0:{}", config.port);
    let socket = UdpSocket::bind(addr).await?;

    println!("Listening on: {}", socket.local_addr()?);

    let mut server = hickory_server::server::ServerFuture::new(handler);
    server.register_socket(socket);

    server.block_until_done().await?;

    Ok(())
}
async fn main_with_new() -> Result<(), anyhow::Error> {
    setup_logger()?;
    let cli = Cli::parse();
    info!("Loading configuration from: {}", cli.config_file);
    let config_str = tokio::fs::read_to_string(&cli.config_file).await?;
    let config: NewConfig = serde_yaml::from_str(&config_str)?;
    let pipline_handler = PipelineHandler::new(config.clone()).await;

    let addr = format!("0.0.0.0:{}", config.port);
    let socket = UdpSocket::bind(addr).await?;

    println!("Listening on: {}", socket.local_addr()?);

    let mut server = hickory_server::server::ServerFuture::new(pipline_handler);
    server.register_socket(socket);

    server.block_until_done().await?;

    Ok(())
}
