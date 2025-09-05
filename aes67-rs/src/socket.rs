/*
 *  Copyright (C) 2025 Michael Bachmann
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published by
 *  the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use crate::{
    config::SocketConfig,
    error::{ConfigError, ConfigResult, ReceiverInternalResult, SenderInternalResult},
};
use miette::{IntoDiagnostic, Result};
use sdp::{
    SessionDescription,
    description::common::{Address, ConnectionInformation},
};
use socket2::{Domain, Protocol as SockProto, SockAddr, Socket, TcpKeepalive, Type};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener},
    time::Duration,
};
use tokio::net::UdpSocket;
use tracing::{info, instrument};

#[instrument]
pub fn init_tcp_socket(bind_addr: IpAddr, port: u16, config: SocketConfig) -> Result<TcpListener> {
    let addr = format!("{bind_addr}:{port}");
    let addr: SocketAddr = addr.parse().into_diagnostic()?;

    let mut tcp_keepalive = TcpKeepalive::new();
    if let Some(keepalive) = config.keepalive_time {
        tcp_keepalive = tcp_keepalive.with_time(keepalive);
    }
    if let Some(keepalive) = config.keepalive_interval {
        tcp_keepalive = tcp_keepalive.with_interval(keepalive);
    }
    if let Some(retries) = config.keepalive_retries {
        tcp_keepalive = tcp_keepalive.with_retries(retries);
    }
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(SockProto::TCP)).into_diagnostic()?;

    socket.set_reuse_address(true).into_diagnostic()?;
    socket.set_nonblocking(true).into_diagnostic()?;
    socket.set_keepalive(true).into_diagnostic()?;
    socket.set_tcp_keepalive(&tcp_keepalive).into_diagnostic()?;
    socket
        .set_tcp_user_timeout(config.user_timeout)
        .into_diagnostic()?;
    socket.set_tcp_nodelay(true).into_diagnostic()?;
    socket.bind(&SockAddr::from(addr)).into_diagnostic()?;
    socket.listen(1024).into_diagnostic()?;
    let listener = socket.into();

    Ok(listener)
}

#[instrument]
pub fn create_rx_socket(
    sdp: &SessionDescription,
    local_ip: IpAddr,
) -> ReceiverInternalResult<UdpSocket> {
    Ok(try_create_rx_socket(sdp, local_ip)?)
}

fn try_create_rx_socket(sdp: &SessionDescription, local_ip: IpAddr) -> ConfigResult<UdpSocket> {
    let global_c = sdp.connection_information.as_ref();

    if sdp.media_descriptions.len() > 1 {
        return Err(ConfigError::InvalidSdp(
            "redundant streams aren't supported yet".to_owned(),
        ));
    }

    let media = if let Some(media) = sdp.media_descriptions.first() {
        media
    } else {
        return Err(ConfigError::InvalidSdp(
            "media description is missing".to_owned(),
        ));
    };

    if media.media_name.media != "audio" {
        return Err(ConfigError::InvalidSdp(format!(
            "unsupported media type: {}",
            media.media_name.media
        )));
    }

    if !(media.media_name.protos.contains(&"RTP".to_owned())
        && media.media_name.protos.contains(&"AVP".to_owned()))
    {
        return Err(ConfigError::InvalidSdp(format!(
            "unsupported media protocols: {:?}; only RTP/AVP is supported",
            media.media_name.protos
        )));
    }

    let c = media.connection_information.as_ref().or(global_c);

    let c = if let Some(c) = c {
        c
    } else {
        return Err(ConfigError::InvalidSdp(
            "connection data is missing".to_owned(),
        ));
    };

    let ConnectionInformation {
        network_type,
        address_type,
        address,
    } = c;

    let address = if let Some(address) = address {
        address
    } else {
        return Err(ConfigError::InvalidSdp(
            "connection-address is missing".to_owned(),
        ));
    };

    if address_type != "IP4" && address_type != "IP6" {
        return Err(ConfigError::InvalidSdp(format!(
            "unsupported addrtype: {address_type}"
        )));
    }

    if network_type != "IN" {
        return Err(ConfigError::InvalidSdp(format!(
            "unsupported nettype: {network_type}"
        )));
    }

    let Address { address, .. } = address;

    // TODO for unicast addresses check if the IP exists on this machine and reject otherwise
    // TODO for IPv4 check if the TTL allows packets to reach this machine and reject otherwise

    let mut split = address.split('/');
    let ip = split.next();
    let prefix = split.next();
    let ip_addr: IpAddr = if let (Some(ip), Some(_prefix)) = (ip, prefix) {
        ip.parse()?
    } else {
        return Err(ConfigError::InvalidSdp(format!(
            "invalid ip address: {address}"
        )));
    };

    let port = media.media_name.port.value.to_owned() as u16;

    let socket = match (ip_addr, local_ip) {
        (IpAddr::V4(ipv4_addr), IpAddr::V4(local_ip)) => {
            create_ipv4_rx_socket(ipv4_addr, local_ip, port)?
        }
        (IpAddr::V6(ipv6_addr), IpAddr::V6(local_ip)) => {
            create_ipv6_rx_socket(ipv6_addr, local_ip, port)?
        }
        (IpAddr::V4(_), IpAddr::V6(_)) => Err(ConfigError::InvalidLocalIP(
            "Cannot receive IPv4 stream when bound to local IPv6 address".to_owned(),
        ))?,
        (IpAddr::V6(_), IpAddr::V4(_)) => Err(ConfigError::InvalidLocalIP(
            "Cannot receive IPv6 stream when bound to local IPv4 address".to_owned(),
        ))?,
    };
    socket.set_nonblocking(true)?;

    Ok(UdpSocket::from_std(socket.into())?)
}

#[instrument]
pub fn create_tx_socket(local_ip: IpAddr, port: u16) -> SenderInternalResult<std::net::UdpSocket> {
    let socket = match local_ip {
        IpAddr::V4(_) => Socket::new(Domain::IPV4, Type::DGRAM, Some(SockProto::UDP)),
        IpAddr::V6(_) => Socket::new(Domain::IPV6, Type::DGRAM, Some(SockProto::UDP)),
    }?;
    let socket_addr = SockAddr::from(SocketAddr::new(local_ip, port));
    socket.bind(&socket_addr)?;
    socket.set_broadcast(true)?;
    socket.set_reuse_address(true)?;
    // socket.set_nonblocking(true)?;

    Ok(socket.into())
}

#[instrument]
pub fn create_ipv4_rx_socket(
    ip_addr: Ipv4Addr,
    local_ip: Ipv4Addr,
    port: u16,
) -> ConfigResult<Socket> {
    info!(
        "Creating IPv4 {} RX socket for stream {}:{} at {}:{}",
        if ip_addr.is_multicast() {
            "multicast"
        } else {
            "unicast"
        },
        ip_addr,
        port,
        local_ip,
        port
    );

    let local_addr = SocketAddr::new(IpAddr::V4(local_ip), port);

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(SockProto::UDP))?;

    socket.set_reuse_address(true)?;

    if ip_addr.is_multicast() {
        socket.join_multicast_v4(&ip_addr, &local_ip)?;
        socket.bind(&SockAddr::from(SocketAddr::new(IpAddr::V4(ip_addr), port)))?;
    } else {
        socket.bind(&SockAddr::from(local_addr))?;
    }
    Ok(socket)
}

#[instrument]
pub fn create_ipv6_rx_socket(
    ip_addr: Ipv6Addr,
    local_ip: Ipv6Addr,
    port: u16,
) -> ConfigResult<Socket> {
    info!(
        "Creating IPv6 {} RX socket for stream {}:{} at {}:{}",
        if ip_addr.is_multicast() {
            "multicast"
        } else {
            "unicast"
        },
        ip_addr,
        port,
        local_ip,
        port
    );

    let local_addr = SocketAddr::new(IpAddr::V6(local_ip), port);

    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(SockProto::UDP))?;

    socket.set_reuse_address(true)?;
    socket.set_read_timeout(Some(Duration::from_millis(250)))?;

    if ip_addr.is_multicast() {
        socket.join_multicast_v6(&ip_addr, 0)?;
        socket.bind(&SockAddr::from(SocketAddr::new(IpAddr::V6(ip_addr), port)))?;
    } else {
        socket.bind(&SockAddr::from(local_addr))?;
    }
    Ok(socket)
}
