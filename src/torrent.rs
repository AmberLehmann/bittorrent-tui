use crate::{
    handshake::Handshake,
    messages::Message,
    metainfo::{Info, MetaInfo},
    popup::OpenTorrentResult,
    tracker::{PeerInfo, TrackerError, TrackerRequest, TrackerRequestEvent, TrackerResponse},
    HashedId20, PeerId20,
};
use bitvec::{
    order::Msb0,
    prelude::{BitSlice, BitVec},
};
use byteorder::{ByteOrder, NetworkEndian};
use bytes::{Buf, BytesMut};
use log::{debug, error, info};
use rand::{distr::Alphanumeric, random_range, Rng};
use regex::Regex;
use sha1::{Digest, Sha1};
use std::{
    fmt::Display,
    fs::File,
    io::Read,
    net::{SocketAddr, ToSocketAddrs},
    sync::{Arc, Mutex, RwLock},
    task::{Context, Poll},
    time::Instant,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
    time::{timeout}
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

fn generate_peer_id() -> PeerId20 {
    let prefix = b"AmogusBT";
    let rand_part: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .collect();

    let mut peer_id = [0u8; 20];
    peer_id[..8].copy_from_slice(prefix);
    peer_id[8..].copy_from_slice(rand_part.as_bytes());
    peer_id
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
    pub pieces_data: Arc<Vec<RwLock<Vec<u8>>>>,
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
                    pieces_to_download_data
                        .push(RwLock::new(Vec::with_capacity(this_piece_len as usize)));

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
                    my_peer_id: generate_peer_id(), // do better?
                    local_addr: SocketAddr::new(torrent.ip, torrent.port),
                    compact: torrent.compact,
                    pieces_info: Arc::new(Mutex::new(pieces_to_download_info)),
                    pieces_data: Arc::new(pieces_to_download_data),
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
    let mut request = TrackerRequest {
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
    let mut response = TrackerResponse::new(&mut buf)?;

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

    let (torrent_tx, peer_rx) = unbounded_channel();
    let handshake_msg = Handshake::new(torrent.info_hash, torrent.my_peer_id).serialize_handshake();
    let peer_handlers: Vec<(JoinHandle<_>, UnboundedSender<TcpStream>)> = response
        .peers
        .iter()
        .take(30)
        .filter(|&p| p.addr != torrent.local_addr)
        .map(|p| {
            let msg = handshake_msg.clone();
            let (tx, rx) = unbounded_channel();
            (
                tokio::spawn(peer_handler(
                    p.addr,
                    p.peer_id.clone(),
                    torrent.meta_info.info.piece_length(),
                    torrent.pieces_info.clone(),
                    torrent.pieces_data.clone(),
                    msg,
                    torrent_tx.clone(),
                    rx,
                )),
                tx,
            )
        })
        .collect();

    let tick_rate = std::time::Duration::from_millis(50);
    let mut interval = tokio::time::interval(tick_rate);
    let mut last_request = Instant::now();
    let mut buf = BytesMut::with_capacity(1024);
    loop {
        let delay = interval.tick();
        // TODO: make actually async like peer_handler
        tokio::select! {
            tracker_resp = tracker_stream.read_buf(&mut buf) => {
                match tracker_resp {
                    Ok(0) => {
                        // error!("Tracker closed connection!");
                    },
                    Ok(bytes_read) => {
                        response = TrackerResponse::new(&mut buf)?;
                        // Potentially assigned new trackers, want to update!
                        continue;
                    },
                    Err(e) => Err(e)?,
                }

                buf.clear();
            },
            Ok((peer_stream, peer_addr)) = listener.accept() => {
                // a peer is trying to connect to us
                info!("{peer_addr} attempting to connect");
                // TODO - call handler
            },
            _ = delay => {
                // request.gen_periodic_req(uploaded, downloaded, left);
                if last_request.elapsed() > std::time::Duration::from_secs(response.interval) {
                    last_request = Instant::now();
                    let mut http_msg = request.encode_http_get(torrent.meta_info.announce.clone());
                    tracker_stream.write_all_buf(&mut http_msg).await?; // NOTE: Causes Pipe Error (BAD)
                }
                // send periodic update to tracker
            },
        }
    }
}

enum PeerMsg {
    Closed,
    Have(u32),
    Field(BitVec<u8, Msb0>),
}

// needs some way for the main thread to signal that this task should be cancled. needs signal to
// ensure this task does not hold the lock or maybe just ensuring that the lock is released before
// the next await is enough?
async fn peer_handler(
    addr: SocketAddr,
    peer_id: Option<Vec<u8>>,
    piece_size: usize,
    pieces_info: Arc<Mutex<Vec<PieceInfo>>>,
    pieces_data: Arc<Vec<RwLock<Vec<u8>>>>,
    mut handshake_msg: BytesMut,
    tx: UnboundedSender<PeerMsg>,
    mut rx: UnboundedReceiver<TcpStream>,
) -> Result<(), tokio::io::Error> {
    let mut peer = ConnectedPeer {
        addr,
        id: peer_id,
        out_stream: TcpStream::connect(addr).await?,
        in_stream: None,
        am_choking: true,
        peer_choking: true,
        am_interested: false,
        peer_interested: false,
        bitfield: BitVec::new(),
        upload_rate: 0.0,
        download_rate: 0.0,
    };
    let mut last_response = Instant::now();
    info!("connected to {addr}");

    let block_size: usize = 1 << 16; // handle slightly bigger size? (i set it to 16 not 15)
    let mut incoming_stream: Option<TcpStream> = None; // what is this for??????????????????????
    let mut len_buf = [0u8; 4];
    let mut stream_buf = vec![0u8; block_size + 50];
    let mut piece_buf = vec![0u8; piece_size];

    // perform handshake
    while handshake_msg.has_remaining() {
        peer.out_stream.write_all_buf(&mut handshake_msg).await?;
    }
    // handshake_msg.resize(68, 0);
    let mut response = [0u8; 68];
    peer.out_stream.read_exact(&mut response).await?;
    info!("Received Handshake Response from: {}", addr);
    let mut peer_id_response: PeerId20 = [0u8; 20];
    peer_id_response.copy_from_slice(&response[48..68]);
    if let Some(remote_peer_vec) = peer.id {
        log::debug!("Length of remote peer_id: {}", remote_peer_vec.len());
        log::debug!("Remote peer_id vec: {:?}", remote_peer_vec);
        log::debug!("Peer_id response: {:?}", peer_id_response);
        if remote_peer_vec != peer_id_response {
            error!("Peer ID not matching in dictionary format!");
        }
        // TODO: Verify peer_id_response
    }

    // Handshake successful, increment number of peers
    // go into main loop
    // randomized timeout
    let tick_rate = std::time::Duration::from_millis(random_range(90..120));
    let mut interval = tokio::time::interval(tick_rate);
    loop {
        let delay = interval.tick();
        tokio::select! {
            // ????????? why is this called out_stream, what is in_stream
            _ = peer.out_stream.readable() => {

                // NOTE: we use several awaits here
                // *must make sure all locks are let go!*

                // either respond to request or
                // timeout on reads is 2 seconds now
                match timeout(tokio::time::Duration::from_secs(2), peer.out_stream.peek(&mut len_buf)).await {
                    Err(e) => {
                        debug!("would block poll on peek v1: {e}");
                        continue;
                    }
                    Ok(Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        debug!("would block poll on peek v2: {e}");
                        continue;
                    },
                    Ok(Err(e)) => {
                        error!("peek error: {e}");
                        return Err(e);
                    },
                    Ok(Ok(0)) => {
                        // stream has likely been closed
                        debug!("Stream closed");
                        let _ = tx.send(PeerMsg::Closed);
                        return Ok(());
                    }
                    Ok(Ok(n)) if n < 4 => {
                        debug!("not enough data to read message length: {n} bytes available");
                        continue;
                    }
                    Ok(Ok(_)) => {
                        // read len_buf
                        let msg_len = (NetworkEndian::read_u32(&len_buf) as usize) + 4;
                        // prepare to consume at least that much
                        if stream_buf.len() < msg_len + 4 {
                            stream_buf.resize(msg_len + 4, 0u8);
                        }

                        // must read in exactly that many bytes, otherwise
                        // will overread stream (TCP, not UDP)
                        // so read into a slice of a certain size? i think that should be fine
                        // timeout on reads is 2 seconds now
                        match timeout(tokio::time::Duration::from_secs(2), peer.out_stream.read_exact(&mut (stream_buf[0..msg_len+4]))).await {
                            Err(e) => {
                                debug!("would block poll on read v1: {e}");
                                continue;
                            }
                            Ok(Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                debug!("would block poll on read v2: {e}");
                                continue;
                            },
                            Ok(Err(e)) => {
                                error!("read error: {e}");
                                return Err(e);
                            },
                            Ok(Ok(_bytes_read)) => {
                                // parse and handle response message from peer
                                let Ok(msg) = Message::parse(&(stream_buf[0..msg_len+4])) else { continue };
                                debug!("read {msg:?}");

                                // not every message can be recieved from this connection but good to
                                // include them anyways
                                match msg {
                                    Message::KeepAlive => last_response = Instant::now(),
                                    Message::Choke => peer.peer_choking = true,
                                    Message::UnChoke => peer.peer_choking = false,
                                    Message::Interested => peer.peer_interested = true,
                                    Message::NotInterested => peer.peer_interested = false,
                                    Message::Have(h) => {
                                        // update global torrent piece map
                                        let _ = tx.send(PeerMsg::Have(h.piece_index));
                                    },
                                    Message::Bitfield(b) => {
                                        // update global torrent piece map
                                        peer.bitfield = b.bitfield.to_bitvec();
                                        let _ = tx.send(PeerMsg::Field(peer.bitfield.clone()));
                                    },
                                    Message::Request(r) => {
                                        // check if we have the piece then send it if we do and if we arent
                                        // choking this peer
                                    },
                                    Message::Piece(p) => {
                                        piece_buf[p.begin as usize..p.begin as usize + p.block.len()].copy_from_slice(p.block);
                                        // TODO: mark the bounds of this section as being filed
                                    },
                                    Message::Cancel(c) => {},
                                    Message::Port(_p) => {
                                        // we dont support DHT so we can ignore this message
                                    },
                                    Message::Unknown => {
                                        log::error!("Received unknown message type.");
                                    },
                                }
                            }
                        }
                    }
                }
            },
            Some(new_peer) = rx.recv() => {
                incoming_stream = Some(new_peer);
            }
            Some(_) = readable_tcpstream(&incoming_stream) => {
                let Some(ref s) = incoming_stream else { continue };
                // our peer is making a request or sending an update
            }
            _ = delay => {
                // if we arent currently waiting for a reponse back from our peer and they arent
                // choking us then claim one of the next rarest pieces and request it.
                // debug!("Delay");
                // set a timer and if the request takes too long or cancle it and update info so
                // another task has a chance to claim it
                continue;
            },
        }
    }
}

async fn readable_tcpstream(stream: &Option<TcpStream>) -> Option<()> {
    match stream {
        Some(s) => s.readable().await.ok(),
        None => None,
    }
}

#[derive(Debug)]
pub struct ConnectedPeer {
    pub addr: SocketAddr,
    pub id: Option<Vec<u8>>,
    pub out_stream: TcpStream,
    pub in_stream: Option<TcpStream>,

    pub am_choking: bool,
    pub am_interested: bool,
    pub peer_choking: bool,
    pub peer_interested: bool,

    pub bitfield: BitVec<u8, Msb0>,
    pub upload_rate: f64,
    pub download_rate: f64,
}
