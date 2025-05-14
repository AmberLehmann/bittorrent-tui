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
    pub block: &'a [u8],
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
    Unknown
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
            Message::Unknown => {
                log::error!("Why are you asking me to do this");
                panic!();
            }
        }

        // write resulting message size to the start
        let size = writer.stream_position()? - 4; // -4 for length from current pos
        writer.seek(io::SeekFrom::Start(0))?; // go back to start to write this
        writer.write_u32::<NetworkEndian>(size as u32)?; // length prefix is a four byte big-endian value

        Ok(size as usize)
    }

    pub fn parse(buf: &'a [u8]) -> Result<(Self, &'a [u8])> {
        if buf.len() < 4 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof, 
                "Message too short"
            ));
        }
        let size = NetworkEndian::read_u32(&buf[0..4]);
        if buf.len() < 4 + (size as usize) {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof, 
                format!("Message shorter than expected. expected {}, got {}", 4 + (size as usize), buf.len())
            ));
        }

        let msg = 
        if size == 0 {
            Message::KeepAlive
        } else {
            let msg_type = buf[4];

            match msg_type {
                0 => Message::Choke,         // choke
                1 => Message::UnChoke,       // unchoke
                2 => Message::Interested,    // interested
                3 => Message::NotInterested, // not interested
                4 => {
                    // have
                    let piece_index: u32 = NetworkEndian::read_u32(&buf[5..9]);
                    Message::Have(Have { piece_index })
                }
                5 => {
                    // bitfield
                    // we know 'size' is a u32 representing 1 + X
                    // where X is the number of bytes that makes up this bitfield
                    let bitfield: &BitSlice<u8, Msb0> =
                        buf[5..5 + size as usize - 1].view_bits::<Msb0>();
                    Message::Bitfield(Bitfield { bitfield })
                }
                6 => {
                    // request
                    let index: u32 = NetworkEndian::read_u32(&buf[5..9]);
                    let begin: u32 = NetworkEndian::read_u32(&buf[9..13]);
                    let length: u32 = NetworkEndian::read_u32(&buf[13..17]);
                    Message::Request(Request {
                        index,
                        begin,
                        length,
                    })
                }
                7 => {
                    // piece
                    let index: u32 = NetworkEndian::read_u32(&buf[5..9]);
                    let begin: u32 = NetworkEndian::read_u32(&buf[9..13]);
                    // len is 9 + X
                    let len = size as usize - 9;
                    //reader.read_exact(&mut block)?;
                    Message::Piece(Piece {
                        index,
                        begin,
                        block: &buf[13..13 + len],
                    })
                }
                8 => {
                    // cancel
                    let index: u32 = NetworkEndian::read_u32(&buf[5..9]);
                    let begin: u32 = NetworkEndian::read_u32(&buf[9..13]);
                    let length: u32 = NetworkEndian::read_u32(&buf[13..17]);
                    Message::Cancel(Cancel {
                        index,
                        begin,
                        length,
                    })
                }
                9 => {
                    // port
                    let port: u16 = NetworkEndian::read_u16(&buf[5..7]);
                    Message::Port(Port { port })
                }
                _ => Message::Unknown,
            }
        };

        Ok((msg, &buf[4 + (size as usize)..])) // return message + remaining buffer
    }
}

// NOTE: Use `cargo test -- --show-output`.
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor; // https://doc.rust-lang.org/std/io/struct.Cursor.html (fake IO from https://doc.rust-lang.org/std/io/trait.Write.html)
    // use std::fs::File;

    #[test]
    fn create_and_parse_msg_type_piece() {
        let block = b"hehe";
        let msg_struct = Message::Piece(Piece {
            index: 7,
            begin: 2,
            block
        });

        let mut fake_tcp_stream = Cursor::new(vec![0u8; 40]);
        let _total_len = msg_struct.create(&mut fake_tcp_stream).unwrap();

        let (parsed, leftover) = Message::parse(fake_tcp_stream.get_ref()).unwrap();
        match parsed {
            Message::Piece(p) => {
                assert_eq!(p.index, 7);
                assert_eq!(p.begin, 2);
                assert_eq!(p.block, block);
            }
            _ => panic!("Parsed message is not a Piece"),
        }

        // should be length 4 + 9 + X where X is 4, buffer was originally 40, so
        assert_eq!(leftover.len(), 40 - (4+9+4));
    }

    #[test]
    fn hardcode_and_parse_msg_type_piece() {
        let fake_tcp_stream = Cursor::new(
            vec![
                // <len=9+4> (4 bytes)
                0x00, 0x00, 0x00, 0x0D,
                // <id=7> (1 byte)
                0x07,
                // <index=7> (4 bytes)
                0x00, 0x00, 0x00, 0x07,
                // <begin=2> (4 bytes)
                0x00, 0x00, 0x00, 0x02,
                // <block=b"hehe">
                b'h', b'e', b'h', b'e'
            ]
        );

        let (parsed, leftover) = Message::parse(fake_tcp_stream.get_ref()).unwrap();
        match parsed {
            Message::Piece(p) => {
                assert_eq!(p.index, 7);
                assert_eq!(p.begin, 2);
                assert_eq!(p.block, b"hehe");
            }
            _ => panic!("Parsed message is not a Piece"),
        }

        assert_eq!(leftover.len(), 0);
    }

    #[test]
    fn create_and_parse_msg_type_bitfield() {
        let bitfield = vec![0xAF];
        let bitfield = BitSlice::<u8, Msb0>::from_slice(&bitfield);
        let msg_struct = Message::Bitfield(Bitfield {
            bitfield
        });

        let mut fake_tcp_stream = Cursor::new(vec![0u8; 40]);
        let _total_len = msg_struct.create(&mut fake_tcp_stream).unwrap();

        let (parsed, leftover) = Message::parse(fake_tcp_stream.get_ref()).unwrap();
        match parsed {
            Message::Bitfield(p) => {
                let expected_v = vec![0b10101111u8];
                let expected_bits = BitSlice::<u8, Msb0>::from_slice(&expected_v);
                assert_eq!(p.bitfield, expected_bits);
            }
            _ => panic!("Parsed message is not a Bitfield"),
        }

        // should be length 4 + 1 + X where X is 1, buffer was originally 40, so
        assert_eq!(leftover.len(), 40 - (4+1+1));
    }

    #[test]
    fn hardcode_and_parse_msg_type_bitfield() {
        let fake_tcp_stream = Cursor::new(
            vec![
                // <len=1+1> (4 bytes)
                0x00, 0x00, 0x00, 0x02,
                // <id=5> (1 byte)
                0x05,
                // <bitfield=in binary 1010=A 1111=F>
                0xAF
            ]
        );

        let (parsed, leftover) = Message::parse(fake_tcp_stream.get_ref()).unwrap();
        match parsed {
            Message::Bitfield(p) => {
                let expected_v = vec![0b10101111u8];
                let expected_bits = BitSlice::<u8, Msb0>::from_slice(&expected_v);
                assert_eq!(p.bitfield, expected_bits);
            }
            _ => panic!("Parsed message is not a Bitfield"),
        }

        assert_eq!(leftover.len(), 0);
    }
}
