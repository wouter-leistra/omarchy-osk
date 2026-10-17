include config.mk

BUILDDIR=build-${LAYOUT}

NAME=wvkbd
BIN=${NAME}-${LAYOUT}
SRC=.
MAN1 = ${NAME}.1

PKGS = wayland-client xkbcommon pangocairo

WVKBD_SOURCES += $(wildcard $(SRC)/*.c)
WVKBD_HEADERS += $(wildcard $(SRC)/*.h)
WVKBD_DIR_SOURCES = $(foreach src, $(WVKBD_SOURCES), $(addprefix $(BUILDDIR)/, $(src)))

PKG_CONFIG ?= pkg-config
CFLAGS += -std=gnu99 -Wall -g -DWITH_WAYLAND_SHM -DLAYOUT=\"layout.${LAYOUT}.h\" -DKEYMAP=\"keymap.${LAYOUT}.h\"
CFLAGS += $(shell $(PKG_CONFIG) --cflags $(PKGS))
LDFLAGS += $(shell $(PKG_CONFIG) --libs $(PKGS)) -lm -lutil -lrt

WAYLAND_HEADERS = $(wildcard proto/*.xml)

HDRS = $(WAYLAND_HEADERS:.xml=-client-protocol.h)
WAYLAND_SRC = $(HDRS:.h=.c)
SOURCES = $(WVKBD_SOURCES) $(WAYLAND_SRC)
WAYLAND_OBJS = $(addprefix $(BUILDDIR)/, $(WAYLAND_SRC:.c=.o))
OBJECTS = $(WVKBD_DIR_SOURCES:.c=.o) $(WAYLAND_OBJS)

SCDOC=scdoc
DOCS = wvkbd.1


all: ${BIN} ${DOCS}

$(BUILDDIR)/config.h:
	mkdir -p $(BUILDDIR)
	cp config.$(LAYOUT).h $@

$(BUILDDIR)/%.o: %.c
	mkdir -p $(dir $@)
	$(CC) -I $(CURDIR) -I $(CURDIR)/$(BUILDDIR) -c $(CFLAGS) -o $@ $<

proto/%-client-protocol.c: proto/%.xml
	wayland-scanner private-code < $< > $@

proto/%-client-protocol.h: proto/%.xml
	wayland-scanner client-header < $< > $@

$(OBJECTS): $(HDRS) $(WVKBD_HEADERS)

wvkbd-${LAYOUT}: $(BUILDDIR)/config.h $(OBJECTS) layout.${LAYOUT}.h
	$(CC) -o wvkbd-${LAYOUT} $(OBJECTS) $(LDFLAGS)

clean:
	rm -rf "$(BUILDDIR)" wvkbd-$(LAYOUT)

# omarchy-osk: offline checks of the layout data and the gesture decoder
check:
	mkdir -p $(BUILDDIR)
	$(CC) -I. -o $(BUILDDIR)/layout_test tests/layout_test.c langs.c swipe.c -lm
	$(BUILDDIR)/layout_test data --check
	$(CC) -I. -o $(BUILDDIR)/decode_test tests/decode_test.c swipe.c -lm
	$(BUILDDIR)/decode_test dict/fi.txt kiitos talo koira tämä | tail -1

format:
	clang-format -i $(WVKBD_SOURCES) $(WVKBD_HEADERS)

%: %.scd
	$(SCDOC) < $< > $@

install: all
	mkdir -p ${DESTDIR}${PREFIX}/bin
	cp -f ${NAME}-${LAYOUT} ${DESTDIR}${PREFIX}/bin
	chmod 755 ${DESTDIR}${PREFIX}/bin/${NAME}-${LAYOUT}
	mkdir -p "${DESTDIR}${MANPREFIX}/man1"
	sed "s/VERSION/${VERSION}/g" < ${MAN1} > ${DESTDIR}${MANPREFIX}/man1/${MAN1}
	chmod 644 ${DESTDIR}${MANPREFIX}/man1/${MAN1}

# omarchy-osk: install for the current user (no root needed). The daemon
# is the Rust one in rust/ (needs cargo); the C version it was ported from
# is kept as the reference the Rust one is tested against (check-rust), and
# install-user-c installs it instead.
USER_PREFIX ?= $(HOME)/.local
CARGO ?= cargo

rust:
	cd rust && $(CARGO) build --release

install-user: rust install-user-files
	install -m 755 rust/target/release/omarchy-oskd $(USER_PREFIX)/bin/omarchy-oskd

install-user-c: wvkbd-omarchy install-user-files
	install -m 755 wvkbd-omarchy $(USER_PREFIX)/bin/omarchy-oskd

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

# the Rust port against the C version: unit tests, then the same scripts,
# keymaps and pixels (tests/ref/README.md)
check-rust: rust
	mkdir -p $(BUILDDIR)
	cd rust && $(CARGO) test --release
	tests/ref/build.sh $(BUILDDIR)/osk-ref
	$(CC) -O1 -I. -o $(BUILDDIR)/decode-ref tests/ref/decode_ref.c swipe.c -lm
	tests/ref/compare.py --c $(BUILDDIR)/osk-ref
	tests/ref/decode_compare.py --c $(BUILDDIR)/decode-ref

.PHONY: rust install-user install-user-c install-user-files check check-rust
