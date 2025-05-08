use bytes::{BufMut, BytesMut};
use log::{error, trace};
use serde::{
    de::{self, Error},
    Deserialize,
};
use serde_bytes::ByteBuf;
use std::{
    fmt::{Display, Write},
    fs::write,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use crate::{HashedId20, PeerId20};

#[derive(Debug)]
pub enum TrackerError {
    FailedToDecode(bendy::serde::error::Error),
}

impl From<bendy::serde::error::Error> for TrackerError {
    fn from(e: bendy::serde::error::Error) -> Self {
        Self::FailedToDecode(e)
    }
}

impl Display for TrackerError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::FailedToDecode(e) => e.fmt(f),
        }
    }
}

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug)]
pub struct TrackerRequest {
    pub info_hash: HashedId20,
    pub peer_id: PeerId20,
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
    pub fn encode_http_get(&self, announce: String) -> BytesMut {
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

        write!(buf, "Host: {}\r\n", announce).unwrap();

        buf.put_slice(b"\r\n");
        buf
    }
}

// The tracker responds with "text/plain" document consisting of a bencoded dictionary with the following keys:
#[derive(Debug, Deserialize)]
pub struct TrackerResponse {
    #[serde(rename = "warning message")]
    pub warning_message: Option<ByteBuf>,
    pub complete: u64,
    pub interval: u64,
    #[serde(rename = "min interval")]
    pub min_interval: Option<u64>,
    #[serde(rename = "tracker_id")]
    pub tracker_id: Option<ByteBuf>,
    pub incomplete: u64,
    #[serde(deserialize_with = "deserialize_peers")]
    pub peers: Vec<SocketAddr>,
}

// Custom Deserialization for Peers, should support compact format
fn deserialize_peers<'de, D>(deserializer: D) -> Result<Vec<SocketAddr>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct Visitor;
    impl<'de> de::Visitor<'de> for Visitor {
        type Value = Vec<SocketAddr>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter
                .write_str("bencoded dictionary or compact byte representation of list of peers")
        }
        // Deserialize Compact Format
        fn visit_bytes<E>(self, b: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            trace!("Deserializing peers in compact format.");
            if b.len() % 6 != 0 {
                return Err(E::custom(TrackerError::FailedToDecode(
                    bendy::serde::error::Error::CustomDecode(
                        "Compact peers must be a multiple of 6.".into(),
                    ),
                )));
            }
            let mut peers = Vec::new();
            for chunk in b.chunks_exact(6) {
                let ip_addr = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
                let port: u16 = u16::from_be_bytes([chunk[4], chunk[5]]);
                let peer = SocketAddr::new(IpAddr::V4(ip_addr), port);
                peers.push(peer);
            }
            Ok(peers)
        }

        // Deserialize bencoded dictionary
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            trace!("Deserializing peers in dictionary format.");
            #[derive(Debug, Deserialize)]
            struct TempPeer {
                ip: String,
                port: u16,
            }
            // size_hint() gets the size if included in the SeqAccess
            let mut peers = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(TempPeer { ip, port }) = seq.next_element()? {
                let ip = match ip.parse() {
                    Ok(v) => v,
                    _ => {
                        error!("Could not parse bencoded struct");
                        continue;
                    }
                };
                peers.push(SocketAddr::new(ip, port));
            }
            Ok(peers)
        }
    }
    deserializer.deserialize_any(Visitor)
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
        dbg!(tr.encode_http_get("test".into()));
    }
}
