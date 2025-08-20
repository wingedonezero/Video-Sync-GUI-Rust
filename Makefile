.PHONY: all core cli gui run-cli run-gui clean

all:
	cargo build --release

core:
	cargo build -p vsg-core --release

cli:
	cargo build -p vsg-cli --release

gui:
	cargo build -p vsg-gui --release

run-cli:
	./rust/target/release/vsg-cli --help || true

run-gui:
	./rust/target/release/vsg-gui || true

clean:
	cargo clean
