use crate::{
    metainfo::{Info, MetaInfo},
    popup::OpenTorrentResult,
    tracker::{TrackerError, TrackerRequest, TrackerRequestEvent, TrackerResponse},
    HashedId20,
};
use log::info;
use rand::{rng, Rng};
use regex::Regex;
use sha1::{Digest, Sha1};
use std::{
    fmt::Display,
    fs::File,
    io::Read,
    net::{SocketAddr, ToSocketAddrs},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
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

impl From<std::io::Error> for OpenTorrentError {
    fn from(value: std::io::Error) -> Self {
        OpenTorrentError::FailedToOpen(value)
    }
}

impl From<bendy::serde::error::Error> for OpenTorrentError {
    fn from(value: bendy::serde::error::Error) -> Self {
        OpenTorrentError::FailedToDecode(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TorrentStatus {
    Waiting,     // signifies that this torrent is awaiting a response from the tracker
    Connected,   // connection has been established with the tracker
    Downloading, // connected and downloading/seeding
    Seeding,     // connected, Ddwnload completed, and seeding
    Paused,      // conected?
}

impl Display for TorrentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TorrentStatus::Waiting => "Waiting",
                TorrentStatus::Connected => "Connected",
                TorrentStatus::Downloading => "Downloading",
                TorrentStatus::Seeding => "Seeding",
                TorrentStatus::Paused => "Paused",
            }
        )
    }
}

pub struct TorrentInfo {
    pub size: u64,
    pub progress: u8,
    pub status: TorrentStatus,
    pub seeds: u8,
    pub peers: u8,
    pub speed: u64,
}

#[derive(Copy, Clone)]
pub enum PieceStatus {
    NotRequested,
    Requested,
    NotConfirmed,
    Confirmed
}

#[derive(Clone)]
pub struct PieceInfo {
    data: Vec<u8>,
    status: PieceStatus,
    length: i64
}

#[derive(Clone)]
pub struct Torrent {
    pub meta_info: MetaInfo,
    pub tracker_addr: SocketAddr,
    pub announce_path: String,
    pub scrape_path: Option<String>,
    pub status: TorrentStatus,
    pub info_hash: HashedId20,
    pub compact: bool,
    pub local_addr: SocketAddr,
    // the data in the u8 vec, the status, the length that we know about
    pub pieces_downloaded: Vec<PieceInfo>
}

