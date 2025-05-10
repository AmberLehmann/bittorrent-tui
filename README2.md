Final Class Project: BitTorrent, Due: May 15, 09:00 AM

## Description

For this project you need to implement a BitTorrent client. Successful implementations need to interoperate with commercial/open-source BitTorrent clients. Your project will be graded on its download performance compared to the reference client. Your client should have download performance comparable to or better than the reference client. You need to devise experiments to demonstrate that your client’s performance is ‘fast enough’ and ‘stable’ in comparison to the reference BitTorrent client.

Along with your implementation, you must submit a report that details:
1. List of supported features
2. Design and implementation choices
3. Testing/measurements/experiments, especially those distinct from the demo
4. Problems that you encountered (and if/how you addressed them)
5. Known bugs or issues
6. Contributions made by each member

## Features

### Core Features

1. Communicate with the tracker using HTTP (with support for compact format)
2. Download a file from other instances of your client
3. Download a file from official BitTorrent clients

### References

[1] BitTorrentSpecification - Theory.org Wiki. https://wiki.theory.org/BitTorrentSpecification.
[2] BitTyrant. http://bittyrant.cs.washington.edu/.
[3] The BitTorrent Protocol Specification. http://www.bittorrent.org/beps/bep_0003.html.
[4] The HTTPS-Only Standard. https://https.cio.gov/.
[5] Dave Levin, Katrina LaCurts, Neil Spring, and Bobby Bhattacharjee. BitTorrent is an Auction: Analyzing and Improving BitTorrent’s Incentives. In ACM SIGCOMM Computer Communication Review, volume 38, pages 243–254. ACM, 2008.

### Extra Credit

1. Support for UDP trackers.
2. Support for HTTPS trackers (see [4] for an intro to HTTPS which happens to be the right level of technical). Wrap your hand-coded HTTP with a TLS library; don’t use an HTTPS library.
3. Multi-file mode.
4. A rarest-first strategy (see “Piece Downloading Strategy” in [1]).
5. Endgame mode (see “End Game” in [1]).
6. BitTyrant mode (see [2]).
7. PropShare [5] and design experiments to compare performance to the official client.
8. We may allot bonus points for other aspects that go above and beyond; post if you have questions about what could qualify.

## Resources

### Specification Information

You can find information on the BitTorrent specification in [1], [2], and [3].

### Reference Client

You can download a reference BitTorrent client for Windows/MacOS/Linux at:

https://www.transmissionbt.com/
https://www.qbittorrent.org/

You can use this client to help understand the protocol (e.g., via Wireshark/tcpdump packet captures) and for comparison in your experiments.
On Ubuntu, you should be able to install it by running: sudo apt-get install {transmission, qbittorrent}

### Other Resources

1. Torrent metafile details: https://fileformats.fandom.com/wiki/Torrent_file
2. Sample metafiles: https://github.com/webtorrent/webtorrent-fixtures/tree/master/fixtures

## Grading

At the end of the semester, each group will meet with the TAs to demonstrate their BitTorrent client implementation. Additionally, each group will discuss the information contained in their report (e.g., design choices) during this meeting. The TAs will make a post on Piazza with more details about scheduling the meetings closer to the deadline.

## Additional Requirements

1. Your code must be submitted as a series of commits that are pushed to the origin/main branch of your team’s Git repository. We consider your latest commit prior to the due date/time to represent your submission.
2. You may implement the project in any language of your choosing, minding the restrictions on libraries in §3.2.
3. If setup/use is non-obvious, your project should have README(s) with instructions, and/or accomplish the same in UI.
4. You are not allowed to work with anyone outside of your project group or to copy code except from the provided materials and small snippets for sockets code (which should be from the provided materials as well, e.g., the sockets book).
5. Your report is to be submitted to ELMS.


## References - My Notes

### [0] Wikipedia (sorry just a quick summary)
https://en.wikipedia.org/wiki/BitTorrent

Protocol for P2P file sharing. User starts up a client. BitTorrent trackers provide a list of available files and allow you to find peers ("seeds") who can transfer them. Decentralized, so is fast.

File being distributed is divided into "pieces". As each peer receives a piece, it becomes a distributor for that piece also. Pieces are protected by checksum-style hashing contained in the torrent descriptor.

