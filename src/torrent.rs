use crate::{
    handshake::Handshake,
    hashing::hash_buffer,
    messages::{self, Message},
    metainfo::{Info, MetaInfo},
    popup::OpenTorrentResult,
    tracker::{PeerInfo, TrackerError, TrackerRequest, TrackerRequestEvent, TrackerResponse},
    HashedId20, PeerId20, HANDSHAKE_LEN, PROTOCOL_V_1,
};
use bitvec::{
    order::Msb0,
    prelude::{BitSlice, BitVec},
};
use byteorder::{ByteOrder, NetworkEndian};
use bytes::{Buf, BytesMut};
use log::{debug, error, info, warn};
use rand::{
    distr::Alphanumeric, random_range, seq::index::sample_weighted, seq::IteratorRandom, Rng,
};
use regex::Regex;
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    fmt::Display,
    fs::File,
    io::{Cursor, Read, Write},
    net::{SocketAddr, ToSocketAddrs},
    sync::{Arc, Mutex, RwLock},
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
    time::timeout,
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

#[derive(Clone)]
pub struct TorrentInfo {
    pub size: u64,
    pub progress: u8,
    pub status: TorrentStatus,
    pub seeds: u8,
    pub peers: u8,
    pub speed: u64,
}

#[derive(Copy, Clone, PartialEq, Eq)]
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
    hash: HashedId20,
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
                        hash: new_meta.info.pieces()[piece_index..piece_index + 20]
                            .try_into()
                            .unwrap(),
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
    tx: UnboundedSender<TorrentInfo>,
    _rx: UnboundedReceiver<TorrentStatus>,
) -> Result<(), TrackerError> {
    let demo = false;

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
        announce_path: torrent.announce_path.clone(),
        numwant: None, // temp default, give user ability to choose
        key: Some("rustyclient".into()),
        trackerid: None, // If a previous announce contained a tracker id, it should be set here.
    };
    let mut status_info = torrent.get_info();
    let mut tracker_stream = TcpStream::connect(torrent.tracker_addr).await?;
    let http_msg = request.encode_http_get(torrent.meta_info.announce.clone());

    tracker_stream.write_all(&http_msg[..]).await?;
    info!("Sent initial request to tracker.");
    let mut buf: Vec<u8> = vec![];
    tracker_stream.read_to_end(&mut buf).await?;
    let mut response = TrackerResponse::new(&mut buf)?;

    // Start listening

    // to prevent duplicate connections if we can
    let known_peers = Arc::new(Mutex::new(HashMap::<PeerId20, SocketAddr>::new()));
    let mut pieces_downloaded = 0;

    log::debug!("Binding listening socket at {}", torrent.local_addr);
    let listener = TcpListener::bind(torrent.local_addr).await?;
    log::debug!("Server is listening.");

    // accept incoming connections + process connection inputs/outputs
    // spawn listening thread? that will constantly loop on things??
    // keep track of total connections somehow?? so we don't go above 55 TODO

    let (torrent_tx, mut peer_rx) = unbounded_channel();
    let handshake_msg = Handshake::new(torrent.info_hash, torrent.my_peer_id).serialize_handshake();
    let mut peer_handlers: HashMap<SocketAddr, HandleEntry> = response
        .peers
        .iter()
        .take(30)
        .filter(|&p| {
            p.addr != torrent.local_addr || (demo && p.addr.ip() == torrent.local_addr.ip())
        }) // change addr to ip + change to eq
        .map(|p| {
            let msg = handshake_msg.clone();
            let (cmd_tx, cmd_rx): (UnboundedSender<PeerCommand>, UnboundedReceiver<PeerCommand>) =
                unbounded_channel(); // command channel
            let known_peers_clone = Arc::clone(&known_peers);
            let real_peer_id: Option<PeerId20> = match &p.peer_id {
                Some(bytes) if bytes.len() == 20 => {
                    let slice: &[u8] = bytes.as_ref();
                    Some(slice.try_into().expect("length checked above"))
                }
                Some(bytes) if bytes.len() == 0 => None,
                Some(bytes) => {
                    error!(
                        "Ignoring peer_id with invalid length: {} (expected 20)",
                        bytes.len()
                    );
                    None
                    // TODO - do not connect to this peer!!!
                }
                None => None,
            };

            // for upload rate calculations
            let peer_stats_1: Arc<Mutex<PeerStats>> = Arc::new(Mutex::new(PeerStats {
                is_interested: false,
                upload_rate: 0.0,
            }));
            let new_peer_stats_1 = Arc::clone(&peer_stats_1);

            (
                p.addr,
                HandleEntry {
                    handle: tokio::spawn(peer_handler(
                        p.addr,
                        real_peer_id,
                        torrent.info_hash,
                        torrent.meta_info.info.piece_length(),
                        torrent.pieces_info.clone(),
                        torrent.pieces_data.clone(),
                        msg,
                        torrent_tx.clone(),
                        cmd_rx,
                        known_peers_clone,
                        true,
                        None,
                        new_peer_stats_1,
                    )),
                    sender: cmd_tx,
                    stats: peer_stats_1,
                    is_new: true,
                },
            )
        })
        .collect();

    // tracker requests
    let tick_rate = std::time::Duration::from_millis(50);
    let mut interval = tokio::time::interval(tick_rate);
    let mut tui_interval = tokio::time::interval(Duration::from_millis(500));
    let mut last_request = Instant::now();

    // peer handler commands - optimistic unchoking
    let tick_rate_commands_o = std::time::Duration::from_secs(30);
    let mut interval_commands_o = tokio::time::interval(tick_rate_commands_o);
    let mut last_request_commands_o = Instant::now();
    let mut old_total_peers: usize = 0;
    let mut old_optimistic_unchoke: Option<SocketAddr> = None;

    // peer handler commands - regular unchoking
    let tick_rate_commands_c = std::time::Duration::from_secs(10);
    let mut interval_commands_c = tokio::time::interval(tick_rate_commands_c);
    let mut last_request_commands_c = Instant::now();

    let mut buf = BytesMut::with_capacity(1024);
    loop {
        let delay = interval.tick();
        let tui_update_delay = tui_interval.tick();
        let delay_command_o = interval_commands_o.tick();
        let delay_command_c = interval_commands_c.tick();
        // TODO: make actually async like peer_handler?
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
            _ = tui_update_delay => {
                status_info.progress = (pieces_downloaded * 100 / torrent.pieces_data.len()) as u8;
                tx.send(status_info.clone()).unwrap();
            },
            Ok((peer_stream, peer_addr)) = listener.accept() => {

                // a peer is trying to connect to us
                info!("{peer_addr} attempting to connect");

                // accept incoming peer start
                let msg = handshake_msg.clone();
                let (cmd_tx, cmd_rx)
                    : (UnboundedSender<PeerCommand>, UnboundedReceiver<PeerCommand>)
                    = unbounded_channel(); // command channel
                let known_peers_clone = Arc::clone(&known_peers);

                // for upload rate calculations
                let peer_stats_2 : Arc<Mutex<PeerStats>> = Arc::new(
                    Mutex::new(PeerStats{is_interested: false, upload_rate: 0.0})
                );
                let new_peer_stats_2 = Arc::clone(&peer_stats_2);

                // also maybe it needs a lock? TODO
                peer_handlers.insert(
                    peer_addr,
                    HandleEntry {
                        handle: tokio::spawn(peer_handler(
                        peer_addr,
                        None,
                        torrent.info_hash,
                        torrent.meta_info.info.piece_length(),
                        torrent.pieces_info.clone(),
                        torrent.pieces_data.clone(),
                        msg,
                        torrent_tx.clone(),
                        cmd_rx,
                        known_peers_clone,
                        false,
                        Some(peer_stream),
                        new_peer_stats_2
                        )),
                        sender: cmd_tx,
                        stats: peer_stats_2,
                        is_new: true
                    }
                );

                // accept incoming peer end
            },
            msg = peer_rx.recv() => {
                debug!("Got something on recv from peer channel");
                let msg = msg.ok_or(TrackerError::ChannelClosed)?;
                debug!("That thing was ok");
                match msg {
                    PeerMsg::Have(i) => {
                        debug!("That thing was a have {i}");
                    },
                    PeerMsg::Closed => {
                        debug!("That thing was a closed, clearing from hashmap");
                        peer_handlers.retain(|_, entry| !entry.handle.is_finished());
                    },
                    PeerMsg::Field(f) => {debug!("That thing was a field");},
                    PeerMsg::Confirmed(i) => {
                        debug!("That thing was a confirmed");
                        pieces_downloaded += 1;
                        info!("downloaded piece {}, {}/{}, sending updates to peer threads", i, pieces_downloaded, torrent.pieces_data.len());
                        // send HaveUpdate to all peers
                        for (addr, entry) in &peer_handlers {
                            let _ = entry.sender.send(PeerCommand::HaveUpdate(i as u32));
                            // debug!("Sent HaveUpdate({}) to peer {}", i, addr);
                        }
                    },
                }
                if pieces_downloaded == torrent.pieces_data.len() {
                    let mut f = match torrent.meta_info.info {
                        Info::Single(ref f) => File::create(&f.name),
                        Info::Multi(_) => panic!(),
                    }?;

                    for p_lock in torrent.pieces_data.iter() {
                        let p = p_lock.read().unwrap();
                        f.write_all(&*p)?;
                    }
                }
            }
            _ = delay => {
                // debug!("delay selected");
                // request.gen_periodic_req(uploaded, downloaded, left);
                if last_request.elapsed() > std::time::Duration::from_secs(response.interval) {
                    debug!("delay: generate periodic request");
                    last_request = Instant::now();
                    if request.event.is_some() {
                       request.event = None;
                    }
                    let http_msg = request.encode_http_get(torrent.meta_info.announce.clone());
                    tracker_stream.write_all(&http_msg).await?; // NOTE: Causes Pipe Error (BAD) - maybe change helps?
                }
                // send periodic update to tracker TODO
                // receive it somewhere TODO
            },
            _ = delay_command_o => { // optimistic unchoking timer
                // debug!("delay_command_o selected");
                if last_request_commands_o.elapsed() > std::time::Duration::from_secs(30) {
                    debug!("delay_command_o: doing optimistic unchoking");
                    last_request_commands_o = Instant::now();
                    // signals
                    // do optimistic unchoking
                    // HashMap<SocketAddr, HandleEntry>
                    // where the handle might be dead.
                    // pub struct HandleEntry {
                    //     pub handle: JoinHandle<Result<(), tokio::io::Error>>,
                    //     pub sender: UnboundedSender<PeerCommand>,
                    //     pub stats: Arc<Mutex<PeerStats>>,
                    //     pub is_new: bool
                    // }

                    // clear all dead handles from the hash map
                    // hehe https://users.rust-lang.org/t/filter-vs-retain-which-is-faster-and-why/31912
                    peer_handlers.retain(|_, entry| !entry.handle.is_finished());

                    if peer_handlers.is_empty() {
                        debug!("delay_command_o: wtf");
                        old_optimistic_unchoke = None;
                    } else {
                        // we have access to old_optimistic_unchoke (Option<SocketAddr>) which are both usize
                        // must change from old_optimistic_unchoke,
                        // and prefer things from old_total_peers -> end of vector 3x more than the rest
                        // base this off the is_new value

                        // put into vector (except for the old choke), add weights to vector based on is_new,
                        // choose from it somehow based on this weights rust boook says stufffdwed see belowoewwww

                        // push to vec with so we can sample stuff
                        let mut candidates = Vec::new();
                        for (addr, entry) in peer_handlers.iter() {
                            if Some(*addr) == old_optimistic_unchoke {
                                continue;
                            }
                            candidates.push((addr, entry));
                        }

                        // function for the sample_weighted thing
                        let weight_fn = |i: usize| {
                            let (_, entry) = candidates[i];
                            if entry.is_new {3} else {1}
                        };

                        // what if there's nothing left, we had the 1 connection? that would suck :(
                        if candidates.is_empty() {
                            debug!("delay_command_o: wtf2");
                            // do nothing just keep same old unchoke
                        } else {
                            debug!("delay_command_o: choosing optimistic unchoke");
                            // choose something
                            // rust book says https://docs.rs/rand/latest/rand/seq/index/fn.sample_weighted.html
                            let the_chosen_index = (sample_weighted(&mut rand::rng(), candidates.len(), weight_fn, 1).unwrap()).into_iter().next().unwrap();
                            let (selected_peer, _) = candidates[the_chosen_index];

                            debug!("delay_command_o: chose {}", selected_peer);

                            // send signal to choke old_optimistic_unchoke (Option<SocketAddr>) if there was one
                            if let Some(prev_peer) = old_optimistic_unchoke {
                                if let Some(entry) = peer_handlers.get(&prev_peer) { // this should always be true but uh idk how else
                                    let _ = entry.sender.send(PeerCommand::Choke); // FINALLY
                                }
                            }

                            // send signal to unchoke selected_peer
                            if let Some(entry) = peer_handlers.get(selected_peer) { // this should always be true but uh idk how else
                                let _ = entry.sender.send(PeerCommand::UnChoke);
                            }

                            // set new old_optimistic_unchoke = selected_peer
                            old_optimistic_unchoke = Some(*selected_peer);

                            // set all handle entries to is_new = false
                            for h in peer_handlers.values_mut() {
                                h.is_new = false;
                            }

                            debug!("delay_command_o: done");
                        }
                    }
                }
            },
            _ = delay_command_c => { // regular unchoking timer
                // debug!("delay_command_c selected");
                if last_request_commands_c.elapsed() > std::time::Duration::from_secs(10) {
                    debug!("delay_command_c: doing periodic unchoke");
                    last_request_commands_c = Instant::now();
                    // signals
                    // do regular unchoking
                    // given
                    // pub struct PeerStats {
                    //     pub addr: SocketAddr,
                    //     pub is_interested: bool,
                    //     pub upload_rate: f64
                    // }
                    // HashMap<SocketAddr, HandleEntry>
                    // where the handle might be dead - in which case we should remove it from this vector? unsure

                    // kill old guys
                    peer_handlers.retain(|_, entry| !entry.handle.is_finished());

                    // alright wtf needs to happen
                    // uhhhh

                    // put into vec, sort vec by PeerStats -> upload rate (all - even if not interested)
                    // need to grab all their locks i guess? so that we can update it later
                    let mut relevant_peers : Vec<(SocketAddr, _)> = peer_handlers.iter()
                        .filter_map(|(addr, entry)| {
                            if Some(*addr) == old_optimistic_unchoke { // don't consider the optimistic unchoke
                                None
                            } else {
                                let stats = entry.stats.lock().ok()?; // shouldn't ever fail
                                Some((*addr, stats))
                            }
                        })
                        .collect();
                    // sort by upload rate
                    relevant_peers.sort_by(|a, b| b.1.upload_rate.partial_cmp(&a.1.upload_rate).unwrap_or(std::cmp::Ordering::Equal));

                    // unchoke the 4 peers who *are interested* and have the best upload rate
                    // unchoke all peers with better upload rate than those 4
                    // choke everyone else
                    // aka go through the vec in order, unchoke everyone
                    // until you have unchoked 4 interested peers
                    // then choke everyone else
                    let mut num_unchoked_interested = 0;

                    debug!("delay_command_c: choosing...");

                    for (addr, stats) in &relevant_peers {
                        if let Some(entry) = peer_handlers.get(addr) {
                            if num_unchoked_interested < 4 { // not yet first 4
                                let _ = entry.sender.send(PeerCommand::UnChoke);
                                if stats.is_interested {
                                    num_unchoked_interested += 1;
                                }
                            } else { // rest of them, choke them
                                let _ = entry.sender.send(PeerCommand::Choke);
                            }
                        }
                    }

                    debug!("delay_command_c: sent out chokes/unchokes");

                }
            }
        }
    }
}

