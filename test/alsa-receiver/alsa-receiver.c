#include <alsa/asoundlib.h>
#include <math.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include "../../include/aes67-vsc-2.h"

#define SAMPLE_RATE 48000
#define FREQUENCY 440.0
#define AMPLITUDE 0.5
#define BUFFER_SIZE 24
#define CHANNELS 2
#define BIT_DEPTH 24
#define SDP "v=0\no=- 10943522194 10943522194 IN IP4 192.168.178.39\ns=AES67-VSC : 2\ni=2 channels: Left, Right\nc=IN IP4 239.69.232.56/32\nt=0 0\na=keywds:AES67-VSC\na=recvonly\nm=audio 5004 RTP/AVP 97\na=rtpmap:97 L24/48000/2\na=ptime:1\na=ts-refclk:ptp=IEEE1588-2008:00-1D-C1-FF-FE-0E-10-C4:0\na=mediaclk:direct=0"

Aes67VscReceiverConfig_t receiver_config = {
    id : "alsa-1",
    sdp : SDP,
    link_offset : 1.0,
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
    snd_pcm_hw_params_set_format(pcm_handle, params, SND_PCM_FORMAT_S16_LE);
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

    short buffer[BUFFER_SIZE * CHANNELS]; // stereo: left+right
    double phase = 0.0;
    double phase_inc = 2.0 * M_PI * FREQUENCY / SAMPLE_RATE;

    // TODO handle errors
    aes67_vsc_init();
    int32_t vsc = aes67_vsc_create_vsc();
    aes67_vsc_create_receiver(&vsc, "alsa-1", &receiver_config);

    // Play sine wave indefinitely
    while (keep_running)
    {
        for (int j = 0; j < BUFFER_SIZE; j++)
        {
            short sample = (short)(AMPLITUDE * 32767.0 * sin(phase));
            for (int ch = 0; ch < CHANNELS; ch++)
            {
                buffer[CHANNELS * j + ch] = sample;
            }
            phase += phase_inc;
            if (phase >= 2.0 * M_PI)
                phase -= 2.0 * M_PI;
        }

        rc = snd_pcm_writei(pcm_handle, buffer, BUFFER_SIZE);
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
    }

    snd_pcm_drain(pcm_handle);
    snd_pcm_close(pcm_handle);

    // this is not really necessary since destroying the VSC will also destroy all its receivers
    aes67_vsc_destroy_receiver(&vsc, "alsa-1");
    aes67_vsc_destroy_vsc(&vsc);

    return 0;
}
