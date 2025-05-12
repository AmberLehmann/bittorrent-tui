use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use std::io;

#[derive(Debug, PartialEq)]
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
    const MSG_TYPE_CHOKE : u8 = 0;
    const MSG_TYPE_UNCHOKE : u8 = 1;
    const MSG_TYPE_INTERESTED : u8 = 2;
    const MSG_TYPE_NOTINTERESTED : u8 = 3;
    const MSG_TYPE_HAVE : u8 = 4;
    const MSG_TYPE_BITFIELD : u8 = 5;
    const MSG_TYPE_REQUEST : u8 = 6;
    const MSG_TYPE_PIECE : u8 = 7;
    const MSG_TYPE_CANCEL : u8 = 8;
    const MSG_TYPE_PORT : u8 = 9;

    // <length prefix><message ID><payload>
    // 1. length prefix is a four byte big-endian value
    // 2. message ID is a single decimal byte
    // 3. payload is message dependent

    pub fn parse<T: Read>(mut reader: T) -> io::Result<Self> {
        let size = reader.read_u32::<NetworkEndian>()?;
        // let msg_type = reader.read_u8::<NetworkEndian>()?;

        if size == 0 {
            Message::KeepAlive
        }

        let reader = &mut reader.take(u64::from(size) - 4);

        match msg_type {
            Self::DHT_PUT => {
                // parse DhtPut payload
                MessagePayload::parse(reader).map(Message::DhtPut)
            }
            Self::DHT_GET => {
                // parse DhtGet payload
                MessagePayload::parse(reader).map(Message::DhtGet)
            }
            Self::DHT_SUCCESS => {
                // parse DhtSuccess payload
                MessagePayload::parse(reader).map(Message::DhtSuccess)
            }
            Self::DHT_FAILURE => {
                // parse DhtFailure payload
                MessagePayload::parse(reader).map(Message::DhtFailure)
            }
            Self::STORAGE_GET => {
                // parse StorageGet payload
                MessagePayload::parse(reader).map(Message::StorageGet)
            }
            Self::STORAGE_PUT => {
                // parse StoragePut payload
                MessagePayload::parse(reader).map(Message::StoragePut)
            }
            Self::STORAGE_GET_SUCCESS => {
                // parse StorageGetSuccess payload
                MessagePayload::parse(reader).map(Message::StorageGetSuccess)
            }
            Self::STORAGE_PUT_SUCCESS => {
                // parse StoragePutSuccess payload
                MessagePayload::parse(reader).map(Message::StoragePutSuccess)
            }
            Self::STORAGE_FAILURE => {
                // parse StorageFailure payload
                MessagePayload::parse(reader).map(Message::StorageFailure)
            }
            Self::PEER_FIND => {
                // parse PeerFind payload
                MessagePayload::parse(reader).map(Message::PeerFind)
            }
            Self::PEER_FOUND => {
                // parse PeerFound payload
                MessagePayload::parse(reader).map(Message::PeerFound)
            }
            Self::PREDECESSOR_NOTIFY => {
                // parse PredecessorNotify payload
                MessagePayload::parse(reader).map(Message::PredecessorNotify)
            }
            Self::PREDECESSOR_REPLY => {
                // parse PredecessorReply payload
                MessagePayload::parse(reader).map(Message::PredecessorReply)
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid message type",
            )),
        }
    }

    pub fn write_to<T: Write + Seek>(&self, mut writer: T) -> io::Result<usize> {
        writer.write_u32::<NetworkEndian>(0)?;

        match self {
            Message::KeepAlive => {
                // do nothing
            }
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
                substance.write_to(&mut writer)?;
            }
            Message::Bitfield(substance) => {
                writer.write_u8(5)?;
                substance.write_to(&mut writer)?;
            }
            Message::Request(substance) => {
                writer.write_u8(6)?;
                substance.write_to(&mut writer)?;
            }
            Message::Piece(substance) => {
                writer.write_u8(7)?;
                substance.write_to(&mut writer)?;
            }
            Message::Cancel(substance) => {
                writer.write_u8(8)?;
                substance.write_to(&mut writer)?;
            }
            Message::Port(substance) => {
                writer.write_u8(9)?;
                substance.write_to(&mut writer)?;
            }
        }

        // write size at beginning of writer
        let size = writer.seek(io::SeekFrom::Current(4))?;

        writer.seek(io::SeekFrom::Start(0))?;
        writer.write_u16::<NetworkEndian>(size as u16)?;

        Ok(size as usize)
    }
}

pub trait MessagePayload: Sized {
    fn parse(reader: &mut dyn Read) -> io::Result<Self>;

    fn write_to(&self, writer: &mut dyn Write) -> io::Result<()>;
}