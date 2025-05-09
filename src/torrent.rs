use crate::metainfo::{Info, MetaInfo, SingleFileInfo};
use crate::tracker::{TrackerError, TrackerRequest, TrackerRequestEvent, TrackerResponse};
use crate::{HashedId20, PeerId20};
use local_ip_address::local_ip;
use log::{debug, error, info};
use rand::{rng, Rng};
use regex::Regex;
use sha1::{Digest, Sha1};
use std::{
    fmt::Display,
    fs::File,
    io::{Read, Write},
    net::{SocketAddr, ToSocketAddrs},
    sync::mpsc::{Receiver, Sender},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

#[derive(Debug)]
pub enum OpenTorrentError {
    BadTrackerURL,
    UnableToResolve,
    UDPTracker,
    MultiFile,
    FailedToOpen(std::io::Error),
    FailedToDecode(bendy::serde::error::Error),
    MissingInfoDict,
}

impl Display for OpenTorrentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadTrackerURL => write!(f, "Torrent provides malformed tracker URL."),
            Self::UnableToResolve => write!(f, "Unable to resolve hostname from ip address."),
            Self::UDPTracker => write!(f, "UDP trackers are not currently supported."),
            Self::MultiFile => write!(f, "Multi-file mode is currently not supported."),
            Self::MissingInfoDict => write!(f, "Unable to locate the info dictionary."),
            Self::FailedToOpen(e) => write!(f, "Failed to open Torrent: {e}"),
            Self::FailedToDecode(e) => write!(f, "Failed to decode Torrent: {e}"),
        }
    }
}

impl std::error::Error for OpenTorrentError {}

#[derive(Clone, Debug)]
pub enum TorrentStatus {
    Waiting,   // signifies that this torrent is awaiting a response from the tracker
    Connected, // connection has been established with the tracker
}

pub struct TorrentInfo {
    pub size: u64,
    pub progress: u8,
    pub status: TorrentStatus,
    pub seeds: u8,
    pub peers: u8,
    pub speed: u64,
}

#[derive(Clone)]
pub struct Torrent {
    pub meta_info: MetaInfo,
    pub tracker_addr: SocketAddr,
    pub status: TorrentStatus,
    pub info_hash: HashedId20,
    //pieces_downloaded: Vec<bool>,
}

impl Torrent {
    pub fn open(path: &str) -> Result<Torrent, OpenTorrentError> {
        let hostname_regex = Regex::new(r"(?P<proto>https?|udp)://(?P<name>[^/]+)").unwrap();
        let mut file = File::open(path).map_err(OpenTorrentError::FailedToOpen)?;

        let mut data = Vec::new();
        let bytes_read = file.read_to_end(&mut data);
        info!("open_torrent() read {:?} bytes", bytes_read.unwrap_or(0));

        // extract info hash
        let mut decoder = bendy::decoding::Decoder::new(&data);
        // get top level dictionary
        let Ok(Some(bendy::decoding::Object::Dict(mut metainfo_dict))) = decoder.next_object()
        else {
            return Err(OpenTorrentError::MissingInfoDict);
        };

        let mut info_hash: HashedId20 = [0u8; 20];
        // search for the info key
        while let Ok(Some(pair)) = metainfo_dict.next_pair() {
            if b"info" == pair.0 {
                let bendy::decoding::Object::Dict(info_dict) = pair.1 else {
                    return Err(OpenTorrentError::MissingInfoDict);
                };
                let raw_info_bytes = info_dict
                    .into_raw()
                    .or(Err(OpenTorrentError::MissingInfoDict))?;
                let mut hasher = Sha1::new();
                hasher.update(raw_info_bytes);
                info_hash = hasher.finalize().into();
            }
        }

        let new_meta: MetaInfo =
            bendy::serde::from_bytes(&data).map_err(OpenTorrentError::FailedToDecode)?;

        info!("Tracker address: {}", new_meta.announce);
        let Some(caps) = hostname_regex.captures(&new_meta.announce) else {
            return Err(OpenTorrentError::BadTrackerURL);
        };

        let proto = caps.name("proto").unwrap();
        if proto.as_str() == "udp" {
            return Err(OpenTorrentError::UDPTracker);
        }

        let hostname = caps.name("name").unwrap();
        let addr = if hostname.as_str().contains(':') {
            hostname.as_str().to_socket_addrs()
        } else {
            format!("{}:80", hostname.as_str()).to_socket_addrs()
        };

        let ip = match addr {
            Ok(mut ip_iter) => ip_iter.next().ok_or(OpenTorrentError::UnableToResolve)?,
            Err(_e) => {
                return Err(OpenTorrentError::BadTrackerURL);
            }
        };

        match new_meta.info {
            Info::Single(_) => Ok(Torrent {
                status: TorrentStatus::Waiting,
                meta_info: new_meta,
                tracker_addr: ip,
                info_hash,
            }),
            Info::Multi(_) => Err(OpenTorrentError::MultiFile),
        }
    }
}

pub async fn handle_torrent(
    torrent: Torrent,
    tx: Sender<TorrentInfo>,
    rx: Receiver<TorrentStatus>,
) -> Result<(), TrackerError> {
    // let local_ipv4 = local_ip()?;
    let left = match &torrent.meta_info.info {
        Info::Multi(_) => return Err(TrackerError::MultiFile),
        Info::Single(f) => f.length,
    };
    let request = TrackerRequest {
        info_hash: torrent.info_hash,
        peer_id: rng().random(),
        event: Some(TrackerRequestEvent::Started),
        port: 6881, // Temp hardcoded
        uploaded: 0,
        downloaded: 0,
        left,
        compact: true,     // tested both compact/non-compact deserialization
        no_peer_id: false, // Ignored for compact
        // ip: Some(local_ipv4), // Temp default to ipv4, give user ability for ipv6
        ip: None,      // Temp default to ipv4, give user ability for ipv6
        numwant: None, // temp default, give user ability to choose
        key: Some("rustyclient".into()),
        trackerid: None, // If a previous announce contained a tracker id, it should be set here.
    };
    let mut stream = TcpStream::connect(torrent.tracker_addr)
        .await
        .map_err(TrackerError::Async)?;
    let http_msg = request.encode_http_get(torrent.meta_info.announce.clone());

    stream
        .write_all(&http_msg[..])
        .await
        .map_err(TrackerError::Async)?;
    info!("Sent initial request to tracker.");
    let mut buf: Vec<u8> = vec![];
    stream
        .read_to_end(&mut buf)
        .await
        .map_err(TrackerError::Async)?;
    let header_end = buf.windows(4).position(|window| window == b"\r\n\r\n");
    let header_end = match header_end {
        Some(pos) => pos + 4, // Account for \r\n\r\n
        None => return Err(TrackerError::MalformedHttpResponse),
    };
    let response: TrackerResponse = bendy::serde::from_bytes(&buf[header_end..])?;
    info!("Received initial tracker response");
    debug!("{:?}", response);

    Ok(())
}
