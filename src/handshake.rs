use bytes::{BufMut, BytesMut};

use crate::{HashedId20, PeerId20};

pub struct Handshake {
    info_hash: HashedId20,
    peer_id: PeerId20,
}

impl Handshake {
    fn new(&mut self, info_hash: HashedId20, peer_id: PeerId20) -> Self {
        Self {
            info_hash,
            peer_id
        }
    }

    fn serialize_handshake(&self) -> BytesMut {
        let mut buf = BytesMut::with_capacity(68);
        buf.put_u8(19);
        buf.put_slice(b"BitTorrent protocol");
        buf.put_u64(0); // Reserved
        buf.put(&self.info_hash[..]);
        buf.put(&self.peer_id[..]);
        buf
    }
}