impl Torrent {
    pub fn open(torrent: OpenTorrentResult) -> Result<Torrent, OpenTorrentError> {
        let hostname_regex = Regex::new(r"(?P<proto>https?|udp)://(?P<name>[^/]+)(?P<path>.*)").unwrap();
        let mut file = File::open(&torrent.path)?;

        let mut data = Vec::new();
        let bytes_read = file.read_to_end(&mut data)?;
        info!("open_torrent() read {} bytes", bytes_read);

        // extract info hash
        let mut decoder = bendy::decoding::Decoder::new(&data);
        // get top level dictionary
        let Ok(Some(bendy::decoding::Object::Dict(mut metainfo_dict))) = decoder.next_object()
        else {
            return Err(OpenTorrentError::MissingInfoDict);
        };

        let mut info_hash = HashedId20::default();
        // search for the info key
        while let Ok(Some(pair)) = metainfo_dict.next_pair() {
            if b"info" == pair.0 {
                let bendy::decoding::Object::Dict(info_dict) = pair.1 else {
                    Err(OpenTorrentError::MissingInfoDict)?
                };
                let raw_info_bytes = info_dict
                    .into_raw()
                    .or(Err(OpenTorrentError::MissingInfoDict))?;
                let mut hasher = Sha1::new();
                hasher.update(raw_info_bytes);
                info_hash = hasher.finalize().into();
            }
        }

        let new_meta: MetaInfo = bendy::serde::from_bytes(&data)?;

        info!("Tracker address: {}", new_meta.announce);
        let caps = hostname_regex
            .captures(&new_meta.announce)
            .ok_or(OpenTorrentError::BadTrackerURL)?;

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
        let ip = addr?.next().ok_or(OpenTorrentError::UnableToResolve)?;

        // get url paths
        let mut new_announce_path: String = caps.name("path").unwrap().as_str().to_owned();
        if new_announce_path.is_empty() {
            new_announce_path = "/".to_owned();
        }
        log::debug!("new_announce_path: {}", new_announce_path);
        let mut new_scrape_path: Option<String> = None;
        if let Some(pos_slash) = new_announce_path.rfind('/') {
            if let Some(ann_slice) = new_announce_path.get(pos_slash..) {
                if ann_slice.starts_with("/announce") {
                    let before = &new_announce_path[..pos_slash];
                    let after = &new_announce_path[(pos_slash + 9)..]; // "/announce".len()
                    let new_scrape_string = format!("{}/scrape{}", before, after);
                    new_scrape_path = Some(new_scrape_string);
                }
            }
        };
        log::debug!("new_scrape_path: {:?}", new_scrape_path);

        match &new_meta.info {
            Info::Single(info_stuff) => {
                // pieces to download 
                let num_pieces = (info_stuff.length / info_stuff.piece_length) as usize;
                let mut pieces_to_download = Vec::with_capacity(num_pieces);
                for _ in 0..num_pieces {
                    pieces_to_download.push(PieceInfo {
                        data: vec![0u8; info_stuff.piece_length as usize], 
                        status: PieceStatus::NotRequested,
                        length: 0 // to be updated as we learn about the size of this piece (not sure if useful or not but i think it will be)
                    });
                }

                Ok(
                    Torrent {
                        status: TorrentStatus::Waiting,
                        meta_info: new_meta,
                        tracker_addr: ip,
                        announce_path: new_announce_path,
                        scrape_path: new_scrape_path,
                        info_hash,
                        local_addr: SocketAddr::new(torrent.ip, torrent.port),
                        compact: torrent.compact,
                        pieces_downloaded: pieces_to_download
                    }
                )
            },
            Info::Multi(_) => Err(OpenTorrentError::MultiFile),
        }
    }

    pub fn get_info(&self) -> TorrentInfo {
        let size = match &self.meta_info.info {
            Info::Single(f) => f.length,
            Info::Multi(_) => unreachable!(),
        };

        TorrentInfo {
            size,
            progress: 0,
            status: self.status,
            seeds: 0,
            peers: 0,
            speed: 0,
        }
    }
}

