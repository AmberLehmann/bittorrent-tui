use bitvec::order::Msb0;
use bitvec::prelude::BitVec;
use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use std::io;
use std::io::{Read, Write, Result, Seek};

pub struct Have { // <len=0005><id=4><piece index>
    pub piece_index : u32
}

pub struct Bitfield { // <len=0001+X><id=5><bitfield>
    pub bitfield : BitVec<u8, Msb0>
}

pub struct Request { // <len=0013><id=6><index><begin><length>
    pub index : u32,
    pub begin : u32,
    pub length : u32
}

pub struct Piece { // <len=0009+X><id=7><index><begin><block>
    pub index : u32,
    pub begin : u32,
    pub block : Vec<u8> // TODO - rethink this data type
}

pub struct Cancel { // <len=0013><id=8><index><begin><length>
    pub index : u32,
    pub begin : u32,
    pub length : u32
}

pub struct Port { // <len=0003><id=9><listen-port>
    pub port : u16
}

pub enum Message {
    KeepAlive, // <len=0000>
    Choke, // <len=0001><id=0>
    UnChoke, // <len=0001><id=1>
    Interested, // <len=0001><id=2>
    NotInterested, // <len=0001><id=3>
    Have(Have), // <len=0005><id=4><piece index>
    Bitfield(Bitfield), // <len=0001+X><id=5><bitfield>
    Request(Request), // <len=0013><id=6><index><begin><length>
    Piece(Piece), // <len=0009+X><id=7><index><begin><block>
    Cancel(Cancel), // <len=0013><id=8><index><begin><length>
    Port(Port), // <len=0003><id=9><listen-port>
}

impl Message {

    // <length prefix><message ID><payload>
    // 1. length prefix is a four byte big-endian value
    // 2. message ID is a single decimal byte
    // 3. payload is message dependent

    pub fn create<T: Write + Seek>(&self, mut writer: T) -> Result<usize> {
        writer.write_u32::<NetworkEndian>(0)?;

        match self {
            Message::KeepAlive => {}
            Message::Choke => {
                writer.write_u8(0)?;
            }
            Message::UnChoke => {
                writer.write_u8(1)?;
            }
            Message::Interested => {
                writer.write_u8(2)?;
            }
            Message::NotInterested => {
                writer.write_u8(3)?;
            }
            Message::Have(substance) => {
                writer.write_u8(4)?;
                writer.write_u32::<NetworkEndian>(substance.piece_index)?;
            }
            Message::Bitfield(substance) => {
                writer.write_u8(5)?;
                writer.write_all(substance.bitfield.as_raw_slice())?;
            }
            Message::Request(substance) => {
                writer.write_u8(6)?;
                writer.write_u32::<NetworkEndian>(substance.index)?;
                writer.write_u32::<NetworkEndian>(substance.begin)?;
                writer.write_u32::<NetworkEndian>(substance.length)?;
            }
            Message::Piece(substance) => {
                writer.write_u8(7)?;
                writer.write_u32::<NetworkEndian>(substance.index)?;
                writer.write_u32::<NetworkEndian>(substance.begin)?;
                writer.write_all(&substance.block)?;
            }
            Message::Cancel(substance) => {
                writer.write_u8(8)?;
                writer.write_u32::<NetworkEndian>(substance.index)?;
                writer.write_u32::<NetworkEndian>(substance.begin)?;
                writer.write_u32::<NetworkEndian>(substance.length)?;
            }
            Message::Port(substance) => {
                writer.write_u8(9)?;
                writer.write_u16::<NetworkEndian>(substance.port)?;
            }
        }

        // write resulting message size to the start
        let size = writer.seek(io::SeekFrom::Current(0))? - 4; // -4 for length from current pos
        writer.seek(io::SeekFrom::Start(0))?; // go back to start to write this
        writer.write_u32::<NetworkEndian>(size as u32)?; // length prefix is a four byte big-endian value

        Ok(size as usize)
    }

    pub fn parse<T: Read>(mut reader: T) -> Result<Self> {
        let size = reader.read_u32::<NetworkEndian>()?;

        if size == 0 {
            return Ok(Message::KeepAlive);
        }

        let msg_type = reader.read_u8()?;

        match msg_type {
            0 => { // choke
                return Ok(Message::Choke); 
            }
            1 => { // unchoke
                return Ok(Message::UnChoke); 
            }
            2 => { // interested
                return Ok(Message::Interested); 
            }
            3 => { // not interested
                return Ok(Message::NotInterested); 
            }
            4 => { // have
                let piece_index : u32 = reader.read_u32::<NetworkEndian>()?;
                return Ok(Message::Have(Have {piece_index})); 
            }
            5 => { // bitfield
                // we know 'size' is a u32 representing 1 + X
                // where X is the number of bytes that makes up this bitfield
                let mut bitfield_bytes : Vec<u8> = vec![0; (size-1) as usize];
                reader.read_exact(&mut bitfield_bytes)?;
                let bitfield = BitVec::<u8, Msb0>::from_vec(bitfield_bytes);
                return Ok(Message::Bitfield(Bitfield {bitfield})); 
            }
            6 => { // request
                let index : u32 = reader.read_u32::<NetworkEndian>()?;
                let begin : u32 = reader.read_u32::<NetworkEndian>()?;
                let length : u32 = reader.read_u32::<NetworkEndian>()?;
                return Ok(Message::Request(Request {index, begin, length})); 
            }
            7 => { // piece
                let index : u32 = reader.read_u32::<NetworkEndian>()?;
                let begin : u32 = reader.read_u32::<NetworkEndian>()?;
                // len is 9 + X
                let mut block : Vec<u8> = vec![0; (size-9) as usize];
                reader.read_exact(&mut block)?;
                return Ok(Message::Piece(Piece {index, begin, block})); 
            }
            8 => { // cancel
                let index : u32 = reader.read_u32::<NetworkEndian>()?;
                let begin : u32 = reader.read_u32::<NetworkEndian>()?;
                let length : u32 = reader.read_u32::<NetworkEndian>()?;
                return Ok(Message::Cancel(Cancel {index, begin, length})); 
            }
            9 => { // port
                return Ok(Message::UnChoke); 
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid message type",
            )),
        }
    }

}


// NOTE: Use `cargo test -- --show-output`.
#[cfg(test)]
mod tests {
    #[test]
    fn create_msg_type_piece() {
        // TODO test
        // let tr = TrackerRequest {
        //     info_hash: *b"12345678901234567890",
        //     peer_id: *b"ABCDEFGHIJKLMNOPQRST",
        //     event: Some(TrackerRequestEvent::Started),
        //     port: 8090,
        //     uploaded: 200,
        //     downloaded: 200,
        //     left: 100,
        //     compact: false,
        //     no_peer_id: false,
        //     ip: Some(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
        //     announce_path: "/announce".to_owned(),
        //     numwant: Some(2),
        //     key: Some("secret".into()),
        //     trackerid: Some("trackster".into()),
        // };
        // dbg!(tr.encode_http_get("test".into()));
    }
}