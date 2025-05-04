pub struct Info {
    temp: u32,
}

pub struct Metainfo<'a> {
    pub info: Info,
    pub announce: String,
    pub announce_list: Vec<Vec<&'a [u8]>>,
    pub creation_date: Option<usize>,
    pub comment: Option<&'a [u8]>,
    pub created_by: Option<&'a [u8]>,
}
