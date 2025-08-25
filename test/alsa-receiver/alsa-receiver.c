#include <alsa/asoundlib.h>
#include <math.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#include "../../include/aes67-vsc-2.h"

// TODO read config from file / return audio format from receiver after parsing SDP
// TODO should SDP be parsed in host or library?
static const unsigned int SAMPLE_RATE = 48000;
static const unsigned int CYCLE_TIME = 192; // this should be half the frames that fit into the link offset
static const unsigned int CHANNELS = 2;
// local testing sender
// static const char SDP[] = "v=0\no=- 10943522194 10943522194 IN IP4 192.168.178.39\ns=AES67-VSC : 2\ni=2 channels: Left, Right\nc=IN IP4 239.69.232.56/32\nt=0 0\na=keywds:AES67-VSC\na=recvonly\nm=audio 5004 RTP/AVP 97\na=rtpmap:97 L24/48000/2\na=ptime:1\na=ts-refclk:ptp=IEEE1588-2008:00-1D-C1-FF-FE-0E-10-C4:0\na=mediaclk:direct=0";
// AVIO bluetooth
static const char SDP[] = "v=0\no=- 10943522194 10943522194 IN IP4 192.168.178.97\ns=AVIO-Bluetooth : 2\ni=2 channels: Left, Right\nc=IN IP4 239.69.232.56/32\nt=0 0\na=keywds:Dante\na=recvonly\nm=audio 5004 RTP/AVP 97\na=rtpmap:97 L24/48000/2\na=ptime:1\na=ts-refclk:ptp=IEEE1588-2008:00-1D-C1-FF-FE-0E-10-C4:0\na=mediaclk:direct=0";

Aes67VscReceiverConfig_t receiver_config = {
    id : "alsa-1",
    sdp : SDP,
    link_offset : 8.0,
    buffer_time : 100.0,
    interface_ip : "192.168.178.39"
};

static volatile int keep_running = 1;
static snd_pcm_t *pcm_handle = NULL;

void int_handler(int dummy)
{
    (void)dummy;
    keep_running = 0;
    if (pcm_handle)
    {
        // Abort any blocking write immediately
        snd_pcm_drop(pcm_handle);
    }
}

int main(int argc, char *argv[])
{
    snd_pcm_hw_params_t *params;
    int dir;
    int rc;

    // Catch Ctrl-C to exit cleanly
    signal(SIGINT, int_handler);
    signal(SIGTERM, int_handler);

    // Open default PCM device
    rc = snd_pcm_open(&pcm_handle, "default", SND_PCM_STREAM_PLAYBACK, 0);
    if (rc < 0)
    {
        fprintf(stderr, "unable to open pcm device: %s\n", snd_strerror(rc));
        return 1;
    }

    // Allocate hardware parameters object
    snd_pcm_hw_params_malloc(&params);
    snd_pcm_hw_params_any(pcm_handle, params);

    // Set parameters
    snd_pcm_hw_params_set_access(pcm_handle, params, SND_PCM_ACCESS_RW_INTERLEAVED);
    snd_pcm_hw_params_set_format(pcm_handle, params, SND_PCM_FORMAT_FLOAT_LE);
    snd_pcm_hw_params_set_channels(pcm_handle, params, CHANNELS);
    unsigned int rate = SAMPLE_RATE;
    snd_pcm_hw_params_set_rate_near(pcm_handle, params, &rate, &dir);

    // Apply hardware parameters
    rc = snd_pcm_hw_params(pcm_handle, params);
    if (rc < 0)
    {
        fprintf(stderr, "unable to set hw parameters: %s\n", snd_strerror(rc));
        return 1;
    }

    snd_pcm_hw_params_free(params);

    // Prepare audio interface
    snd_pcm_prepare(pcm_handle);

    uint64_t buffer_len = CYCLE_TIME * CHANNELS;
    float buffer[buffer_len];
    for (int i = 0; i < buffer_len; i++) {
        buffer[i] = 0.0;
    }

    int32_t maybe_receiver = aes67_vsc_create_receiver("alsa-1", &receiver_config);
    if (maybe_receiver < 0) {
        int err = -maybe_receiver;
        fprintf(stderr, "Error creating receiver: %d\n", err);
        return err;
    }
    uint32_t receiver = (uint32_t) maybe_receiver;

    struct timespec now = {
        tv_sec:0,
        tv_nsec:0,
    };

    clock_gettime(CLOCK_TAI, &now);
    double now_d = (double) now.tv_sec + (double) now.tv_nsec / 1000000000.0;
    double media_time_d = now_d * SAMPLE_RATE;
    uint64_t media_time = (uint64_t) round(media_time_d);

    // Play sine wave indefinitely
    while (keep_running)
    {

        // write data fetched in last cycle
        rc = snd_pcm_writei(pcm_handle, buffer, CYCLE_TIME);

        if (rc == -EPIPE)
        {
            // Underrun
            fprintf(stderr, "underrun occurred\n");
            snd_pcm_prepare(pcm_handle);
        }
        else if (rc < 0)
        {
            fprintf(stderr, "error writing to PCM device: %s\n", snd_strerror(rc));
        }

        // advance media clock
        media_time += CYCLE_TIME;

        // pre-fetch data for next cycle
        uint8_t res = aes67_vsc_receive(receiver, media_time, (size_t) buffer, buffer_len);
        if (res != 0) {        
            for (int i = 0; i < buffer_len; i++) {
                buffer[i] = 0.0;
            }
        }
    }

    snd_pcm_drain(pcm_handle);
    snd_pcm_close(pcm_handle);

    aes67_vsc_destroy_receiver(receiver);

    return 0;
}
