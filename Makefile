# omarchy-osk: build with cargo, install for the current user (no root needed)

CARGO ?= cargo
RELEASE = target/release

all: build

build:
	$(CARGO) build --release

install-user: build
	tools/stage.sh target/stage/omarchy-osk
	target/stage/omarchy-osk/install.sh --from target/stage/omarchy-osk --files-only

# the release tarball (tools/release.sh also tests it on this machine and
# records the result in TESTED)
dist: build
	tools/stage.sh target/stage/omarchy-osk
	tar -czf target/omarchy-osk-x86_64.tar.gz -C target/stage omarchy-osk
	cd target && sha256sum omarchy-osk-x86_64.tar.gz > SHA256SUMS
	@echo target/omarchy-osk-x86_64.tar.gz

# unit tests, then the behaviour recorded from the original C version:
# the same keys, keymaps, pixels and swipe results (tests/golden.py)
check: build
	$(CARGO) test --release
	tests/golden.py --bin $(RELEASE)/osk-trace --decode-bin $(RELEASE)/osk-decode

# speed and resource use (tests/bench.py)
bench: build
	tests/bench.py

clean:
	$(CARGO) clean

.PHONY: all build install-user dist check bench clean
