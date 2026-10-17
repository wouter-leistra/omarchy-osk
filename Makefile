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

# omarchy-osk: install for the current user (no root needed)
USER_PREFIX ?= $(HOME)/.local
install-user: wvkbd-omarchy
	mkdir -p $(USER_PREFIX)/bin $(USER_PREFIX)/share/omarchy-osk/dict
	install -m 755 wvkbd-omarchy $(USER_PREFIX)/bin/omarchy-oskd
	install -m 755 bin/omarchy-osk bin/omarchy-osk-toggle bin/omarchy-osk-restart bin/omarchy-osk-lock bin/omarchy-osk-autoshow $(USER_PREFIX)/bin/
	mkdir -p $(HOME)/.config/systemd/user
	install -m 644 omarchy/systemd/omarchy-osk-autoshow.service $(HOME)/.config/systemd/user/
	install -m 644 dict/*.txt dict/README.md $(USER_PREFIX)/share/omarchy-osk/dict/
	mkdir -p $(USER_PREFIX)/share/omarchy-osk/layouts $(USER_PREFIX)/share/omarchy-osk/locales
	install -m 644 data/languages.txt data/popups.txt data/README.md $(USER_PREFIX)/share/omarchy-osk/
	install -m 644 data/layouts/*.txt $(USER_PREFIX)/share/omarchy-osk/layouts/
	install -m 644 data/locales/*.txt $(USER_PREFIX)/share/omarchy-osk/locales/
