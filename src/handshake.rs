use crate::HashedId20;

pub struct Handshake {
    pstrlen: u8,
    pstr: String,
    reserved: u64,
    info_hash: HashedId20,
    peer_id: HashedId20,
}

impl Handshake {
    fn new(info_hash: HashedId20, peer_id: HashedId20) -> Self {
        Handshake {
            pstrlen: 19,
            pstr: "BitTorrent protocol".to_string(),
            reserved: 0,
            info_hash,
            peer_id,
        }
    }
}
