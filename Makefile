.PHONY: build debug clean

build:
	cd rust && cargo build -p vsg-cli --release

debug:
	cd rust && cargo build -p vsg-cli

clean:
	cd rust && cargo clean
