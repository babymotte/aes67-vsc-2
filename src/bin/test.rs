use std::{
    io::Cursor,
    net::{IpAddr, Ipv4Addr},
};

use aes67_vsc_2::{receiver::config::ReceiverConfig, vsc::VirtualSoundCardApi};
use miette::{IntoDiagnostic, Result};
use sdp::SessionDescription;

const SDP: &str = "v=0
o=- 10943522194 10943522227 IN IP4 192.168.178.97
s=AVIO-Bluetooth : 2
i=2 channels: Left, Right
c=IN IP4 239.69.232.56/32
t=0 0
a=keywds:Dante
a=recvonly
m=audio 5004 RTP/AVP 97
a=rtpmap:97 L24/48000/2
a=ptime:1
a=ts-refclk:ptp=IEEE1588-2008:00-1D-C1-FF-FE-0E-10-C4:0
a=mediaclk:direct=0
";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let vsc = VirtualSoundCardApi::new("test-vsc".to_owned()).await?;
    let session = SessionDescription::unmarshal(&mut Cursor::new(SDP)).into_diagnostic()?;
    let (receiver, id) = vsc
        .create_receiver(ReceiverConfig {
            buffer_time: 100.0,
            link_offset: 420.0,
            interface_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 178, 39)),
            id: "alsa-1".to_owned(),
            delay_calculation_interval: None,
            session,
        })
        .await?;

    eprintln!("{id}: {:?}", receiver.info().await?);

    Ok(())
}
