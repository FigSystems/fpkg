install-debug:
	cargo build
	sudo mkdir -p /usr/local/bin
	sudo cp -f target/x86_64-unknown-linux-musl/debug/fpkg /usr/local/bin/
	sudo chown root:root /usr/local/bin/fpkg
	sudo chmod u+s /usr/local/bin/fpkg
	sudo cp tools/pkg_to_fpkg.sh /usr/local/bin/pkg_to_fpkg
	sudo chmod +x /usr/local/bin/pkg_to_fpkg

install-release:
	cargo build --release
	sudo mkdir -p /usr/local/bin
	sudo cp -f target/x86_64-unknown-linux-musl/release/fpkg /usr/local/bin/
	sudo chown root:root /usr/local/bin/fpkg
	sudo chmod u+s /usr/local/bin/fpkg
