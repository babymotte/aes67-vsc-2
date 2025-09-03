CC      = gcc
CFLAGS  = -Wall
LDFLAGS = -lasound -lm -L../../target/debug -laes67_vsc_2

ifeq ($(BUILD),debug)   
# "Debug" build - no optimization, and debugging symbols
CFLAGS += -O0 -g
else
# "Release" build - optimization, and no debug symbols
CFLAGS += -O2 -s -DNDEBUG
endif

TARGET  = alsa-receiver
SRC     = alsa-receiver.c

all: $(TARGET)

$(TARGET): $(SRC)
	$(CC) $(CFLAGS) -o $@ $^ $(LDFLAGS)

clean:
	rm -f $(TARGET)

debug:
	LD_LIBRARY_PATH="../../target/debug" ./$(TARGET)

run:
	LD_LIBRARY_PATH="../../target/release" ./$(TARGET)

cap: 
	sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ./$(TARGET)
	sudo setcap 'cap_net_bind_service+eip cap_sys_nice+eip' ../../target/release/libaes67_vsc_2.so