use bytes::{BufMut, Bytes, BytesMut};
use std::{
    fmt::{Display, Write},
    net::SocketAddr,
};

#[derive(Clone, Copy)]
pub enum TrackerRequestEvent {
    Started,
    Stopped,
    Completed,
}

impl Display for TrackerRequestEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TrackerRequestEvent::Started => "started",
                TrackerRequestEvent::Stopped => "stopped",
                TrackerRequestEvent::Completed => "completed",
            }
        )
    }
}

// TODO: Probably want to create a type with traits
type HashedId20 = [u8; 20];

pub struct TrackerRequest {
    pub info_hash: HashedId20,
    pub peer_id: HashedId20,
    pub event: Option<TrackerRequestEvent>,
    pub port: u16,
    pub uploaded: u64,
    pub downloaded: u64,
    pub left: u64,
    pub compact: bool,
    pub no_peer_id: bool,
    pub ip: Option<std::net::IpAddr>,
    pub numwant: Option<usize>,
    pub key: Option<Box<str>>,
    pub trackerid: Option<Box<str>>,
}

impl TrackerRequest {
    fn encode_http_get(&self) -> BytesMut {
        use urlencoding::encode_binary;
        let mut buf = BytesMut::with_capacity(512);
        write!(
            buf,
            "GET /announce?info_hash={}&peer_id={}",
            encode_binary(&self.info_hash),
            encode_binary(&self.peer_id)
        )
        .unwrap();
        if let Some(current_event) = self.event {
            write!(buf, "&event={current_event}").unwrap();
        }
        write!(
            buf,
            "&port={}&uploaded={}&downloaded={}&left={}&compact={}&no_peer_id={}",
            self.port,
            self.uploaded,
            self.downloaded,
            self.left,
            if self.compact { 1 } else { 0 },
            if self.no_peer_id { 1 } else { 0 }
        )
        .unwrap();
        if let Some(addr) = self.ip {
            write!(buf, "&ip={addr}").unwrap();
        }
        if let Some(num) = self.numwant {
            write!(buf, "&numwant={num}").unwrap();
        }
        if let Some(k) = &self.key {
            write!(buf, "&key={k}").unwrap();
        }
        if let Some(t) = &self.trackerid {
            write!(buf, "&key={t}").unwrap();
        }
        buf.put_slice(b" HTTP/1.1\r\n");
        buf
    }
}

// TODO: Add traits
pub struct Peers(Vec<SocketAddr>);

pub struct TrackerResponse {
    pub warning_message: Option<Bytes>,
    pub complete: u64,
    pub interval: u64,
    pub min_interval: Option<u64>,
    pub tracker_id: Option<Bytes>,
    pub incomplete: u64,
    pub peers: Peers,
}

// NOTE: Use `cargo test -- --show-output`.
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    #[test]
    fn encode_1() {
        let tr = TrackerRequest {
            info_hash: *b"12345678901234567890",
            peer_id: *b"ABCDEFGHIJKLMNOPQRST",
            event: Some(TrackerRequestEvent::Started),
            port: 8090,
            uploaded: 200,
            downloaded: 200,
            left: 100,
            compact: false,
            no_peer_id: false,
            ip: Some(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
            numwant: Some(2),
            key: Some("secret".into()),
            trackerid: Some("trackster".into()),
        };
        dbg!(tr.encode_http_get());
    }
}