Pieces may arrive non-sequentially. Throughout a single download, pieces should be the same size.

https://en.wikipedia.org/wiki/Torrent_file

A .torrent file (meta-info file) contains metadata about files and folders to be distributed, and also usually about the network location of "trackers" (type of server) who can help participants find each other and form "swarms".

### [1] BitTorrentSpecification, [3] The BitTorrent Protocol Specification
http://bittyrant.cs.washington.edu/
http://www.bittorrent.org/beps/bep_0003.html
https://wiki.theory.org/BitTorrentSpecification

#### Summary

A BitTorrent file distribution consists of these entities:

1. An ordinary web server
2. A static 'metainfo' file
3. A BitTorrent tracker
4. An 'original' downloader
5. The end user web browsers
6. The end user downloaders

There are ideally many end users for a single file.

To start serving, a host goes through the following steps:

1. Start running a tracker (or, more likely, have one running already).
2. Start running an ordinary web server, such as apache, or have one already.
3. Associate the extension .torrent with mimetype application/x-bittorrent on their web server (or have done so already).
4. Generate a metainfo (.torrent) file using the complete file to be served and the URL of the tracker.
5. Put the metainfo file on the web server.
7. Link to the metainfo (.torrent) file from some other web page.
8. Start a downloader which already has the complete file (the 'origin').

To start downloading, a user does the following:

1. Install BitTorrent (or have done so already).
2. Surf the web.
3. Click on a link to a .torrent file.
4. Select where to save the file locally, or select a partial download to resume.
5. Wait for download to complete.
6. Tell downloader to exit (it keeps uploading until this happens).

#### Terminology
"peer" - a client participating in a download, vs "the client" being the one on the local machine. "piece" - portion of downloaded data, verifiable by SHA1 hash, "block" is a portion of data that a client can request from a peer, two or more "block"s make up a verifiable "piece".

#### Bencoding
supports the following types: byte strings, integers, lists, and dictionaries

1. Byte Strings: <ASCII length>:<string>
2. Integers: i<base 10 ASCII>e (note: handle as at least signed 64bit)
3. Lists: l<stuff that is <bencoded thing>>e
4. Dictionaries: d<stuff that is <bencoded string><bencoded thing>>e

#### Metainfo Files
all bencoded, .torrent. is a dictionary with these keys. all char strings are UTF-8.
combined with https://www.bittorrent.org/beps/bep_0003.html info

some optional things:
3. announce-list: (optional) this is an extention to the official specification, offering backwards-compatibility. (list of lists of strings).
4. creation date: (optional) the creation time of the torrent, in standard UNIX epoch format (integer, seconds since 1-Jan-1970 00:00:00 UTC)
5. comment: (optional) free-form textual comments of the author (string)
6. created by: (optional) name and version of the program used to create the .torrent (string)
7. encoding: (optional) the string encoding format used to generate the pieces part of the info dictionary in the .torrent metafile (string)

required things:
1. "announce": announce URL of the tracker (string)
2. "info": a dictionary, describes the file(s). There are two possible forms: one for the case of a 'single-file' torrent with no directory structure, and one for the case of a 'multi-file' torrent (see below for details)

inside "info":

for both "single" and "multiple" modes:
1. piece length: number of bytes in each piece (integer).
    i. The piece length specifies the nominal piece size, and is usually a power of 2. Current best-practice is to keep the piece size to 512KB or less, for torrents around 8-10GB. The most common sizes are 256 kB, 512 kB, and 1 MB. [3] says "most commonly 2^18 = 256 K".
    ii. Every piece is of equal length except for the final piece, which is irregular. The number of pieces is thus determined by 'ceil(total length / piece size)'.
    iii. For the purposes of piece boundaries in the multi-file case, consider the file data as one long continuous stream, composed of the concatenation of each file, so pieces may overlap file boundaries.
2. pieces: string consisting of the concatenation of all 20-byte SHA1 hash values, one per piece (byte string, i.e. not urlencoded)
    i. Each piece has a corresponding SHA1 hash of the data contained within that piece. These hashes are concatenated to form the pieces value. Note that this is not a list but rather a single string. The length of the string must be a multiple of 20.
