# omarchy-osk: build with cargo, install for the current user (no root needed)

CARGO ?= cargo
USER_PREFIX ?= $(HOME)/.local
RELEASE = target/release

all: build

build:
	$(CARGO) build --release

install-user: build install-user-files
	install -m 755 $(RELEASE)/omarchy-oskd $(USER_PREFIX)/bin/omarchy-oskd

install-user-files:
	mkdir -p $(USER_PREFIX)/bin $(USER_PREFIX)/share/omarchy-osk/dict
	install -m 755 bin/omarchy-osk bin/omarchy-osk-toggle bin/omarchy-osk-restart bin/omarchy-osk-lock bin/omarchy-osk-autoshow \
		bin/omarchy-osk-plugins bin/omarchy-osk-boot-look $(USER_PREFIX)/bin/
	mkdir -p $(HOME)/.config/systemd/user
	install -m 644 omarchy/systemd/omarchy-osk-autoshow.service omarchy/systemd/omarchy-osk-boot-look.path \
		omarchy/systemd/omarchy-osk-boot-look.service $(HOME)/.config/systemd/user/
	install -Dm 755 unlock/omarchy-look $(USER_PREFIX)/lib/omarchy-osk/omarchy-look
	for p in menu image-picker lock; do \
		install -Dm 644 -t $(USER_PREFIX)/share/omarchy-osk/patches/$$p omarchy/patches/$$p/*.patch; \
	done
	# keep the patched Omarchy plugin copies current after `omarchy update`
	install -Dm 755 omarchy/hooks/post-update $(HOME)/.config/omarchy/hooks/post-update.d/omarchy-osk
	install -m 644 dict/*.txt dict/README.md $(USER_PREFIX)/share/omarchy-osk/dict/
	mkdir -p $(USER_PREFIX)/share/omarchy-osk/layouts $(USER_PREFIX)/share/omarchy-osk/locales
	install -m 644 data/languages.txt data/popups.txt data/README.md $(USER_PREFIX)/share/omarchy-osk/
	install -m 644 data/layouts/*.txt $(USER_PREFIX)/share/omarchy-osk/layouts/
	install -m 644 data/locales/*.txt $(USER_PREFIX)/share/omarchy-osk/locales/

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

.PHONY: all build install-user install-user-files check bench clean