pub async fn handle_torrent(
    torrent: Torrent,
    _tx: UnboundedSender<TorrentInfo>,
    _rx: UnboundedReceiver<TorrentStatus>,
) -> Result<(), TrackerError> {
    let left = match &torrent.meta_info.info {
        Info::Multi(_) => return Err(TrackerError::MultiFile),
        Info::Single(f) => f.length,
    };
    let request = TrackerRequest {
        info_hash: torrent.info_hash,
        peer_id: rng().random(),
        event: Some(TrackerRequestEvent::Started),
        port: torrent.local_addr.port(),
        uploaded: 0,
        downloaded: 0,
        left,
        compact: torrent.compact, // tested both compact/non-compact deserialization
        no_peer_id: false,        // Ignored for compact
        // ip: Some(local_ipv4), // Temp default to ipv4, give user ability for ipv6
        ip: Some(torrent.local_addr.ip()), // Temp default to ipv4, give user ability for ipv6
        announce_path: torrent.announce_path,
        numwant: None,                     // temp default, give user ability to choose
        key: Some("rustyclient".into()),
        trackerid: None, // If a previous announce contained a tracker id, it should be set here.
    };
    let mut stream = TcpStream::connect(torrent.tracker_addr).await?;
    let http_msg = request.encode_http_get(torrent.meta_info.announce.clone());

    stream.write_all(&http_msg[..]).await?;
    info!("Sent initial request to tracker.");
    let mut buf: Vec<u8> = vec![];
    stream.read_to_end(&mut buf).await?;
    let header_end = match buf.windows(4).position(|window| window == b"\r\n\r\n") {
        Some(pos) => pos + 4, // Account for \r\n\r\n
        None => return Err(TrackerError::MalformedHttpResponse),
    };
    let response: TrackerResponse = bendy::serde::from_bytes(&buf[header_end..])?;
    info!(
        "Tracker response received: client assigned {} peers.",
        response.peers.len()
    );

    // TODO
    // talk to peers now

    // ok so our torrent has
    // pub pieces_downloaded: Vec<{Option<Vec<u8>>, PieceStatus, usize}>
    // pub local_addr: SocketAddr,

    // and the response has
    // pub peers: Vec<PeerInfo>
    // pub interval: u64
    // pub min_interval: Option<u64>
    // pub tracker_id: Option<ByteBuf>
    // where 
    // pub struct PeerInfo {
    //     pub addr: SocketAddr,
    //     pub peer_id: Option<Vec<u8>>
    // }

    // "the official client version 3 in fact only actively forms new connections 
    // if it has less than 30 peers and will refuse connections if it has 55."
    // so start listening and being prepeated to accept incoming connections
    // and connect to 30 of the peers from the response

    // we also need to be periodically making requests to the tracker
    // based on the interval from the tracker response 
    // which have info like uploaded, downloaded, left
    // as well as "event" started/stopped/completed
    // presumably we should send stopped when we finish
    // unless we'd violate min interval(?)

    // for each connection we form, send a handshake
    // expect a handshake back
    // make sure if there is a peer_id, it should match the peer_id we're 
    // expecting from this connection based on the tracker info

    // for each connection we accept, expect a handshake 
    // read in the handshake up to the info hash
    // if the info hash does not match, close the connection
    // if it does, be prepared that you may or may not get a peer_id as well and deal with that as needed(?)
    // then send a handshake back

    // once the handshake happens, we optionally send a bitfield
    // and/or optionally expect a bitfield
    // and must drop if it doesn't match expectations
    // "optional" meaning don't send if you have no pieces
    // if you have pieces you must send it lol

    // therefore we need to create a bitfield:
    // TODO: create bitfield from pieces_downloaded in clever way

    // ok so make a structure here(?) for connections
    // holding ConnectedPeer type values
    // start with the 30 of them, both sides choking and both sides interested

    // then I assume we send "interested" if they have stuff I don't have based on the bitfield??
    // and we wait for them to unchoke us
    // once this is true we can send them requests
    
    // we need to send keep-alives every <2 minutes

    // we need to choke/unchoke/interested/not interested
    // based on Choking and Optimistic Unchoking
    // blocks are uploaded when we are not choking them, and they are interested in us
    // assumption: we can request things when they are not choking us and we are interested in them

    // we need to send "have"s whenever we get a new piece fully

    // we need to send requests for things of up to size 2^14(?) based on the downloading strategy
    // when we receive responses to these, calculate upload rate?
    // must set a timeout of 60secs for "anti-snubbing"
    // in which case mark the piece as open to be requested by another thread
    // and for anti-snubbing maybe(?) mark this guy as choked and do ???? i don't really get this
    // i guess mark them as having a shit upload rate or something? idk

    // we need to respond to requests for things, and/or not based on a "cancel"
    //      we are advised to keep a few (10ish?) unfullfilled requests on each connection??



    // ec: enter endgame mode eventually
    
    // done talking to peers

    Ok(())
}

#[derive(Debug)]
pub struct ConnectedPeer {
    pub addr: SocketAddr,
    pub peer_id: Option<Vec<u8>>,
    pub am_choking: u8,
    pub am_interested: u8,
    pub peer_choking: u8,
    pub peer_interested: u8
}
