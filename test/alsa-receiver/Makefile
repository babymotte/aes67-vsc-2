CC      = gcc
CFLAGS  = -Wall -O2
LDFLAGS = -lasound -lm -L../../target/debug -laes67_vsc_2

TARGET  = alsa-receiver
SRC     = alsa-receiver.c

all: $(TARGET)

$(TARGET): $(SRC)
	$(CC) $(CFLAGS) -o $@ $^ $(LDFLAGS)

clean:
	rm -f $(TARGET)

run:
	LD_LIBRARY_PATH="../../target/debug" ./$(TARGET)
