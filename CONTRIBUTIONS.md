Prajwal-
Adjusting Hasher to be for 20bytes

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

