- Prajwal
  - Adjusting Hasher to be for 20bytes
  - Adjusting Hashing code for the torrent specs
  - Working on UDP impelementation in Rust
  - tests for TCP and UDP trackers

- Thomas
  - UI
  - Metainfo/Torrent deserialization
  - fixing hasher adjustment
  - logging

- Amber 
  - TrackerRequest/Response methods + deserialization
  - Creating custom error messages, types for TrackerError enum
  - Constant Refactoring
  - Started on tracker communication on torrent open
  - Started on the README.md
  - Started on handshake struct
  - Requested, and received handshake
  - Fixed bug where connection with peer would immediately close after handshake
  - Fixed deserialization function for TrackerResponse to handle older torrent formats

- Maya
  - Created message type (de)serialization + tests
  - Update metainfo deserialization to properly get peer_ids
  - Update handshaking to allow for incoming peers and filter improper/duplicate handshakes
  - Update incoming data handling to deal with framed data in streams
  - Fixed bug where sent data was sending improper packet sizes
  