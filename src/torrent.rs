use crate::{
    handshake::Handshake,
    metainfo::{Info, MetaInfo},
    popup::OpenTorrentResult,
    tracker::{PeerInfo, TrackerError, TrackerRequest, TrackerRequestEvent, TrackerResponse},
    HashedId20, PeerId20,
};
use bytes::Buf;
use log::info;
use rand::{random_range, rng, Rng};
use regex::Regex;
use sha1::{Digest, Sha1};
use std::{
    fmt::Display,
    fs::File,
    io::Read,
    net::{SocketAddr, ToSocketAddrs},
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
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
    Confirmed,
}

#[derive(Copy, Clone)]
pub enum BlockStatus {
    NotRequested,
    Requested,
    Confirmed,
}

#[derive(Copy, Clone)]
pub struct BlockInfo {
    status: BlockStatus,
    offset: u32,
    length: u32,
}

#[derive(Clone)]
pub struct PieceInfo {
    status: PieceStatus,
    needed_requests: Vec<BlockInfo>,
    num_havers: u32,
}

#[derive(Clone)]
pub struct Torrent {
    pub meta_info: MetaInfo,
    pub tracker_addr: SocketAddr,
    pub announce_path: String,
    pub scrape_path: Option<String>,
    pub status: TorrentStatus,
    pub info_hash: HashedId20,
    pub my_peer_id: PeerId20,
    pub compact: bool,
    pub local_addr: SocketAddr,
    // the data in the u8 vec, the status, the length that we know about
    pub pieces_info: Arc<Mutex<Vec<PieceInfo>>>,
    pub pieces_data: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Torrent {
    pub fn open(torrent: OpenTorrentResult) -> Result<Torrent, OpenTorrentError> {
        let hostname_regex =
            Regex::new(r"(?P<proto>https?|udp)://(?P<name>[^/]+)(?P<path>.*)").unwrap();
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
                // get number of pieces to download
                let num_pieces = (info_stuff.length / info_stuff.piece_length) as usize;

                let mut pieces_to_download_info = Vec::new();
                let mut pieces_to_download_data = Vec::new();
                for piece_index in 0..num_pieces {
                    // how long is this piece
                    let mut this_piece_len = info_stuff.piece_length;
                    if piece_index == num_pieces - 1 {
                        this_piece_len = info_stuff.length % info_stuff.piece_length;
                    }
                    let this_piece_len = this_piece_len as u32;
                    // prep spot to put that data
                    pieces_to_download_data.push(Vec::with_capacity(this_piece_len as usize));

                    // what do we know about this piece

                    // vector of blocks we need to request
                    // with the offset within the piece
                    // and the length of the request
                    // such that each block is 16KB as much as possible
                    // except the last one which should be "the rest"
                    let mut this_needed_requests: Vec<BlockInfo> = Vec::new();
                    let mut curr_offset: u32 = 0;
                    let block_size: u32 = 1 << 14; // 2^14 or 16KB
                    while curr_offset < this_piece_len {
                        let remaining = this_piece_len - curr_offset;
                        let block_len = remaining.min(block_size); // smaller of intended block size and the remaining amount
                        this_needed_requests.push(BlockInfo {
                            status: (BlockStatus::NotRequested),
                            offset: (curr_offset),
                            length: (block_len),
                        });
                        curr_offset += block_len;
                    }

                    pieces_to_download_info.push(PieceInfo {
                        status: PieceStatus::NotRequested,
                        needed_requests: this_needed_requests,
                        num_havers: 0,
                    });
                }

                Ok(Torrent {
                    status: TorrentStatus::Waiting,
                    meta_info: new_meta,
                    tracker_addr: ip,
                    announce_path: new_announce_path,
                    scrape_path: new_scrape_path,
                    info_hash,
                    my_peer_id: rng().random(), // do better?
                    local_addr: SocketAddr::new(torrent.ip, torrent.port),
                    compact: torrent.compact,
                    pieces_info: Arc::new(Mutex::new(pieces_to_download_info)),
                    pieces_data: Arc::new(Mutex::new(pieces_to_download_data)),
                })
            }
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
        peer_id: torrent.my_peer_id,
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
        numwant: None, // temp default, give user ability to choose
        key: Some("rustyclient".into()),
        trackerid: None, // If a previous announce contained a tracker id, it should be set here.
    };
    let mut tracker_stream = TcpStream::connect(torrent.tracker_addr).await?;
    let http_msg = request.encode_http_get(torrent.meta_info.announce.clone());

