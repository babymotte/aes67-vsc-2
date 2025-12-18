# AES67 RS VSC Management Agent

An in-process daemon that manages a local AES67 VSC instance and provides a REST API for configuring it. It also provides hooks for linking created senders and receivers to audio drivers like ALSA, JACK or PipeWire.

This is primarily intended to be used in desktop VSC applications. When using `aes67-rs` as a library embedded into another application it is recommended to control it directly over its Rust or C API.

## Responsibilities

 - load application config
 - try to initialize the VSC based on the config
 - start REST API
 - update config and VSC state based on REST API calls
 - forward VSC related API calls (e.g. create/delete sender/receiver) to the vsc's Rust API