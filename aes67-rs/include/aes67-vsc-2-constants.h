#ifndef __RUST_AES67_RS_CONSTS__
#define __RUST_AES67_RS_CONSTS__

#ifdef __cplusplus
extern "C"
{
#endif

    static const unsigned int AES_VSC_OK = 0x00;
    static const unsigned int AES_VSC_ERROR_NOT_INITIALIZED = 0x01;
    static const unsigned int AES_VSC_ERROR_ALREADY_INITIALIZED = 0x02;
    static const unsigned int AES_VSC_ERROR_UNSUPPORTED_BIT_DEPTH = 0x03;
    static const unsigned int AES_VSC_ERROR_UNSUPPORTED_SAMPLE_RATE = 0x04;
    static const unsigned int AES_VSC_ERROR_VSC_NOT_CREATED = 0x05;
    static const unsigned int AES_VSC_ERROR_RECEIVER_NOT_FOUND = 0x06;
    static const unsigned int AES_VSC_ERROR_SENDER_NOT_FOUND = 0x07;
    static const unsigned int AES_VSC_ERROR_INVALID_CHANNEL = 0x08;
    static const unsigned int AES_VSC_ERROR_RECEIVER_BUFFER_UNDERRUN = 0x09;
    static const unsigned int AES_VSC_ERROR_CLOCK_SYNC_ERROR = 0x0A;
    static const unsigned int AES_VSC_ERROR_RECEIVER_NOT_READY_YET = 0x0B;
    static const unsigned int AES_VSC_ERROR_NO_DATA = 0x0C;

#ifdef __cplusplus
} /* extern \"C\" */
#endif

#endif /* __RUST_AES67_RS_CONSTS__ */
