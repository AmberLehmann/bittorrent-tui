use serde::Deserialize;
use serde_bytes::ByteBuf;

#[derive(Debug, Deserialize)]
pub struct SingleFileInfo {
    #[serde(rename = "piece length")]
    pub piece_length: u32,
    pub pieces: ByteBuf,
    pub private: Option<u32>,

    pub name: String,
    pub length: u64,
    pub md5sum: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct File {
    pub length: u64,
    pub md5sum: Option<String>,
    pub path: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct MultiFileInfo {
    #[serde(rename = "piece length")]
    pub piece_length: u32,
    pub pieces: ByteBuf,
    pub private: Option<u32>,

    pub name: String,
    pub files: Vec<File>,
}

#[derive(Debug, Deserialize)]
pub enum Info {
    #[serde(untagged)]
    Single(SingleFileInfo),

    #[serde(untagged)]
    Multi(MultiFileInfo),
}

#[derive(Debug, Deserialize)]
pub struct MetaInfo {
    pub info: Info,
    pub announce: String,
    #[serde(rename = "announce-list")]
    pub announce_list: Option<Vec<Vec<String>>>,
    pub creation_date: Option<usize>,
    pub comment: Option<String>,
    pub created_by: Option<String>,
}
