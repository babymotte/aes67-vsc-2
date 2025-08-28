#include <alsa/asoundlib.h>
#include <math.h>
#include <pthread.h>
#include <sched.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#include "../../include/aes67-vsc-2.h"

// TODO read config from file
static const char RECEIVER_ID[] = "alsa-1";
static const char INTERFACE_IP[] = "192.168.178.36";
static const float LINK_OFFSET = 411.0;
static const unsigned int ALSA_FRAMES_PER_CYCLE = 24;

// AVIO Bluetooth
static const char SDP[] = "v=0\r\no=- 10943522194 10943522219 IN IP4 192.168.178.97\r\ns=AVIO-Bluetooth : 2\r\ni=2 channels: Left, Right\r\nc=IN IP4 239.69.232.56/32\r\nt=0 0\r\na=keywds:Dante\r\na=recvonly\r\nm=audio 5004 RTP/AVP 97\r\na=rtpmap:97 L24/48000/2\r\na=ptime:1\r\na=ts-refclk:ptp=IEEE1588-2008:00-1D-C1-FF-FE-0E-10-C4:0\r\na=mediaclk:direct=0\r\n";
// XCEL 1201
// static const char SDP[] = "v=0\r\no=- 18311622000 18311622019 IN IP4 192.168.178.114\r\ns=XCEL-1201 : 32\r\ni=2 channels: DANTE TX 01, DANTE TX 02\r\nc=IN IP4 239.69.224.56/32\r\nt=0 0\r\na=keywds:Dante\r\na=recvonly\r\nm=audio 5004 RTP/AVP 97\r\na=rtpmap:97 L24/48000/2\r\na=ptime:1\r\na=ts-refclk:ptp=IEEE1588-2008:2C-CF-67-FF-FE-75-93-93:0\r\na=mediaclk:direct=0\r\n";
// NUC
// static const char SDP[] = "v=0\r\no=- 12043261674 12043261683 IN IP4 192.168.178.190\r\ns=NUC : 2\r\ni=2 channels: Left, Right\r\nc=IN IP4 239.69.143.213/32\r\nt=0 0\r\na=keywds:Dante\r\na=recvonly\r\nm=audio 5004 RTP/AVP 97\r\na=rtpmap:97 L24/48000/2\r\na=ptime:1\r\na=ts-refclk:ptp=IEEE1588-2008:2C-CF-67-FF-FE-75-93-93:0\r\na=mediaclk:direct=0\r\n";

// TODO read from SDP
static const unsigned int SAMPLE_RATE = 48000;
static const unsigned int CHANNELS = 2;

Aes67VscReceiverConfig_t receiver_config = {
    id : RECEIVER_ID,
    sdp : SDP,
    link_offset : LINK_OFFSET,
    buffer_time : 20.0 * LINK_OFFSET,
    interface_ip : INTERFACE_IP
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

uint64_t current_time_media(struct timespec *now)
{
    clock_gettime(CLOCK_TAI, now);
    return (uint64_t)(*now).tv_sec * SAMPLE_RATE + (uint64_t)(*now).tv_nsec * SAMPLE_RATE / 1000000000;
}

uint64_t current_time_usec(struct timespec *now)
{
    clock_gettime(CLOCK_TAI, now);
    return (uint64_t)(*now).tv_sec * 1000000 + (uint64_t)(*now).tv_nsec / 1000;
}

void mute(int *mute)
{
    if (!*mute)
    {
        fprintf(stderr, "mute ON\n");
    }
    *mute = 200;
}

int set_thread_prio()
{
    struct sched_param param;
    int policy = SCHED_FIFO; // Or SCHED_RR for round-robin

    // Set the priority (range depends on policy)
    param.sched_priority = 99; // 1..99 for FIFO/RR (99 = highest)

    pthread_t this_thread = pthread_self();
    if (pthread_setschedparam(this_thread, policy, &param) != 0)
    {
        fprintf(stderr, "failed to set thread prio\n");
        perror("pthread_setschedparam");
        return 1;
    }

    fprintf(stderr, "thread prio successfully set\n");

    return 0;
}

int main(int argc, char *argv[])
{

    // set_thread_prio();

    snd_pcm_hw_params_t *params;
    int dir;
    int rc;

    // Catch Ctrl-C to exit cleanly
    signal(SIGINT, int_handler);
    signal(SIGTERM, int_handler);

    int32_t maybe_receiver = aes67_vsc_create_receiver("alsa-1", &receiver_config);
    if (maybe_receiver < 0)
    {
        int err = -maybe_receiver;
        fprintf(stderr, "Error creating receiver: %d\n", err);
        return err;
    }
    uint32_t receiver = (uint32_t)maybe_receiver;

    // create and zero playout buffer
    uint64_t buffer_len = ALSA_FRAMES_PER_CYCLE * CHANNELS;
    float buffer[buffer_len];

    uint64_t link_offset_frames = LINK_OFFSET * SAMPLE_RATE / 1000;

    // used to keep track of current time
    struct timespec now;

    // warmup, wait for receiver to actually receive data
    while (keep_running && aes67_vsc_receive(receiver, current_time_media(&now) - link_offset_frames, (size_t)buffer, buffer_len) == AES_VSC_ERROR_RECEIVER_NOT_READY_YET)
    {
        usleep(100000);
    }

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

    // start playout

    for (int i = 0; i < buffer_len; i++)
    {
        buffer[i] = 0.0;
    }

    uint64_t media_time = (current_time_media(&now) / ALSA_FRAMES_PER_CYCLE) * ALSA_FRAMES_PER_CYCLE;

    int muted = 0;

    while (keep_running)
    {
        uint64_t playout_time = media_time - link_offset_frames;

        uint8_t res = aes67_vsc_receive(receiver, playout_time, (size_t)buffer, buffer_len);

        if (res == AES_VSC_ERROR_CLOCK_SYNC_ERROR)
        {
            fprintf(stderr, "we are out of sync with the receiver's clock. something is very wrong here\n");
            return 1;
        }

        if (res == AES_VSC_ERROR_NO_DATA)
        {
            // we have freewheeled too far ahead, let's wait and try again
            usleep(1);
            // skip writing to playout buffer and incrementing cycle
            continue;
        }

        if (muted)
        {
            muted--;
            for (int i = 0; i < buffer_len; i++)
            {
                buffer[i] = 0.0;
            }
            if (!muted)
            {
                fprintf(stderr, "mute OFF\n");
            }
        }

        // write audio data to alsa buffer
        rc = snd_pcm_writei(pcm_handle, buffer, ALSA_FRAMES_PER_CYCLE);
        if (rc == -EPIPE)
        {
            // Underrun
            if (!muted)
            {
                fprintf(stderr, "underrun occurred\n");
            }
            mute(&muted);
            snd_pcm_prepare(pcm_handle);
        }
        else if (rc < 0)
        {
            fprintf(stderr, "error writing to PCM device: %s\n", snd_strerror(rc));
        }
        else
        {
            media_time += rc;
        }
    }

    snd_pcm_drain(pcm_handle);
    snd_pcm_close(pcm_handle);

    aes67_vsc_destroy_receiver(receiver);

    return 0;
}
