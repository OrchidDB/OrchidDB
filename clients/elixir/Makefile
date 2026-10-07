ERL_INCLUDE_DIR ?= $(shell erl -noshell -eval 'io:format("~s", [filename:join([code:root_dir(), "usr", "include"])]), halt().')
UNAME := $(shell uname -s)
ifeq ($(UNAME),Darwin)
LDFLAGS = -dynamiclib -undefined dynamic_lookup
else
LDFLAGS = -shared -ldl
endif
all: priv/orchiddb_nif.so CORE_REVISION
CORE_REVISION:
	python3 ../../scripts/clients.py metadata
priv/orchiddb_nif.so: c_src/orchiddb_nif.c
	mkdir -p priv
	$(CC) -O2 -fPIC -Wall -Wextra -I"$(ERL_INCLUDE_DIR)" $< $(LDFLAGS) -o $@
clean:
	rm -f priv/orchiddb_nif.so
