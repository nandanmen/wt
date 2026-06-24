.PHONY: test install

PREFIX ?= /usr/local

test:
	./test/wt_test.sh

install:
	install -d "$(DESTDIR)$(PREFIX)/bin"
	install -m 755 wt "$(DESTDIR)$(PREFIX)/bin/wt"
