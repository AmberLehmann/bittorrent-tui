## Rusty Implementation of BitTorrent 
WIP BitTorrent client implementation written in Rust.

## Usage
To run the BitTorrent client in TUI mode: 
`cargo run`

## Key Mappings
![TUI prototype](/image.png)
### Main Menu

| Key | Action |
| -------- | ------- |
| `q` or `Esc` | Quit out of TUI |
| `d` or `1` | Navigate to Downloads Tab |
| `t` or `2` | Navigate to Torrent Tab |
| `l` or `3` | Navigate to Logs Tab |
| `j` | Scroll Down |
| `k` | Scroll Up |
| `o` | Open `.torrent` file from path |

### Open Torrent Popup

| Key | Action |
| -------- | ------- |
| `Esc` | Close Popup |
| `Tab` | Select Next Field |
| `BackTab` | Select Previous Field |
| `t`,`f` | Set Compact Field |
| `a`,`4`,`6` | Set Ip Mode Field |
| `Enter` | Open Torrent |


## Logging
The Logs Tab will include information about the state of the program. If an error occurs, an Error will be printed here.
If anything is not working as expected, check the Logs tab for Errors.

## Supported Features
- Performant TUI interface
- Logging with 5 log levels (trace, debug, info, warning, error)
- Ability to connect with HTTP trackers 
- Ability to download from peers assigned by the tracker 


## Unsupported Features (May come soon)
- Optional arguments passed through command line
- Support for UDP trackers
- Support for HTTP trackers
- Multi-file Mode
- Rarest first piece downloading strategy
- Endgame mode
- BitTyrant
- PropShare

