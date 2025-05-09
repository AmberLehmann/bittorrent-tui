use crate::{logger::Logger, tui::App};
use color_eyre::Result;

mod args;
mod handshake;
mod hashing;
mod logger;
mod metainfo;
mod popup;
mod theme;
mod torrent;
mod tracker;
mod tui;

pub type PeerId20 = [u8; 20];
pub type HashedId20 = [u8; 20];

#[tokio::main]
async fn main() -> Result<()> {
    // let args = Args::parse();
    let mut terminal = ratatui::init();

    let (tx, rx) = std::sync::mpsc::channel();
    Logger::new(tx).init().unwrap();
    let mut app = App::new(rx);
    let result = app.run(&mut terminal).await;

    ratatui::restore();
    result
}
