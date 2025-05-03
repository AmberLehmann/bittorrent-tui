use bytes::{BufMut, BytesMut};
use std::fmt::{Display, Write};

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
            write!(buf, "&event={}", current_event).unwrap();
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
            write!(buf, "&ip={}", addr).unwrap();
        }
        if let Some(num) = self.numwant {
            write!(buf, "&numwant={}", num).unwrap();
        }
        if let Some(k) = &self.key {
            write!(buf, "&key={}", k).unwrap();
        }
        if let Some(t) = &self.trackerid {
            write!(buf, "&key={}", t).unwrap();
        }
        buf.put_slice(b" HTTP/1.1\r\n");
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    #[test]
    fn tracker_1() {
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
        let bytes = tr.encode_http_get();
        let output = std::str::from_utf8(&bytes).unwrap().to_string();
        println!("{output}\n len:{}", bytes.len());
    }
}
