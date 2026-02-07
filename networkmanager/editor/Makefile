# DrayTek SSL VPN — NetworkManager editor plugin build
#
# Produces three shared libraries:
#   libnm-vpn-plugin-draytek.so                — NMVpnEditorPlugin (no GTK)
#   libnm-vpn-plugin-draytek-editor.so         — NMVpnEditor (GTK3)
#   libnm-gtk4-vpn-plugin-draytek-editor.so    — NMVpnEditor (GTK4)

CC       ?= gcc
PKG_CFG  := pkg-config
CFLAGS   := -Wall -Wextra -Wno-unused-parameter -fPIC -shared -O2
CFLAGS   += -fvisibility=hidden

# Install paths
NM_PLUGIN_DIR := $(shell $(PKG_CFG) --variable=plugindir libnm 2>/dev/null || echo /usr/lib/x86_64-linux-gnu/NetworkManager)

# Plugin .so: glib + libnm (no GTK)
COMMON_CFLAGS  := $(shell $(PKG_CFG) --cflags glib-2.0 libnm gmodule-2.0)
COMMON_LDFLAGS := $(shell $(PKG_CFG) --libs glib-2.0 libnm gmodule-2.0) -ldl

# GTK3 editor: glib + libnm + GTK3
GTK3_CFLAGS  := $(shell $(PKG_CFG) --cflags glib-2.0 libnm gtk+-3.0)
GTK3_LDFLAGS := $(shell $(PKG_CFG) --libs glib-2.0 libnm gtk+-3.0)

# GTK4 editor: glib + libnm + GTK4
GTK4_CFLAGS  := $(shell $(PKG_CFG) --cflags glib-2.0 libnm gtk4)
GTK4_LDFLAGS := $(shell $(PKG_CFG) --libs glib-2.0 libnm gtk4)

PLUGIN_SO    := libnm-vpn-plugin-draytek.so
EDITOR_GTK3  := libnm-vpn-plugin-draytek-editor.so
EDITOR_GTK4  := libnm-gtk4-vpn-plugin-draytek-editor.so

.PHONY: all clean install

all: $(PLUGIN_SO) $(EDITOR_GTK3) $(EDITOR_GTK4)

$(PLUGIN_SO): nm-draytek-editor-plugin.c nm-vpn-plugin-utils.c nm-draytek-editor.h nm-vpn-plugin-utils.h
	$(CC) $(CFLAGS) $(COMMON_CFLAGS) \
		-DNM_VPN_PLUGIN_DIR=\"$(NM_PLUGIN_DIR)\" \
		-o $@ \
		nm-draytek-editor-plugin.c nm-vpn-plugin-utils.c \
		$(COMMON_LDFLAGS)

$(EDITOR_GTK3): nm-draytek-editor.c nm-draytek-editor.h
	$(CC) $(CFLAGS) $(GTK3_CFLAGS) \
		-o $@ \
		nm-draytek-editor.c \
		$(GTK3_LDFLAGS)

$(EDITOR_GTK4): nm-draytek-editor.c nm-draytek-editor.h
	$(CC) $(CFLAGS) $(GTK4_CFLAGS) \
		-DUSE_GTK4 \
		-o $@ \
		nm-draytek-editor.c \
		$(GTK4_LDFLAGS)

clean:
	rm -f $(PLUGIN_SO) $(EDITOR_GTK3) $(EDITOR_GTK4)

install: all
	install -d $(DESTDIR)$(NM_PLUGIN_DIR)
	install -m 755 $(PLUGIN_SO) $(DESTDIR)$(NM_PLUGIN_DIR)/
	install -m 755 $(EDITOR_GTK3) $(DESTDIR)$(NM_PLUGIN_DIR)/
	install -m 755 $(EDITOR_GTK4) $(DESTDIR)$(NM_PLUGIN_DIR)/
