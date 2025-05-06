use crate::metainfo::{Info, MetaInfo, SingleFileInfo};
use std::net::{SocketAddr, ToSocketAddrs};

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
