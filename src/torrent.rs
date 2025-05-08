use crate::handshake::HashedId20;
use crate::metainfo::{Info, MetaInfo, SingleFileInfo};
use crate::tracker::{TrackerRequest, TrackerRequestEvent};
use crate::{HashedId20, PeerId20};
use gethostname::gethostname;
use local_ip_address::local_ip;
use rand::{rng, Rng};
use sha1::{Digest, Sha1};

use log::{debug, error, info, trace};
use regex::Regex;
use std::{
    fmt::{write, Display},
    fs::File,
    io::{Read, Stdout},
    net::{SocketAddr, ToSocketAddrs},
    sync::mpsc::{Receiver, Sender},
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
        let ip = match format!("{}:80", hostname.as_str()).to_socket_addrs() {
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
            }),
            Info::Multi(_) => Err(OpenTorrentError::MultiFile),
        }
    }
}

pub fn handle_torrent(torrent: Torrent, tx: Sender<TorrentInfo>, rx: Receiver<TorrentStatus>) {
    let Ok(local_ip_v4) = local_ip() else {
        error!("Unable to get local IPv4");
        return;
    };
    // TODO: Get 20 byte Sha1 hash from info key in metainfo
    let info_hash: HashedId20 = rng().random();
    let peer_id: PeerId20 = rng().random();
    debug!("{:?}", peer_id);

    // TODO: Construct TrackerRequest
    let request = TrackerRequest {
        info_hash,
        peer_id,
        event: Some(TrackerRequestEvent::Started),
        port: 6881, // Temp hardcoded
        uploaded: 0,
        downloaded: 0,
        left: 0,
        compact: true,
        no_peer_id: false,     // Ignored for compact
        ip: Some(local_ip_v4), // Temp default to ipv4, give user ability for ipv6
        numwant: None,         // temp default, give user ability to choose
        key: Some("rustyclient".into()),
        trackerid: None, // If a previous announce contained a tracker id, it should be set here.
    };
    let http_message = request.encode_http_get();

    error!("torrent thread not implemented");

    //unimplemented!();
}
