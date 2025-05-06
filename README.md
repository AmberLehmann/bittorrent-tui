## Rusty Implementation of BitTorrent 
WIP BitTorrent client implementation for CMSC417 written in Rust.

## Usage
To run the BitTorrent client in TUI mode: 
`cargo run`


| Key | Action |
| -------- | ------- |
| `q`  | Quit out of TUI |
| `d` or `1` | Navigate to Downloads Tab |
| `t` or `2` | Navigate to Torrent Tab |
| `l` or `3` | Navigate to Logs Tab |
| `j` | Scroll Down |
| `k` | Scroll Up |


## Logging
The Logs Tab will include information about the state of the program. If an error occurs, an Error will be printed here.
If anything is not working as expected, check the Logs tab for Errors.

## Supported Features
- Performant TUI interface
- Logging with 5 log levels (trace, debug, info, warning, error)

## Unsupported Features (May come soon)
- Optional arguments passed through command line
- Support for UDP trackers
- Support for HTTP trackers
- Multi-file Mode
- Rarest first piece downloading strategy
- Endgame mode
- BitTyrant
- PropShare