3. private: (optional - NOTE: don't really worry about it) this field is an integer. If it is set to "1", the client MUST publish its presence to get other peers ONLY via the trackers explicitly described in the metainfo file. If this field is set to "0" or is not present, the client may obtain peer from other means, e.g. PEX peer exchange, dht. Here, "private" may be read as "no external peer source". NOTE: There is much debate surrounding private trackers.
        
for "single" mode only:
1. name: the filename. This is purely advisory. (string)
2. length: length of the file in bytes (integer)
3. md5sum: (optional) a 32-character hexadecimal string corresponding to the MD5 sum of the file. This is not used by BitTorrent at all, but it is included by some programs for greater compatibility.

for "multiple" mode only:
1. name: the name of the directory in which to store all the files. This is purely advisory. (string)
2. files: a list of dictionaries, one for each file. Each dictionary in this list contains the following keys:
    i. length: length of the file in bytes (integer)
    ii. md5sum: (optional) a 32-character hexadecimal string corresponding to the MD5 sum of the file. This is not used by BitTorrent at all, but it is included by some programs for greater compatibility.
    iii. path: a list containing one or more string elements that together represent the path and filename. Each element in the list corresponds to either a directory name or (in the case of the final element) the filename. For example, a the file "dir1/dir2/file.ext" would consist of three string elements: "dir1", "dir2", and "file.ext". This is encoded as a bencoded list of strings such as l4:dir14:dir28:file.exte

NOTE: "Multi-file mode" counts for EC.

#### Tracker
is an HTTP/HTTPS service which responds to HTTP GET requests.

The base URL consists of the "announce URL" as defined in the metainfo (.torrent) file. The parameters are then added to this URL, using standard CGI methods (i.e. a '?' after the announce URL, followed by 'param=value' sequences separated by '&'). 

all binary data in the URL (particularly info_hash and peer_id) must be properly escaped. This means any byte not in the set 0-9, a-z, A-Z, '.', '-', '_' and '~', must be encoded using the "%nn" format, where nn is the hexadecimal value of the byte.

For a 20-byte hash of \x12\x34\x56\x78\x9a\xbc\xde\xf1\x23\x45\x67\x89\xab\xcd\xef\x12\x34\x56\x78\x9a,
The right encoded form is %124Vx%9A%BC%DE%F1%23Eg%89%AB%CD%EF%124Vx%9A 

##### Tracker Request
parameters used in the client->tracker GET request:
1. "info_hash": urlencoded 20-byte SHA1 hash of the value of the "info" key from the Metainfo file.
2. "peer_id": urlencoded 20-byte string used as a unique ID for the client, generated by the client at startup. This is allowed to be any value, however, one may rightly presume that it must at least be unique for your local machine, thus should probably incorporate things like process ID and perhaps a timestamp recorded at startup. See later in TCP section for more info.
3. "port": that the client is listening on, typically 6881-6889
4. "uploaded": total amount (preferably in total number of bytes) uploaded (since the client sent the 'started' event to the tracker) in base ten ASCII
5. "downloaded": same but downloaded
6. "left": number of bytes needed to download to be 100% complete and get all the included files in the torrent in base ten ASCII
7. "compact": 1 or 0
    i. The peers list is replaced by a peers string with 6 bytes per peer. 
    ii. The first four bytes are the host (in network byte order), the last two bytes are the port (again in network byte order). 
    iii. It should be noted that some trackers only support compact responses (for saving bandwidth) and either refuse requests without "compact=1" or simply send a compact response unless the request contains "compact=0" (in which case they will refuse the request.)
8. "no_peer_id": Indicates that the tracker can omit peer id field in peers dictionary. This option is ignored if compact is enabled.
9. "event": ("started", "stopped", "completed", else don't specify/leave empty):
    i. "started": is first request to tracker
    ii. "stopped": client informs tracker that client is shutting down
    iii. "completed": sent to tracker when download completes, only if it wasn't already complete when the client started
10. "ip": (optional), IP of the client in dotted quad format or rfc3513 defined hexed IPv6 address. see notes on the [actual page](https://wiki.theory.org/BitTorrentSpecification#Peer_wire_protocol_.28TCP.29) for when not/needed.
11. "numwant": (optional), number of peers that the client would like to receive from the tracker. typical default is 50.
12. "key": (optional), additional identification that is not shared with any other peers intended to allow a client to prove their identity should their IP address change
13. "trackerid": (optional), if any previous announce contained a tracker id, it should be set here

##### Tracker Response
"text/plain" document consisting of a bencoded dictionary with the following keys:
1. "failure reason": human-readable error message as to why the request failed. if present, no other keys should be.
2. "warning message": (optional), like the one above but response still can be processed
3. "interval": in seconds that the client should wait between sending regular requests to the tracker
4. "min interval": (optional), min announce interval. If present, clients must not reannounce more frequently than this.
5. "tracker id": string that the client should send back on its next announcements. If absent and a previous announce sent a tracker id, client should not discard the old value; keep using it
6. "complete": integer number of peers with the entire file (seeders)
7. "incomplete": integer number of non-seeder peers (leechers)
8. "peers" not compact vers: list of dicts, each with keys:
    i. "peer id": urlencoded 20-byte string used as a unique ID for the client, generated by the client at startup
    ii. "ip": either IPv6 (hexed) or IPv4 (dotted quad) or DNS name (string)
    iii. "port": integer
9. "peers" compact vers: string of multiples of 6 bytes, 4 for IP + 2 for port, all in network order (big endian)

Implementer's Note: Even 30 peers is plenty, the official client version 3 in fact only actively forms new connections if it has less than 30 peers and will refuse connections if it has 55. This value is important to performance. When a new piece has completed download, HAVE messages (see below) will need to be sent to most active peers. As a result the cost of broadcast traffic grows in direct proportion to the number of peers. Above 25, new peers are highly unlikely to increase download speed.

##### Tracker Scrape

By convention most trackers support another form of request, which queries the state of a given torrent (or all torrents) that the tracker is managing. This is referred to as the "scrape page" because it automates the otherwise tedious process of "screen scraping" the tracker's stats page. 

scrape URL is also a HTTP GET. Begin with the announce URL. Find the last '/' in it. If the text immediately following that '/' isn't 'announce' it will be taken as a sign that that tracker doesn't support the scrape convention. If it does, substitute 'scrape' for 'announce' to find the scrape page. 

add optional parameter "info_hash": urlencoded 20-byte SHA1 hash of the value of the "info" key from the Metainfo file, which restricts the tracker's report to that particular torrent. strongly encouraged. defacto standard: several info hashes is valid.

response is bencoded dict again, where each key is a 20 byte info hash and has a sub dict with keys:
1. complete: number of peers with the entire file, i.e. seeders (integer)
2. downloaded: total number of times the tracker has registered a completion ("event=complete", i.e. a client finished downloading the torrent)
3. incomplete: number of non-seeder peers, aka "leechers" (integer)
4. name: (optional) the torrent's internal name, as specified by the "name" file in the info section of the .torrent file

Here's an example:

d5:filesd20:....................d8:completei5e10:downloadedi50e10:incompletei10eeee
Where .................... is the 20 byte info_hash and there are 5 seeders, 10 leechers, and 50 complete downloads. 

There are some unoffician extra keys, "failure reason" string and "flags" dict maybe with "min_request_interval" key

#### TCP Peer Wire Protocol
facilitates the exchange of pieces as described in the 'metainfo file.

the original specification also used the term "piece" when describing the peer protocol, but as a different term than "piece" in the metainfo file. For that reason, the term "block" will be used in this specification to describe the data that is exchanged between peers over the wire.

A client must maintain state information for each connection that it has with a remote peer: 
1. "choked":  Whether or not the remote peer has choked this client. When a peer chokes the client, it is a notification that no requests will be answered until the client is unchoked. The client should not attempt to send requests for blocks, and it should consider all pending (unanswered) requests to be discarded by the remote peer.
2. "interested": Whether or not the remote peer is interested in something this client has to offer. This is a notification that the remote peer will begin requesting blocks when the client unchokes them.

More realistically something like this:
1. am_choking: this client is choking the peer
2. am_interested: this client is interested in the peer
3. peer_choking: peer is choking this client
4. peer_interested: peer is interested in this client

connections start out as "choked" and "not interested". a block is downloaded by the client when the client is interested in a peer, and that peer is not choking the client. A block is uploaded by a client when the client is not choking a peer, and that peer is interested in the client. 

state information (as to whether or not the client is interested in them) should be kept up-to-date with each peer even when the client is choked.

protocol consists of an initial handshake. After that, peers communicate via an exchange of length-prefixed messages.

Unless specified otherwise, all integers in the peer wire protocol are encoded as four byte big-endian values. This includes the length prefix on all messages that come after the handshake. 

##### Handshake

must be the first message transmitted by the client. It is (49+len(pstr)) bytes long

<pstrlen><pstr><reserved><info_hash><peer_id>

1. pstrlen: string length of <pstr>, as a single raw byte. In version 1.0 of the BitTorrent protocol, pstrlen = 19. 
2. pstr: string identifier of the protocol. In version 1.0 of the BitTorrent protocol, pstr = "BitTorrent protocol". 
3. reserved: eight (8) reserved bytes. All current implementations use all zeroes. Each bit in these bytes can be used to change the behavior of the protocol. An email from Bram suggests that trailing bits should be used first, so that leading bits may be used to change the meaning of trailing bits.
4. info_hash: 20-byte SHA1 hash of the info key in the metainfo file. This is the same info_hash that is transmitted in tracker requests.
5. peer_id: 20-byte string used as a unique ID for the client. This is usually the same peer_id that is transmitted in tracker requests (but not always e.g. an anonymity option in Azureus).

"the recipient must respond as soon as it sees the info_hash part of the handshake (the peer id will presumably be sent after the recipient sends its own handshake). The tracker's NAT-checking feature does not send the peer_id field of the handshake." <-- what? AAAAAAAAA

If a client receives a handshake with an info_hash that it is not currently serving, then the client must drop the connection. 

If the initiator of the connection receives a handshake in which the peer_id does not match the expected peerid (that it presumably got from the tracker), then the initiator is expected to drop the connection.

###### peer_id

20 bytes long. Arbitraty. Two conventions how to encode client and client version information into the peer_id.

Azureus-style uses the following encoding: '-', two characters for client id, four ascii digits for version number, '-', followed by random numbers. For example: '-AZ2060-'... 

Shadow's style uses the following encoding: one ascii alphanumeric for client identification, up to five characters for version number (padded with '-' if less than five), followed by three characters (commonly '---', but not always the case), followed by random characters. Each character in the version string represents a number from 0 to 63. '0'=0, ..., '9'=9, 'A'=10, ..., 'Z'=35, 'a'=36, ..., 'z'=61, '.'=62, '-'=63. For example: 'S58B-----'... for Shadow's 5.8.11 

Various other styles exist.

##### Messages

<length prefix><message ID><payload>
1. length prefix is a four byte big-endian value
2. message ID is a single decimal byte
3. payload is message dependent

Message Types:
1. "keep-alive": <len=0000>: There is no message ID and no payload. (to prevent timeout, generally set to ~2 mins) [3] says "note that timeouts can be done much more quickly when data is expected."
2. "choke": <len=0001><id=0>
3. "unchoke": <len=0001><id=1>
4. "interested": <len=0001><id=2>
5. "not interested": <len=0001><id=3>
6. "have": <len=0005><id=4><piece index>: The payload is the zero-based index of a piece that has just been successfully downloaded and verified via the hash.
    i. That is the strict definition, in reality some games may be played. In particular because peers are extremely unlikely to download pieces that they already have, a peer may choose not to advertise having a piece to a peer that already has that piece. At a minimum "HAVE suppression" will result in a 50% reduction in the number of HAVE messages, this translates to around a 25-35% reduction in protocol overhead. At the same time, it may be worthwhile to send a HAVE message to a peer that has that piece already since it will be useful in determining which piece is rare.
7. "bitfield": <len=0001+X><id=5><bitfield>: (optional), sent immediately after the handshaking sequence is completed, and before any other messages are sent.
    i. The payload is a bitfield representing the pieces that have been successfully downloaded. The high bit in the first byte corresponds to piece index 0. Bits that are cleared indicated a missing piece, and set bits indicate a valid and available piece. Spare bits at the end are set to zero. 
    ii. Clients should drop the connection if they receive bitfields that are not of the correct size, or if the bitfield has any of the spare bits set.
8. "request": <len=0013><id=6><index><begin><length>
    i. "index": integer specifying the zero-based piece index
    ii. "begin": integer specifying the zero-based byte offset within the piece
    iii. "length": integer specifying the requested length.
    iv. Strictly, the specification allows 2^15 (32KB) requests. The reality is near all clients will now use 2^14 (16KB) requests. Due to clients that enforce that size, it is recommended that implementations make requests of that size. Due to smaller requests resulting in higher overhead due to tracking a greater number of requests, implementers are advised against going below 2^14 (16KB).... something something it's more complex than that, see [page](https://wiki.theory.org/BitTorrentSpecification#Peer_wire_protocol_.28TCP.29).
9. "piece": <len=0009+X><id=7><index><begin><block>
    i. "index": integer specifying the zero-based piece index
    ii. "begin": integer specifying the zero-based byte offset within the piece
    iii. "block": block of data, which is a subset of the piece specified by index.
10. "cancel": <len=0013><id=8><index><begin><length>: like a "request" but says actually nvm don't send me that
11. "port": <len=0003><id=9><listen-port>: sent by newer versions of the Mainline that implements a DHT tracker. The listen port is the port this peer's DHT node is listening on. This peer should be inserted in the local routing table (if DHT tracker is supported). 

#### Algorithms

##### Queuing

In general peers are advised to keep a few unfullfilled requests on each connection. This is done because otherwise a full round trip is required from the download of one block to begining the download of a new block (round trip between PIECE message and next REQUEST message). On links with high BDP (bandwidth-delay-product, high latency or high bandwidth), this can result in a substantial performance loss. 

Implementer's note: This is the most crucial performance item. A static queue of 10 requests is reasonable for 16KB blocks on a 5mbps link with 50ms latency. Links with greater bandwidth are becoming very common so UI designers are urged to make this readily available for changing. Notably cable modems were known for traffic policing and increasing this might of alleviated some of the problems caused by this.

##### Super-Seeding

something something... only recommended for initial seeding servers.

##### Piece Downloading Strategy

Clients may choose to download pieces in random order.

A better strategy is to download pieces in rarest first order. The client can determine this by keeping the initial bitfield from each peer, and updating it with every have message. Then, the client can download the pieces that appear least frequently in these peer bitfields. Note that any Rarest First strategy should include randomization among at least several of the least common pieces, as having many clients all attempting to jump on the same "least common" piece would be counter productive. 

NOTE: "rarest-first strategy" from [1] counts for EC

##### End Game

When a download is almost complete, there's a tendency for the last few blocks to trickle in slowly. To speed this up, the client sends requests for all of its missing blocks to all of its peers. To keep this from becoming horribly inefficient, the client also sends a cancel to everyone else every time a block arrives. 

When to enter end game mode is an area of discussion. See [page](https://wiki.theory.org/BitTorrentSpecification#Peer_wire_protocol_.28TCP.29) for more discussion.

NOTE: "endgame mode" from [1] counts for EC

##### Choking and Optimistic Unchoking

Choking is done for several reasons. TCP congestion control behaves very poorly when sending over many connections at once. Also, choking lets each peer use a tit-for-tat-ish algorithm to ensure that they get a consistent download rate. 

There are several criteria a good choking algorithm should meet. It should cap the number of simultaneous uploads for good TCP performance. It should avoid choking and unchoking quickly, known as 'fibrillation'. It should reciprocate to peers who let it download. Finally, it should try out unused connections once in a while to find out if they might be better than the currently used ones, known as optimistic unchoking. 

The currently deployed choking algorithm avoids fibrillation by only changing choked peers once every ten seconds. 

Reciprocation and number of uploads capping is managed by unchoking the four peers which have the best upload rate and are interested. This maximizes the client's download rate. These four peers are referred to as downloaders, because they are interested in downloading from the client. 

Peers which have a better upload rate (as compared to the downloaders) but aren't interested get unchoked. If they become interested, the downloader with the worst upload rate gets choked. If a client has a complete file, it uses its upload rate rather than its download rate to decide which peers to unchoke. 

For optimistic unchoking, at any one time there is a single peer which is unchoked regardless of its upload rate (if interested, it counts as one of the four allowed downloaders). Which peer is optimistically unchoked rotates every 30 seconds. Newly connected peers are three times as likely to start as the current optimistic unchoke as anywhere else in the rotation. This gives them a decent chance of getting a complete piece to upload. 

##### Anti-snubbing

Occasionally a BitTorrent peer will be choked by all peers which it was formerly downloading from. In such cases it will usually continue to get poor download rates until the optimistic unchoke finds better peers. 

To mitigate this problem, when over a minute goes by without getting any piece data while downloading from a peer, BitTorrent assumes it is "snubbed" by that peer and doesn't upload to it except as an optimistic unchoke. This frequently results in more than one concurrent optimistic unchoke, (an exception to the exactly one optimistic unchoke rule mentioned above), which causes download rates to recover much more quickly when they falter.  <--- what? AAAAAAAAAAAA

#### Official Extensions

1. Fast Peers Extensions - allow a peer to more quickly bootstrap into a swarm by giving a peer a specific set of pieces which they will be allowed download regardless of choked status. They reduce message overhead by adding HaveAll and HaveNone messages and allow explicit rejection of piece requests whereas previously only implicit rejection was possible meaning that a peer might be left waiting for a piece that would never be delivered. 
2. Distributed Hash Table - allow for the tracking of peers downloading torrents without the use of a standard tracker.
3. Connection Obfuscation - allows the creation of obfuscated (encrypted) connections between peers. This can be used to bypass ISPs throttling BitTorrent traffic. 

#### Unofficial Extensions

see [1](https://wiki.theory.org/BitTorrentSpecification#Peer_wire_protocol_.28TCP.29)

### [2] BitTyrant
https://bittyrant.cs.washington.edu/

protocol compatible BitTorrent client that is optimized for fast download performance. 

NOTE: "BitTyrant mode" is for EC

peers which upload more to you get more of your bandwidth. differs from existing clients in its selection of which peers to unchoke and send rates to unchoked peers.

BitTyrant will rank all peers by their receive / sent ratios, preferentially unchoking those peers with high ratios. For example, a peer sending data to you at 20 KBps and receiving data from you at 10 KBps will have a ratio of 2, and would be unchoked before unchoking someone uploading at 10 KBps (ratio 1). Further, BitTyrant dynamically adjusts its send rate, giving more data to peers that can and do upload quickly and reducing send rates to others. 

However, our current BitTyrant implementation always contributes excess capacity, even when it might not improve performance. Our goal is to improve performance, not minimize upload contribution. 

Code linked as a zip file, paper linked [here](http://www.michaelpiatek.com//papers/BitTyrant.pdf)

Might be worth looking at their "evaluation" section to help with our report? Though maybe it's more than what we can do.


### [4] The HTTPS-Only Standard (for Support for HTTPS trackers EC)
https://https.cio.gov/

NOTE: EC: Support for HTTPS trackers (see [4] for an intro to HTTPS which happens to be the right level of technical). Wrap your hand-coded HTTP with a TLS library; don’t use an HTTPS library.

The link just describes HTTPS.

### [5] Dave Levin, Katrina LaCurts, Neil Spring, and Bobby Bhattacharjee. BitTorrent is an Auction: Analyzing and Improving BitTorrent’s Incentives. In ACM SIGCOMM Computer Communication Review, volume 38, pages 243–254. ACM, 2008.
http://ccr.sigcomm.org/online/files/p243-levin.pdf

NOTE: EC: "Propshare [5]", see Algorithm 4 Proportional share auction clearing.

## Rough Outline from Discord

Rust 1.75.0
Allowed Crates: mio, serde, serde_json, rand, log, libc, bytes, clap, thiserror, gethostname, zerocopy

"urlencoding is also fine."
"Is there a preferred bencode alternative?" - "No opinions here—I ain't no rustacean." (just use bencode I guess)
"can we use the regex rust crate" - "I'm sure you can do this fine with other methods, but sure."
"tokio and async is allowed"
