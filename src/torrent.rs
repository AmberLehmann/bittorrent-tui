use crate::metainfo::{Info, MetaInfo, SingleFileInfo};
use log::{error, info, trace};
use regex::Regex;
use std::{
    fmt::{write, Display},
    fs::File,
    io::{Read, Stdout},
    net::{SocketAddr, ToSocketAddrs},
};

#[derive(Debug)]
pub enum OpenTorrentError {
    BadTrackerURL,
    UnableToResolve,
    UDPTracker,
    MultiFile,
    FailedToOpen(std::io::Error),
    FailedToDecode(serde_bencode::Error),
}

impl Display for OpenTorrentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadTrackerURL => write!(f, "Torrent provides malformed tracker URL."),
            Self::UnableToResolve => write!(f, "Unable to resolve hostname from ip address."),
            Self::UDPTracker => write!(f, "UDP trackers are not currently supported."),
            Self::MultiFile => write!(f, "Multi-file mode is currently not supported."),
            Self::FailedToOpen(e) => write!(f, "Failed to open Torrent: {e}"),
            Self::FailedToDecode(e) => write!(f, "Failed to decode Torrent: {e}"),
        }
    }
}

pub enum TorrentStatus {
    Waiting,   // signifies that this torrent is awaiting a response from the tracker
    Connected, // connection has been established with the tracker
}

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

        let new_meta: MetaInfo =
            serde_bencode::from_bytes(&data).map_err(OpenTorrentError::FailedToDecode)?;

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
