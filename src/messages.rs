use bitvec::order::Msb0;
use bitvec::prelude::{BitSlice};
use bitvec::view::BitView;
use byteorder::{ByteOrder, NetworkEndian, WriteBytesExt};
use std::io::{self, Result, Seek, Write};

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

    pub fn create(&self, buf: &mut [u8]) -> Result<usize> {
        //writer.write_u32::<NetworkEndian>(0)?;

        match self {
            Message::KeepAlive => {
                NetworkEndian::write_u32(buf, 0);
            }
            Message::Choke => {
                NetworkEndian::write_u32(buf, 1);
                buf[4] = 0;
            }
            Message::UnChoke => {
                NetworkEndian::write_u32(buf, 1);
                buf[4] = 1;
            }
            Message::Interested => {
                NetworkEndian::write_u32(buf, 1);
                buf[4] = 2;
            }
            Message::NotInterested => {
                NetworkEndian::write_u32(buf, 1);
                buf[4] = 3;
            }
            Message::Have(s) => {
                NetworkEndian::write_u32(buf, 5);
                buf[4] = 4;
                NetworkEndian::write_u32(&mut buf[5..9], s.piece_index);
            }
            Message::Bitfield(s) => {
                NetworkEndian::write_u32(buf, 0);
                buf[4] = 5;
                buf[5..].copy_from_slice(s.bitfield.to_bitvec().as_raw_slice());
            }
            Message::Request(s) => {
                NetworkEndian::write_u32(buf, 13);
                buf[4] = 6;
                NetworkEndian::write_u32(&mut buf[5..9], s.index);
                NetworkEndian::write_u32(&mut buf[9..13], s.begin);
                NetworkEndian::write_u32(&mut buf[13..17], s.length);
            }
            Message::Piece(s) => {
                NetworkEndian::write_u32(buf, 0);
                buf[4] = 7;
                NetworkEndian::write_u32(&mut buf[5..9], s.index);
                NetworkEndian::write_u32(&mut buf[9..13], s.begin);
                buf[13..].clone_from_slice(s.block);
            }
            Message::Cancel(s) => {
                NetworkEndian::write_u32(buf, 13);
                buf[4] = 8;
                NetworkEndian::write_u32(&mut buf[5..9], s.index);
                NetworkEndian::write_u32(&mut buf[9..13], s.begin);
                NetworkEndian::write_u32(&mut buf[13..17], s.length);
            }
            Message::Port(s) => {
                NetworkEndian::write_u32(buf, 3);
                buf[4] = 9;
                NetworkEndian::write_u16(&mut buf[5..7], s.port);
            }
            Message::Unknown => {
                log::error!("Why are you asking me to do this");
                panic!();
            }
        }

        Ok(NetworkEndian::read_u32(buf) as usize)
    }

    pub fn parse(buf: &'a [u8]) -> Result<Self> {
        if buf.len() < 4 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof, 
                "Message too short"
            ));
        }
        let size = NetworkEndian::read_u32(&buf[0..4]);
        if buf.len() != 4 + (size as usize) {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof, 
                format!("Message length different than expected. expected {}, got {}", 4 + (size as usize), buf.len())
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

        Ok(msg) // return message + remaining buffer
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

        let mut fake_tcp_stream = Cursor::new(vec![0u8; 17]);
        let _total_len = msg_struct.create(&mut fake_tcp_stream).unwrap();

        let parsed = Message::parse(fake_tcp_stream.get_ref()).unwrap();
        match parsed {
            Message::Piece(p) => {
                assert_eq!(p.index, 7);
                assert_eq!(p.begin, 2);
                assert_eq!(p.block, block);
            }
            _ => panic!("Not a piece"),
        }
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

        let parsed = Message::parse(fake_tcp_stream.get_ref()).unwrap();
        match parsed {
            Message::Piece(p) => {
                assert_eq!(p.index, 7);
                assert_eq!(p.begin, 2);
                assert_eq!(p.block, b"hehe");
            }
            _ => panic!("Not a piece"),
        }
    }

    #[test]
    fn create_and_parse_msg_type_bitfield() {
        let bitfield = vec![0xAF];
        let bitfield = BitSlice::<u8, Msb0>::from_slice(&bitfield);
        let msg_struct = Message::Bitfield(Bitfield {
            bitfield
        });

        let mut fake_tcp_stream = Cursor::new(vec![0u8; 6]);
        let _total_len = msg_struct.create(&mut fake_tcp_stream).unwrap();

        let parsed = Message::parse(fake_tcp_stream.get_ref()).unwrap();
        match parsed {
            Message::Bitfield(p) => {
                let expected_v = vec![0b10101111u8];
                let expected_bits = BitSlice::<u8, Msb0>::from_slice(&expected_v);
                assert_eq!(p.bitfield, expected_bits);
            }
            _ => panic!("Not a bitfield"),
        }
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

        let parsed = Message::parse(fake_tcp_stream.get_ref()).unwrap();
        match parsed {
            Message::Bitfield(p) => {
                let expected_v = vec![0b10101111u8];
                let expected_bits = BitSlice::<u8, Msb0>::from_slice(&expected_v);
                assert_eq!(p.bitfield, expected_bits);
            }
            _ => panic!("Not a bitfield"),
        }
    }
}
