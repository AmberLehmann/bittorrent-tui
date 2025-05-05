use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SingleFileInfo {
    pub name: String,
    pub length: u32,
    pub md5sum: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileInfo {
    pub length: u32,
    pub md5sum: Option<String>,
    pub path: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct MultiFileInfo {
    pub name: String,
    pub files: Vec<FileInfo>,
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
    pub announce: Option<String>, // TODO: REMOVE AFTER TESTING
    pub announce_list: Option<Vec<Vec<String>>>,
    pub creation_date: Option<usize>,
    pub comment: Option<String>,
    pub created_by: Option<String>,
}
