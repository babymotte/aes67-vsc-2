use aes67_vsc_2::{
    config::{Config, WebServerConfig},
    error::Aes67Vsc2Result,
    playout::config::PlayoutConfig,
    receiver::config::ReceiverConfig,
};
use sdp::SessionDescription;
use std::{
    io::Cursor,
    net::{IpAddr, Ipv4Addr},
};

const SDP_SINGLE: &str = include_str!("../../test/single.sdp");
// const SDP_REDUNDANT: &str = include_str!("../../test/redundant.sdp");

#[tokio::main(flavor = "current_thread")]
async fn main() -> Aes67Vsc2Result<()> {
    let session =
        SessionDescription::unmarshal(&mut Cursor::new(SDP_SINGLE)).expect("invalid example SDP");

    let mut config = Config::load().await?;

    config.app.name = "AES67-VSC-2 Receiver".to_owned();

    let mut webserver = WebServerConfig::default();
    webserver.bind_address = IpAddr::V4(Ipv4Addr::LOCALHOST);
    webserver.port = 32000;

    config.receiver_config = Some(ReceiverConfig {
        webserver,
        session,
        link_offset: 4.0,
        buffer_overhead: 10.0,
        interface_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 178, 39)),
    });

    let mut webserver = WebServerConfig::default();
    webserver.bind_address = IpAddr::V4(Ipv4Addr::LOCALHOST);
    webserver.port = 32001;

    config.playout_config = Some(PlayoutConfig {
        webserver,
        receiver: "http://127.0.0.1:32000".to_owned(),
    });

    println!(
        "{}",
        serde_yaml::to_string(&config).expect("could not serialize config")
    );
    Ok(())
}
