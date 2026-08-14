.PHONY: test install build

PREFIX ?= /usr/local
CARGO ?= cargo

build:
	$(CARGO) build --release

test:
	$(CARGO) test
	$(CARGO) build
	WT="$(CURDIR)/target/debug/wt" ./test/wt_test.sh

install: build
	install -d "$(DESTDIR)$(PREFIX)/bin"
	install -m 755 target/release/wt "$(DESTDIR)$(PREFIX)/bin/wt"
