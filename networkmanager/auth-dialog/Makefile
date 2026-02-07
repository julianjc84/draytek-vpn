# DrayTek SSL VPN — NetworkManager auth-dialog build
#
# Produces: nm-draytek-auth-dialog

CC      ?= gcc
PKG_CFG := pkg-config
CFLAGS  := -Wall -Wextra -Wno-unused-parameter -O2
CFLAGS  += $(shell $(PKG_CFG) --cflags gtk4 glib-2.0)
LDFLAGS := $(shell $(PKG_CFG) --libs gtk4 glib-2.0)

TARGET  := nm-draytek-auth-dialog

.PHONY: all clean install

all: $(TARGET)

$(TARGET): main.c
	$(CC) $(CFLAGS) -o $@ $< $(LDFLAGS)

clean:
	rm -f $(TARGET)

install: all
	install -d $(DESTDIR)/usr/libexec
	install -m 755 $(TARGET) $(DESTDIR)/usr/libexec/
