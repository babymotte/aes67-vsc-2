# AES67-RS

A Rust library for sending and receiving audio over IP in compliance with the AES67 standard. The AES67-RS crate itself does not implement any playback or recording functionality, it is merely responsible for taking audio data from a buffer and sending it out as an AES67 stream or for receiving an AES67 stream and writing it to a buffer.

It does however provide interfaces that can be used to build integrations with audio systems like Alsa, PipeWire or JACK.

## API

The library exposes multiple APIs:

### Rust

AES67-RS can be used as regular Rust library. It is structured into two layers:

#### Direct I/O

The lower level layer allows directly instantiating and configuring AES67 senders or receivers. This can be useful when building applications that have a well known number of inputs or outputs, like a media player that streams a stereo or 5.1 stream to a configurable receiver.

#### Virtual Sound Card

The virtual sound card layer provides an interface for dynamically creating, configuring and destroying senders and/or receivers on demand. It also exposes a REST API.

### C

The AES67-RS compiles to a dynamically linked library and an according C header file that can be loaded and used from any C application. The API it provides is equivalent to the Rust Virtual Sound Card API.
