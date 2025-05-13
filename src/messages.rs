use bitvec::order::Msb0;
use bitvec::prelude::{BitSlice, BitVec};
use bitvec::view::BitView;
use byteorder::{ByteOrder, NetworkEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Read, Result, Seek, Write};

#[derive(Debug)]
pub struct Have {
    // <len=0005><id=4><piece index>
    pub piece_index: u32,
}

#[derive(Debug)]
pub struct Bitfield<'a> {
    // <len=0001+X><id=5><bitfield>
    pub bitfield: &'a BitSlice<u8, Msb0>,
}

#[derive(Debug)]
pub struct Request {
    // <len=0013><id=6><index><begin><length>
    pub index: u32,
    pub begin: u32,
    pub length: u32,
}

#[derive(Debug)]
pub struct Piece<'a> {
    // <len=0009+X><id=7><index><begin><block>
    pub index: u32,
    pub begin: u32,
    pub block: &'a [u8], // TODO - rethink this data type
}

#[derive(Debug)]
pub struct Cancel {
    // <len=0013><id=8><index><begin><length>
    pub index: u32,
    pub begin: u32,
    pub length: u32,
}

#[derive(Debug)]
pub struct Port {
    // <len=0003><id=9><listen-port>
    pub port: u16,
}

#[derive(Debug)]
pub enum Message<'a> {
    KeepAlive,              // <len=0000>
    Choke,                  // <len=0001><id=0>
    UnChoke,                // <len=0001><id=1>
    Interested,             // <len=0001><id=2>
    NotInterested,          // <len=0001><id=3>
    Have(Have),             // <len=0005><id=4><piece index>
    Bitfield(Bitfield<'a>), // <len=0001+X><id=5><bitfield>
    Request(Request),       // <len=0013><id=6><index><begin><length>
    Piece(Piece<'a>),       // <len=0009+X><id=7><index><begin><block>
    Cancel(Cancel),         // <len=0013><id=8><index><begin><length>
    Port(Port),             // <len=0003><id=9><listen-port>
}

impl<'a> Message<'a> {
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
                writer.write_all(substance.bitfield.to_bitvec().as_raw_slice())?;
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
                writer.write_all(substance.block)?;
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
        let size = writer.stream_position()? - 4; // -4 for length from current pos
        writer.seek(io::SeekFrom::Start(0))?; // go back to start to write this
        writer.write_u32::<NetworkEndian>(size as u32)?; // length prefix is a four byte big-endian value

        Ok(size as usize)
    }

    pub fn parse(buf: &'a [u8]) -> Result<Self> {
        let size = NetworkEndian::read_u32(&buf[0..4]);

        if size == 0 {
            return Ok(Message::KeepAlive);
        }

        let msg_type = buf[4];

        match msg_type {
            0 => Ok(Message::Choke),         // choke
            1 => Ok(Message::UnChoke),       // unchoke
            2 => Ok(Message::Interested),    // interested
            3 => Ok(Message::NotInterested), // not interested
            4 => {
                // have
                let piece_index: u32 = NetworkEndian::read_u32(&buf[5..9]);
                Ok(Message::Have(Have { piece_index }))
            }
            5 => {
                // bitfield
                // we know 'size' is a u32 representing 1 + X
                // where X is the number of bytes that makes up this bitfield
                let bitfield: &BitSlice<u8, Msb0> =
                    buf[5..5 + size as usize - 1].view_bits::<Msb0>();
                Ok(Message::Bitfield(Bitfield { bitfield }))
            }
            6 => {
                // request
                let index: u32 = NetworkEndian::read_u32(&buf[5..9]);
                let begin: u32 = NetworkEndian::read_u32(&buf[9..13]);
                let length: u32 = NetworkEndian::read_u32(&buf[13..17]);
                Ok(Message::Request(Request {
                    index,
                    begin,
                    length,
                }))
            }
            7 => {
                // piece
                let index: u32 = NetworkEndian::read_u32(&buf[5..9]);
                let begin: u32 = NetworkEndian::read_u32(&buf[9..13]);
                // len is 9 + X
                let len = size as usize - 9;
                //reader.read_exact(&mut block)?;
                Ok(Message::Piece(Piece {
                    index,
                    begin,
                    block: &buf[13..13 + len],
                }))
            }
            8 => {
                // cancel
                let index: u32 = NetworkEndian::read_u32(&buf[5..9]);
                let begin: u32 = NetworkEndian::read_u32(&buf[9..13]);
                let length: u32 = NetworkEndian::read_u32(&buf[13..17]);
                Ok(Message::Cancel(Cancel {
                    index,
                    begin,
                    length,
                }))
            }
            9 => {
                // port
                let port: u16 = NetworkEndian::read_u16(&buf[5..7]);
                Ok(Message::Port(Port { port }))
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
    }
}
