PREFIX  ?= /usr/local
DESTDIR ?=
BIN      = target/release/openfetch

all: $(BIN)

$(BIN):
	cargo build --release --locked

test:
	cargo test --locked

install: $(BIN)
	install -Dm755 $(BIN) $(DESTDIR)$(PREFIX)/bin/openfetch
	install -Dm644 src/default_config.toml $(DESTDIR)$(PREFIX)/share/openfetch/config.toml
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/openfetch/LICENSE
	install -Dm644 NOTICE $(DESTDIR)$(PREFIX)/share/licenses/openfetch/NOTICE

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/openfetch
	rm -rf $(DESTDIR)$(PREFIX)/share/openfetch $(DESTDIR)$(PREFIX)/share/licenses/openfetch

clean:
	cargo clean

.PHONY: all test install uninstall clean