    tracker_stream.write_all(&http_msg[..]).await?;
    info!("Sent initial request to tracker.");
    let mut buf: Vec<u8> = vec![];
    tracker_stream.read_to_end(&mut buf).await?;
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
    // pub pieces_downloaded: Vec<(Option<Vec<u8>>, PieceStatus, usize)>
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

    log::debug!("Binding listening socket at {}", torrent.local_addr);
    let listener = TcpListener::bind(torrent.local_addr).await?;
    log::debug!("Server is listening.");

    //let pieces_info_arc_clone = Arc::clone(&torrent.pieces_info);
    //let pieces_data_arc_clone = Arc::clone(&torrent.pieces_data);

    // accept incoming connections + process connection inputs/outputs
    // spawn listening thread? that will constantly loop on things??
    // keep track of total connections somehow?? so we don't go above 55 TODO

    let peer_handlers: Vec<JoinHandle<_>> = response
        .peers
        .iter()
        .take(30)
        .map(|p| {
            tokio::spawn(peer_handler(
                p.addr,
                p.peer_id.clone(),
                torrent.meta_info.info.piece_length(),
                torrent.pieces_info.clone(),
                torrent.info_hash,
                torrent.my_peer_id,
            ))
        })
        .collect();

    let mut buf = Vec::new();
    let tick_rate = std::time::Duration::from_millis(50);
    let mut interval = tokio::time::interval(tick_rate);
    loop {
        let delay = interval.tick();
        // TODO: make actually async like peer_handler
        tokio::select! {
            tracker_resp = tracker_stream.read(&mut buf) => {
                match tracker_resp {
                    _ => {},
                }
            },
            _ = delay => {
                // send periodic update to tracker
            },
        }
    }

    //    match listener.accept().await {
    //        Ok((stream, addr)) => {
    //            let info_hash = info_hash_clone.clone();
    //            let my_peer_id = my_peer_id_clone.clone();
    //            let pieces_info_arc = Arc::clone(&pieces_info_arc_clone);
    //            let pieces_data_arc = Arc::clone(&pieces_data_arc_clone);
    //            // is going to need to somehow choose next thing to request (lock on some vec about that?)
    //            // is going to need to somehow update the piece and trigger an audit(?) if the piece is done(?)
    //            // how can we tell - need to save the amount of the piece downloaded somehow
    //            // is also going to need to be able to update the connected peer struct for choking and stuff???
    //
    //            tokio::spawn(async move {
    //                handle_incoming_peer(
    //                    stream,
    //                    addr,
    //                    info_hash,
    //                    my_peer_id,
    //                    pieces_info_arc,
    //                    pieces_data_arc,
    //                )
    //                .await; // receive, kill, etc.
    //            });
    //        }
    //        Err(e) => {
    //            log::error!("Failed to accept incoming connection: {}", e);
    //        }
    //    }
    //}

    //let info_hash_clone = torrent.info_hash.clone();
    //let my_peer_id_clone = torrent.my_peer_id.clone();
    //let pieces_info_arc_clone = Arc::clone(&torrent.pieces_info);
    //let pieces_data_arc_clone = Arc::clone(&torrent.pieces_data);
    //
    //// connect to at most 30 peers from the TrackerResponse
    //for peer in response.peers.into_iter().take(30) {
    //    let info_hash = info_hash_clone.clone();
    //    let my_peer_id = my_peer_id_clone.clone();
    //    let pieces_info_arc = Arc::clone(&pieces_info_arc_clone);
    //    let pieces_data_arc = Arc::clone(&pieces_data_arc_clone);
    //    // keep track of total connections somehow?? so we don't go above 55 TODO
    //    let new_handle = tokio::spawn(async move {
    //        match TcpStream::connect(peer.addr).await {
    //            Ok(mut stream) => {
    //                handle_outgoing_peer(
    //                    stream,
    //                    peer,
    //                    info_hash,
    //                    my_peer_id,
    //                    pieces_info_arc,
    //                    pieces_data_arc,
    //                )
    //                .await; // receive, kill, etc.
    //            }
    //            Err(e) => {
    //                log::error!("Failed to connect to {}: {}", peer.addr, e);
    //            }
    //        }
    //    });
    //}

