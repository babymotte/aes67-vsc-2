use aes67_vsc_2::{
    receiver::config::RxDescriptor,
    socket::create_rx_socket,
    time::{MediaClock, SystemMediaClock},
    utils::U32_WRAP,
};
use rtp_rs::RtpReader;
use sdp::SessionDescription;
use std::io::Cursor;
use tokio::{select, signal};

const SDP: &str = "v=0\r\no=- 18311622000 18311622024 IN IP4 192.168.178.114\r\ns=XCEL-1201 : 32\r\ni=2 channels: DANTE TX 01, DANTE TX 02\r\nc=IN IP4 239.69.224.56/32\r\nt=0 0\r\na=keywds:Dante\r\na=recvonly\r\nm=audio 5004 RTP/AVP 97\r\na=rtpmap:97 L24/48000/2\r\na=ptime:1\r\na=ts-refclk:ptp=IEEE1588-2008:00-1D-C1-FF-FE-0E-10-C4:0\r\na=mediaclk:direct=0\r\n";
const LOCAL_IP: &str = "192.168.178.39";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let session = SessionDescription::unmarshal(&mut Cursor::new(SDP)).expect("invalid SDP");
    let desc = RxDescriptor::new("minirec".to_owned(), &session, 1.0).expect("invalid session");

    let clock = SystemMediaClock::new(desc.audio_format);

    let sock = create_rx_socket(&session, LOCAL_IP.parse().expect("invalid IP"))
        .expect("could not create port");

    let mut buf = [0u8; 1500];

    loop {
        select! {
            Ok(len) = sock.recv(&mut buf) => received(&buf[..len],  clock.current_media_time().expect("could not get system clock"), &desc),
            _ = signal::ctrl_c() => break,
        }
    }
}

fn received(data: &[u8], current_media_time: u64, desc: &RxDescriptor) {
    let rtp = match RtpReader::new(data) {
        Ok(it) => it,
        Err(e) => {
            eprintln!("Received invalid RTP packet: {e:?}");
            return;
        }
    };

    let wrapped_media_time = current_media_time % U32_WRAP;
    let delay_frames = wrapped_media_time - rtp.timestamp() as u64;
    let delay_usec = delay_frames * 1_000_000 / desc.audio_format.sample_rate as u64;
    let delay_packets = delay_frames as f32
        / (rtp.payload().len() as f32 / desc.audio_format.frame_format.bytes_per_frame() as f32);
    eprintln!(
        "Network delay: {delay_frames} frames / {delay_usec} µs / {delay_packets:.1} packets"
    );
}
