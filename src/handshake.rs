use bytes::{BufMut, BytesMut};

use crate::{HashedId20, PeerId20, HANDSHAKE_LEN, PROTOCOL_V_1};

pub struct Handshake {
    info_hash: HashedId20,
    peer_id: PeerId20,
}

impl Handshake {
    pub fn new(info_hash: HashedId20, peer_id: PeerId20) -> Self {
        Self { info_hash, peer_id }
    }

    pub fn serialize_handshake(&self) -> BytesMut {
        let mut buf = BytesMut::with_capacity(HANDSHAKE_LEN);
        buf.put_u8(19);
        buf.put_slice(PROTOCOL_V_1);
        buf.put_u64(0); // Reserved
        buf.put(&self.info_hash[..]);
        buf.put(&self.peer_id[..]);
        buf
    }
}