#[derive(Debug)]
enum PeerMsg {
    Closed,
    Have(u32),
    Field(BitVec<u8, Msb0>),
    Confirmed(usize),
}

// needs some way for the main thread to signal that this task should be cancled. needs signal to
// ensure this task does not hold the lock or maybe just ensuring that the lock is released before
// the next await is enough?
async fn peer_handler(
    addr: SocketAddr,
    peer_id: Option<PeerId20>,
    info_hash: HashedId20,
    piece_size: usize,
    pieces_info: Arc<Mutex<Vec<PieceInfo>>>,
    pieces_data: Arc<Vec<RwLock<Vec<u8>>>>,
    mut handshake_msg: BytesMut,
    tx: UnboundedSender<PeerMsg>,
    mut cmd_rx: UnboundedReceiver<PeerCommand>,
    known_peers: Arc<Mutex<HashMap<PeerId20, SocketAddr>>>,
    is_outgoing: bool,
    existing_stream: Option<TcpStream>,
    shared_stats: Arc<Mutex<PeerStats>>,
) -> Result<(), tokio::io::Error> {
    let mut peer = ConnectedPeer {
        addr,
        id: peer_id,
        out_stream: if let Some(s) = existing_stream {
            s
        } else {
            TcpStream::connect(addr).await?
        },
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

    // for tracking upload rate
    // (index, begin) of a block request mapped to when it was made
    let mut pending_requests: HashMap<(u32, u32), Instant> = HashMap::new();
    // (bytes in the upload, time passed) for a few recent uploads
    let mut recent_uploads: Vec<(usize, f64)> = Vec::new();

    let block_size: usize = 1 << 14; // handle slightly bigger size? (i set it to 16 not 15)
    let mut len_buf = [0u8; 4];
    let mut stream_buf = vec![0u8; block_size + 50];
    let mut piece_buf = vec![0u8; piece_size];
    let mut requested: Option<(usize, PieceInfo)> = None;
    let mut blocks = vec![false; piece_size / block_size];

    if is_outgoing {
        if let Err(e) = do_outgoing_handshake(
            &mut peer.out_stream,
            addr,
            info_hash,
            peer_id,
            handshake_msg,
            Arc::clone(&known_peers),
        )
        .await
        {
            error!("Handshake error, returning from peer_handler");
            // stream should be closed
            debug!("Stream closed, {addr}");
            let _ = tx.send(PeerMsg::Closed);
            return Err(tokio::io::Error::new(
                tokio::io::ErrorKind::Other,
                format!("Handshake failed"),
            ));
        }
    } else {
        if let Err(e) = do_incoming_handshake(
            &mut peer.out_stream,
            addr,
            info_hash,
            peer_id,
            handshake_msg,
            Arc::clone(&known_peers),
        )
        .await
        {
            error!("Handshake error, returning from peer_handler");
            // stream should be closed
            debug!("Stream closed, {addr}");
            let _ = tx.send(PeerMsg::Closed);
            return Err(tokio::io::Error::new(
                tokio::io::ErrorKind::Other,
                format!("Handshake failed"),
            ));
        }
    }

    // Handshake successful, increment number of peers
    // go into main loop
    // randomized timeout
    let tick_rate = std::time::Duration::from_millis(random_range(90..120));
    let mut interval = tokio::time::interval(tick_rate);
    let mut tui_interval = tokio::time::interval(Duration::from_millis(500));
    loop {
        let delay = interval.tick();
        let tui_update_delay = tui_interval.tick();
        tokio::select! {
            _ = peer.out_stream.readable() => {
                // debug!("Reselected peer.out_stream.readable()");
                // NOTE: we use several awaits here
                // *must make sure all locks are let go!*

                // either respond to request or
                // timeout on reads is 2 seconds now
                match timeout(
                    tokio::time::Duration::from_secs(1),
                    peer.out_stream.peek(&mut len_buf)
                ).await {
                    Err(e) => {
                        debug!("would block poll on peek v1: {e}, {addr}");
                        continue;
                    }
                    Ok(Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        debug!("would block poll on peek v2: {e}, {addr}");
                        continue;
                    },
                    Ok(Err(e)) => {
                        error!("peek error: {e}, {addr}");
                        return Err(e);
                    },
                    Ok(Ok(0)) => {
                        // stream has likely been closed
                        debug!("Stream closed, {addr}");
                        let _ = tx.send(PeerMsg::Closed);
                        return Ok(());
                    }
                    Ok(Ok(n)) if n < 4 => {
                        // debug!("not enough data to read message length: {n} bytes available");  // idk can just comment this out lol
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await; // idk can just comment this out lol
                        continue;
                    }
                    Ok(Ok(_)) => {
                        // read len_buf
                        let msg_len = (NetworkEndian::read_u32(&len_buf) as usize) + 4;
                        debug!("Read msg_len - 4 as {}, {addr}", msg_len - 4);
                        // prepare to consume at least that much
                        if stream_buf.len() < msg_len {
                            stream_buf.resize(msg_len, 0u8);
                        }
                        debug!("stream_buf[0..msg_len].len() = {}, {addr}", stream_buf[0..msg_len].len());

                        // must read in exactly that many bytes, otherwise
                        // will overread stream (TCP, not UDP)
                        // so read into a slice of a certain size? i think that should be fine
                        // timeout on reads is 2 seconds now
                        match timeout(
                            tokio::time::Duration::from_secs(2),
                            peer.out_stream.read_exact(&mut (stream_buf[0..msg_len]))
                        ).await {
                            Err(e) => {
                                debug!("would block poll on read v1: {e}, {addr}");
                                continue;
                            }
                            Ok(Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                debug!("would block poll on read v2: {e}, {addr}"); // idk can just comment this out lol
                                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await; // idk can just comment this out lol
                                continue;
                            },
                            Ok(Err(e)) => {
                                error!("read error: {e}, {addr}");
                                return Err(e);
                            },
                            Ok(Ok(bytes_read)) => {
                                debug!("read {bytes_read} bytes");
                                // parse and handle response message from peer
                                let Ok(msg) = Message::parse(&(stream_buf[0..msg_len])) else { continue };
                                debug!("read {msg}, {addr}");


                                // not every message can be recieved from this connection but good to
                                // include them anyways
                                match msg {
                                    Message::KeepAlive => last_response = Instant::now(),
                                    Message::Choke => peer.peer_choking = true,
                                    Message::UnChoke => peer.peer_choking = false,
                                    Message::Interested => {
                                        peer.peer_interested = true;
                                        if let Ok(mut stats) = shared_stats.lock() {
                                            stats.is_interested = true;
                                        }
                                    },
                                    Message::NotInterested => {
                                        peer.peer_interested = false;
                                        if let Ok(mut stats) = shared_stats.lock() {
                                            stats.is_interested = false;
                                        }
                                    },
                                    Message::Have(h) => {
                                        debug!("Seciding something something on send from peer channel");
                                        // update global torrent piece map
                                        // let _ = tx.send(PeerMsg::Have(h.piece_index)); // useless
                                        peer.bitfield.set(h.piece_index as usize, true);
                                    },
                                    Message::Bitfield(b) => {
                                        // update global torrent piece map
                                        peer.bitfield = b.bitfield.to_bitvec();
                                        let _ = tx.send(PeerMsg::Field(peer.bitfield.clone()));

                                        let Ok(b) = Message::Interested.create(&mut stream_buf) else { continue };
                                        //info!("saying I'm interested in {} ", peer.addr);
                                        let bytes_written = peer.out_stream
                                            .write_all_buf(
                                                &mut Cursor::new(&mut stream_buf[..b])
                                            )
                                            .await;
                                        peer.out_stream.flush().await?;
                                        debug!("interested write returned {:?}, wrote {}", bytes_written, b);
                                    },
                                    Message::Request(r) => {
                                        // check if we have the piece then send it if we do and if we arent
                                        // choking this peer
                                        if peer.am_choking {
                                            // do nothing
                                            debug!("peer {} requested {} but we were choking them", peer.addr, r.index);
                                        } else {
                                            debug!("peer {} requested {}, we aren't choking them, trying to upload", peer.addr, r.index);
                                            // AAAAAAAAAAAAAAAA
                                            {
                                                let mut info = pieces_info.lock().unwrap();
                                                // do something with this
                                            }
                                        }
                                    },
                                    Message::Piece(p) => {
                                        // idk where to put this but this is for calculating upload rate
                                        {
                                            if let Some(time_of_request) = pending_requests.remove(&(p.index, p.begin)) {
                                                let elapsed_time = time_of_request.elapsed().as_millis() as f64;
                                                if elapsed_time > 0.0 { // idk i had to do this in C
                                                    recent_uploads.push((p.block.len(), elapsed_time));
                                                }

                                                // remove head so that "recent" is actually true lol
                                                while recent_uploads.len() > 5 {
                                                    recent_uploads.remove(0);
                                                }

                                                // recent_uploads is the (length of the block, the elapsed time)
                                                // need to calculate general upload rate based off this
                                                let (total_bytes, total_time): (usize, f64) =
                                                    recent_uploads.iter().fold((0,0.0),
                                                        |(l_sum, e_sum), (l, e)| (l_sum + l, e_sum + e));
                                                let upload_rate = if total_time > 0.0 { // time is weird
                                                    total_bytes as f64 / total_time
                                                } else {
                                                    0.0
                                                };

                                                // Update shared stats
                                                if let Ok(mut stats) = shared_stats.lock() {
                                                    stats.upload_rate = upload_rate;
                                                }
                                            }
                                        }
                                        // end of upload rate tracking

                                        piece_buf[p.begin as usize..p.begin as usize + p.block.len()]
                                            .copy_from_slice(p.block);
                                        blocks[p.begin as usize / block_size] = true;

                                        //info!("{:?}", blocks);
                                        let mut next = 0;
                                        while next < blocks.len() {
                                            if !blocks[next] {
                                                break;
                                            } else {
                                                next += 1;
                                            }
                                        }

                                        if next >= blocks.len() {
                                            // TODO: hashing
                                            let id = hash_buffer(&piece_buf);
                                            let Some(ref mut pi) = &mut requested else { continue };
                                            //if pi.1.hash != id {
                                            //    {
                                            //        let mut info = pieces_info.lock().unwrap();
                                            //        info[pi.0].status = PieceStatus::NotRequested;
                                            //    }
                                            //    requested = None;
                                            //    warn!("Hash did not match");
                                            //    continue;
                                            //}
                                            let mut wr = pieces_data[pi.0].write().unwrap();
                                            *wr = std::mem::take(&mut piece_buf);
                                            piece_buf = vec![0u8; piece_size];
                                            {
                                                let mut info = pieces_info.lock().unwrap();
                                                info[pi.0].status = PieceStatus::Confirmed;
                                            }
                                            let _ = tx.send(PeerMsg::Confirmed(pi.0));
                                            requested = None;
                                        } else {
                                            // reqest next lsp
                                            let Some(ref pi) = requested else { continue };
                                            let len = Message::Request(
                                                messages::Request {
                                                    index: pi.0 as u32,
                                                    begin: (block_size * next) as u32,
                                                    length: block_size as u32
                                                }
                                            ).create(&mut stream_buf).unwrap();
                                            //info!("requesting block {} from {} ", next, peer.addr);
                                            let bytes_written = peer.out_stream.write_all(&mut stream_buf[..len]).await;
                                            pending_requests.insert(
                                                (pi.0 as u32, (block_size * next) as u32),
                                                Instant::now()
                                            ); // TODO - not just 0, interpolate with fixed logic
                                        }
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
            _ = delay => {
                // debug!("Reselected delay");
                // if we arent currently waiting for a reponse back from our peer and they arent
                // choking us then claim one of the next rarest pieces and request it.
                if requested.is_none() && !peer.peer_choking {
                    let mut bytes_written = None;
                    {
                        let mut info = pieces_info.lock().unwrap();
                        // this is not rarest first, just random TODO: make rarest first
                        let p = info.iter()
                                .enumerate()
                                .filter(|&(_, p)| p.status == PieceStatus::NotRequested)
                                .take(4)
                                .choose(&mut rand::rng());
                        match p {
                            Some((i, p)) => {
                                requested = Some((i, p.clone()));
                                info[i].status = PieceStatus::Requested;
                                let Ok(b) = Message::Request(
                                        messages::Request {
                                            index: i as u32,
                                            begin: 0,
                                            length: block_size as u32
                                        }).create(&mut stream_buf) else { continue };
                                bytes_written = Some(b);
                                // for tracking upload rate
                                pending_requests.insert((i as u32, 0), Instant::now()); // TODO - not just 0, interpolate with fixed logic
                            },
                            None => {
                                // we have downloaded all pieces so no need to send anything
                            }
                        }
                    }

                    let Some(len) = bytes_written else { continue };
                    info!("requesting block 0 from {} ", peer.addr);
                    peer.out_stream.write_all(&mut stream_buf[..len]).await?;
                }
                // debug!("Delay");
                // set a timer and if the request takes too long or cancle it and update info so
                // another task has a chance to claim it
                // TODO
                continue;
            },
            Some(cmd) = cmd_rx.recv() => { // for the choke unchoke shit
                match cmd {
                    PeerCommand::Choke => {
                        if peer.am_choking == true {
                            debug!("Received choke signal for {}, but was already choking", addr);
                            // do nothing
                        } else {
                            peer.am_choking = true;
                            if let Ok(len) = Message::Choke.create(&mut stream_buf) {
                                debug!("Sending choke to {}", addr);
                                peer.out_stream.write_all_buf(&mut Cursor::new(&mut stream_buf[..len])).await?;
                                peer.out_stream.flush().await?;
                            }
                        }
                    },
                    PeerCommand::UnChoke => {
                        if peer.am_choking == false {
                            debug!("Received unchoke signal for {}, but was already unchoked", addr);
                            // do nothing
                        } else {
                            peer.am_choking = false;
                            if let Ok(len) = Message::UnChoke.create(&mut stream_buf) {
                                debug!("Sending unchoke to {}", addr);
                                peer.out_stream.write_all_buf(&mut Cursor::new(&mut stream_buf[..len])).await?;
                                peer.out_stream.flush().await?;
                            }
                        }

                    },
                    PeerCommand::Kill => {
                        debug!("Received kill signal for {}", addr);
                        return Ok(());
                    },
                    PeerCommand::HaveUpdate(i) => {
                        debug!("Received have signal for index {}, {}", i, addr);
                        if let Ok(len) = Message::Have(messages::Have{piece_index: i}).create(&mut stream_buf) {
                            debug!("Sending have {} to {}", i, addr);
                            (&mut peer.out_stream).write_all_buf(&mut Cursor::new(&mut stream_buf[..len][..])).await?;
                            peer.out_stream.flush().await?;
                        }
                    }
                    _ => {
                        debug!("Received weirdo signal for {}", addr);
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum DoHandshakeError {
    BadRead,
    BadWrite,
    BadProtocolString,
    BadPeerId,
    DuplicatePeerId,
    BadHashId,
    FailedLock,
    TooManyPeers,
}

async fn do_incoming_handshake(
    stream: &mut TcpStream,
    peer_addr: SocketAddr,
    info_hash: HashedId20,
    peer_id: Option<PeerId20>,
    handshake_msg: BytesMut,
    known_peers: Arc<Mutex<HashMap<PeerId20, SocketAddr>>>,
) -> Result<(), DoHandshakeError> {
    debug!("Handling an incoming handshake from {}", peer_addr);
    // do that
    // receive their handshake not including peer_id and make sure the info_hash matches
    let mut their_handshake = [0u8; HANDSHAKE_LEN - 20]; // -20 for peer-id
    let res = stream.read_exact(&mut their_handshake).await;
    if let Err(e) = res {
        error!("Could not read start of handshake: {e}");
        return Err(DoHandshakeError::BadRead);
    }
    debug!("Read first bit of handshake from {}", peer_addr);

    // pstr handling
    let pstrlen = their_handshake[0] as usize;
    let pstr = &their_handshake[1..1 + pstrlen];
    if pstr != PROTOCOL_V_1 {
        error!(
            "Expected protocol {:?}, got protocol {:?}",
            PROTOCOL_V_1, pstr
        );
        return Err(DoHandshakeError::BadProtocolString);
    }

    debug!("Confirmed protocol from {}", peer_addr);

    // their_handshake[1 + pstrlen..1 + pstrlen + 8] is the reserved bits which we ignore

    // info_hash handling
    let received_info_hash = &their_handshake[1 + pstrlen + 8..1 + pstrlen + 8 + 20];
    if received_info_hash != info_hash {
        error!(
            "Expected info_hash {:?}, got info_hash {:?}",
            info_hash, received_info_hash
        );
        return Err(DoHandshakeError::BadHashId);
    }

    debug!("Confirmed info_hash from {}", peer_addr);

    // send a handshake
    let res = stream.write_all(&handshake_msg).await;
    if let Err(e) = res {
        error!("Could not write handshake: {e}");
        return Err(DoHandshakeError::BadWrite);
    }

    debug!("Sent handshake to {}", peer_addr);

    // receive and check their peer_id against known_peers to check if we need to close the connection
    let mut their_id = [0u8; 20]; // 20 for peer-id
    let res = stream.read_exact(&mut their_id).await;
    if let Err(e) = res {
        error!("Could not read peer_id: {e}");
        return Err(DoHandshakeError::BadRead);
    }

    debug!("Got peer_id from {}", peer_addr);

    // make sure no duplicate connections
    match known_peers.lock() {
        Err(e) => {
            error!("Could not get lock on known_peers: {e}");
            return Err(DoHandshakeError::FailedLock);
        }
        Ok(mut known_peers_writer) => {
            if known_peers_writer.contains_key(&their_id) {
                debug!("Already connected to id={:?}, {}", &their_id, peer_addr);
                drop(known_peers_writer);
                return Err(DoHandshakeError::DuplicatePeerId);
            } else if known_peers_writer.len() >= 55 {
                debug!("Already connected to 55 peers, {}", peer_addr);
                drop(known_peers_writer);
                return Err(DoHandshakeError::TooManyPeers);
            }
            debug!(
                "This peer id={:?} was not a known peer {}",
                &their_id, peer_addr
            );
            known_peers_writer.insert(their_id, peer_addr);
            drop(known_peers_writer);
        }
    }

    Ok(())
}

async fn do_outgoing_handshake(
    stream: &mut TcpStream,
    peer_addr: SocketAddr,
    info_hash: HashedId20,
    peer_id: Option<PeerId20>,
    handshake_msg: BytesMut,
    known_peers: Arc<Mutex<HashMap<PeerId20, SocketAddr>>>,
) -> Result<(), DoHandshakeError> {
    debug!("Handling an outgoing handshake to {}", peer_addr);
    // do that
    // send them a handshake
    let res = stream.write_all(&handshake_msg).await;
    if let Err(e) = res {
        error!("Could not write handshake: {e}");
        return Err(DoHandshakeError::BadWrite);
    }

    // receive their handshake
    let mut their_handshake = [0u8; HANDSHAKE_LEN];
    let res = stream.read_exact(&mut their_handshake).await;
    if let Err(e) = res {
        error!("Could not read handshake: `{e}` from peer {:?}", peer_id);
        return Err(DoHandshakeError::BadRead);
    }
    debug!("Read the handshake from {}", peer_addr);

    // pstr handling
    let pstrlen = their_handshake[0] as usize;
    let pstr = &their_handshake[1..1 + pstrlen];
    if pstr != PROTOCOL_V_1 {
        error!(
            "Expected protocol {:?}, got protocol {:?}",
            PROTOCOL_V_1, pstr
        );
        return Err(DoHandshakeError::BadProtocolString);
    }

    debug!("Confirmed protocol from {}", peer_addr);

    // their_handshake[1 + pstrlen..1 + pstrlen + 8] is the reserved bits which we ignore

    // info_hash handling
    let received_info_hash = &their_handshake[1 + pstrlen + 8..1 + pstrlen + 8 + 20];
    if received_info_hash != info_hash {
        error!(
            "Expected info_hash {:?}, got info_hash {:?}",
            info_hash, received_info_hash
        );
        return Err(DoHandshakeError::BadHashId);
    }

    debug!("Confirmed info_hash from {}", peer_addr);

    // peer_id handling
    let their_id_bytes = &their_handshake[1 + pstrlen + 8 + 20..];
    let their_id: PeerId20 = their_id_bytes
        .try_into()
        .map_err(|_| DoHandshakeError::BadPeerId)?;

    debug!("Got peer_id from {}", peer_addr);
    if let Some(expected_peer_id) = peer_id {
        if expected_peer_id != their_id {
            error!(
                "Got peer_id {:?}, but expected \nid {:?}",
                their_id, expected_peer_id
            );
            return Err(DoHandshakeError::BadPeerId);
        } // caller makes sure we aren't connecting to self
    }

    // make sure no duplicate connections
    match known_peers.lock() {
        Err(e) => {
            error!("Could not get read lock on known_peers: {e}");
            return Err(DoHandshakeError::FailedLock);
        }
        Ok(mut known_peers_writer) => {
            if known_peers_writer.contains_key(&their_id) {
                debug!("Already connected to id={:?}, {}", &their_id, peer_addr);
                drop(known_peers_writer);
                return Err(DoHandshakeError::DuplicatePeerId);
            }
            debug!(
                "This peer id={:?} was not a known peer, good, allowing connection! {}",
                &their_id, peer_addr
            );
            known_peers_writer.insert(their_id, peer_addr);
            drop(known_peers_writer);
        }
    }

    Ok(())
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
    pub id: Option<PeerId20>,
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

#[derive(Debug)]
pub enum PeerCommand {
    Choke,
    UnChoke,
    HaveUpdate(u32),
    Kill,
    NewStream(TcpStream),
}

#[derive(Debug)]
pub struct PeerStats {
    pub is_interested: bool,
    pub upload_rate: f64,
}

#[derive(Debug)]
pub struct HandleEntry {
    pub handle: JoinHandle<Result<(), tokio::io::Error>>,
    pub sender: UnboundedSender<PeerCommand>,
    pub stats: Arc<Mutex<PeerStats>>,
    pub is_new: bool,
}
