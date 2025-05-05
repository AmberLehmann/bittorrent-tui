all:
	cargo build --release -Znext-lockfile-bump
	cp ./target/release/btclient . -fl

debug:
	cargo build -Znext-lockfile-bump
	cp ./target/debug/btclient . -fl