    // FUCK will also need to somehow Arc Mutex reference some set of connections
    // so that we can do shit with unchoking and stuff
    // i can't do this right now sorry

    // periodic updates to tracker
    // needs to be its own thread
    // TODO

    // someone needs to be checking periodically if we are done downloading?? at which point we kill everything and
    // tell the tracker I guess?
    // i guess the selection algorithm maybe can do that

    //Ok(())
}

// needs some way for the main thread to signal that this task should be cancled. needs signal to
// ensure this task does not hold the lock or maybe just ensuring that the lock is released before
// the next await is enough?
async fn peer_handler(
    addr: SocketAddr,
    remote_peer_id: Option<Vec<u8>>,
    piece_size: usize,
    pieces_info: Arc<Mutex<Vec<PieceInfo>>>,
    info_hash: HashedId20,
    local_peer_id: PeerId20,
) -> Result<(), tokio::io::Error> {
    let block_size: usize = 1 << 15;
    let mut stream: TcpStream = TcpStream::connect(addr).await?;
    let mut stream_buf = Vec::with_capacity(block_size + 50);
    let mut piece_buf: Vec<u8> = Vec::with_capacity(piece_size);
    let tick_rate = std::time::Duration::from_millis(random_range(30..50));
    let mut interval = tokio::time::interval(tick_rate);

    // perform handshake
    let mut handshake_msg = Handshake::new(info_hash, local_peer_id).serialize_handshake();
    while handshake_msg.has_remaining() {
        stream.write_all_buf(&mut handshake_msg).await?;
    }
    handshake_msg.resize(68, 0);
    log::debug!(
        "Capacity of buf after writing: {}",
        handshake_msg.capacity()
    );
    stream.read_exact(&mut handshake_msg).await?;
    log::debug!("Received handshake response!");
    let mut peer_id_response: PeerId20 = [0u8; 20];
    handshake_msg
        .split_off(48)
        .copy_to_slice(&mut peer_id_response);
    if let Some(remote_peer_vec) = remote_peer_id {
        log::trace!("Length of remote peer_id: {}", remote_peer_vec.len());
        // TODO: Verify peer_id_response
    }
    // Handshake successful, increment number of peers

    // go into main loop
    loop {
        let delay = interval.tick();
        tokio::select! {
            _ = stream.readable() => {
                // either respond to request or
                match stream.try_read(&mut stream_buf) {
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {},
                    Ok(0) => {
                        // stream has likely been closed
                    }
                    Ok(bytes_read) => {
                        // parse and handle message from peer
                    }
                    Err(e) => Err(e)?,
                }
            },
            _ = delay => {
                // if we arent currently waiting for a reponse back from our peer and they arent
                // choking us then claim one of the next rarest pieces and request it.

                // set a timer and if the request takes too long or cancle it and update info so
                // another task has a chance to claim it
            },
        }
    }
}

#[derive(Debug)]
pub struct ConnectedPeer {
    pub addr: SocketAddr,
    pub peer_id: Option<Vec<u8>>,
    pub stream: TcpStream,

    pub am_choking: bool,
    pub am_interested: bool,
    pub peer_choking: bool,
    pub peer_interested: bool,

    pub their_bitfield: Vec<u8>,
    pub upload_rate: f64,
    pub download_rate: f64,
}

pub async fn handle_incoming_peer(
    stream: TcpStream,
    addr: SocketAddr,
    info_hash: HashedId20,
    my_peer_id: PeerId20,
    pieces_info_arc: Arc<Mutex<Vec<PieceInfo>>>,
    pieces_data_arc: Arc<Mutex<Vec<Vec<u8>>>>,
) {
    todo!();
}

pub async fn handle_outgoing_peer(
    stream: TcpStream,
    peer: PeerInfo,
    info_hash: HashedId20,
    my_peer_id: PeerId20,
    pieces_info_arc: Arc<Mutex<Vec<PieceInfo>>>,
    pieces_data_arc: Arc<Mutex<Vec<Vec<u8>>>>,
) {
    todo!();
}
