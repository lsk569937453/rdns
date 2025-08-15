mod handler;
use crate::handler::request_handler::MyRequestHandler;
use hickory_client::client::Client;
use hickory_proto::rr::Name;
use hickory_proto::rr::RData;
use hickory_proto::runtime::TokioRuntimeProvider;
use hickory_proto::udp::UdpClientStream;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tracing_appender::rolling;
use tracing_subscriber::Layer;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
#[macro_use]
extern crate tracing;
#[macro_use]
extern crate anyhow;
fn setup_logger(log_to_file: bool) -> Result<(), anyhow::Error> {
    if log_to_file {
        let app_file = rolling::daily("./logs", "access.log");

        let file_layer = tracing_subscriber::fmt::Layer::new()
            .with_target(true)
            .with_ansi(false)
            .with_writer(app_file)
            .with_filter(tracing_subscriber::filter::LevelFilter::INFO);

        tracing_subscriber::registry()
            .with(file_layer)
            .with(tracing_subscriber::filter::LevelFilter::INFO)
            .init();
    }
    Ok(())
}
#[tokio::main]
async fn main() {
    if let Err(e) = main_with_error().await {
        error!("Error: {}", e);
    }
}
async fn main_with_error() -> Result<(), anyhow::Error> {
    let mut records: HashMap<Name, RData> = HashMap::new();
    records.insert(
        Name::from_utf8("example.com.")?,
        RData::A(Ipv4Addr::new(127, 0, 0, 1).into()),
    );
    records.insert(
        Name::from_utf8("www.example.com.")?,
        RData::CNAME(hickory_proto::rr::rdata::CNAME(Name::from_ascii(
            "example.com.",
        )?)),
    );

    let upstream_addr = "8.8.8.8:53".parse()?;
    let conn = UdpClientStream::builder(upstream_addr, TokioRuntimeProvider::default()).build();
    let (client, bg) = Client::connect(conn).await.unwrap();

    tokio::spawn(bg);

    let handler = MyRequestHandler {
        records: Arc::new(records),
        forwarder: client,
    };

    let addr = "127.0.0.1:5353";
    let socket = UdpSocket::bind(addr).await?;
    println!("Listening on: {}", socket.local_addr()?);

    let mut server = hickory_server::server::ServerFuture::new(handler);
    server.register_socket(socket);

    server.block_until_done().await?;

    Ok(())
}
