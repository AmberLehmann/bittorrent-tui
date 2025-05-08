use log::{debug, error, info, trace};
use sha1::{Digest, Sha1};
use std::sync::Arc;
use tokio::sync::{
    mpsc,
    mpsc::{UnboundedReceiver, UnboundedSender},
};

use crate::HashedId20;

// context: implementation of bittorrent which spawns 1 torrent thread per torrent, wherein
// there is a mio poll to listen for messages from peers.
// when it gets a "piece" message, it should hash it and compare that with what it expects
// the hash to be based on the .torrent file

// i am a torrent thread spawning a hashing thread
// except i don't exist yet so this is a fake function which is pretending to be the torrent thread
fn fake_spawner_thread() {
    let (tx_to_hasher, mut rx_from_main): (
        UnboundedSender<(usize, Arc<Vec<u8>>)>,
        UnboundedReceiver<(usize, Arc<Vec<u8>>)>,
    ) = mpsc::unbounded_channel(); // for sending over the u8 vecs and their matching index?

    let (tx_to_main, mut rx_from_hasher): (
        UnboundedSender<(usize, HashedId20)>,
        UnboundedReceiver<(usize, HashedId20)>,
    ) = mpsc::unbounded_channel(); // for sending back the u64 result and their matching index?

    // spawn a single thread that will do all out hashing
    tokio::spawn(async move { do_hashing(rx_from_main, tx_to_main).await });

    let mut sent_bool: bool = false;
    let mut got_tha_piece: bool = false;

    loop {
        // this would be the listener loop

        // consume all newly available hash values from the thread
        // break out of while if nothing to get
        // try_recv bc we don't want to block
        while let Ok((index, hash)) = rx_from_hasher.try_recv() {
            debug!("Got hash for piece index {}, hash: {:?}", index, hash);
            // verify hash in theory...
            got_tha_piece = true;
        }

        // for waker https://traffloat.github.io/api/master/mio/struct.Waker.html
        // one of the events in the loop will match the waker token when we make one
        // if we hit that waker do the consuming from above

        // wow we got some data from mio how nice, send it to the thread
        if !sent_bool {
            // the data
            let fake_piece_index: usize = 0;
            let fake_piece_data: Vec<u8> = vec![1, 2, 3];
            let fake_piece_ptr: Arc<Vec<u8>> = Arc::new(fake_piece_data);

            // send it
            let send_hash_res = tx_to_hasher.send((fake_piece_index, Arc::clone(&fake_piece_ptr)));
            match send_hash_res {
                Ok(_) => {
                    sent_bool = true;
                }
                Err(e) => {
                    // idk i guess we just drop it? assume hash doesn't work out?
                    error!("Tried to send to hashing thread, but: {}", e);
                }
            };
        }
    }
}

async fn do_hashing(
    mut rx: UnboundedReceiver<(usize, Arc<Vec<u8>>)>, // receive from main
    tx: UnboundedSender<(usize, HashedId20)>,         // to main
                                                      // for mio waker, just pass a waker into here https://traffloat.github.io/api/master/mio/struct.Waker.html
                                                      // waker: Arc<mio::Waker>
) {
    // recv not try_recv bc we *do* want to block

    loop {
        let result: Option<(usize, Arc<Vec<u8>>)> = rx.recv().await;
        let Some((index, arcpointer)) = result else {
            break;
        };
        let hash = hash_buffer(&arcpointer);
        tx.send((index, hash)).unwrap();
        // for mio waker, wake here
        // something like:
        // waker.wake().expect("unable to wake");
    }
}

fn hash_buffer(in_buf: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(in_buf);
    let hash = hasher.finalize();
    hash.into()
}
