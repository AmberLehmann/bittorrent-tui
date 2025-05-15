use serde::Deserialize;
use serde_bytes::ByteBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct SingleFileInfo {
    #[serde(rename = "piece length")]
    pub piece_length: u64,
    pub pieces: ByteBuf,
    #[serde(default)]
    pub private: u32,

    pub name: String,
    pub length: u64,
    #[serde(default)]
    pub md5sum: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct File {
    pub length: u64,
    #[serde(default)]
    pub md5sum: String,
    pub path: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MultiFileInfo {
    #[serde(rename = "piece length")]
    pub piece_length: u64,
    pub pieces: ByteBuf,
    #[serde(default)]
    pub private: u32,

    pub name: String,
    pub files: Vec<File>,
}

#[derive(Debug, Deserialize, Clone)]
pub enum Info {
    #[serde(untagged)]
    Single(SingleFileInfo),

    #[serde(untagged)]
    Multi(MultiFileInfo),
}

impl Info {
    pub fn piece_length(&self) -> usize {
        match self {
            Info::Single(f) => f.piece_length as usize,
            Info::Multi(f) => f.piece_length as usize,
        }
    }

    pub fn pieces(&self) -> &[u8] {
        match self {
            Info::Single(f) => &f.pieces,
            Info::Multi(f) => &f.pieces,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct MetaInfo {
    pub info: Info,
    pub announce: String,
    #[serde(rename = "announce-list")]
    #[serde(default)]
    pub announce_list: Vec<Vec<String>>,
    #[serde(default)]
    pub creation_date: usize,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub created_by: String,
    #[serde(default)]
    pub encoding: String,
}
