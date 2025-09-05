#include <alsa/asoundlib.h>
#include <math.h>
#include <pthread.h>
#include <regex.h>
#include <sched.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#include "../../include/aes67-vsc-2.h"

static const char MEDIA_REGEX[] = "m=audio ([0-9]+) RTP\\/AVP ([0-9]+)";
static const char RTPMAP_REGEX_PREFIX[] = "a=rtpmap:";
static const char RTPMAP_REGEX_SUFFIX[] = " (L[0-9]+)\\/([0-9]+)\\/([0-9]+)";

// TODO read config from file
static const char RECEIVER_ID[] = "alsa-1";
static const char INTERFACE_IP[] = "192.168.178.39";
static const float LINK_OFFSET = 2.0;
static const unsigned int ALSA_FRAMES_PER_CYCLE = 24;

// AVIO Bluetooth
static char SDP[] = "v=0\no=- 2101 0 IN IP4 192.168.178.124\ns=Anubis_611465_2101\nc=IN IP4 239.1.178.124/15\nt=0 0\na=clock-domain:PTPv2 0\na=ts-refclk:ptp=IEEE1588-2008:2C-CF-67-FF-FE-75-93-93:0\na=mediaclk:direct=0\nm=audio 5004 RTP/AVP 98\nc=IN IP4 239.1.178.124/15\na=rtpmap:98 L24/48000/2\na=source-filter: incl IN IP4 239.1.178.124 192.168.178.124\na=clock-domain:PTPv2 0\na=sync-time:0\na=framecount:6\na=palign:0\na=ptime:0.125\na=ts-refclk:ptp=IEEE1588-2008:2C-CF-67-FF-FE-75-93-93:0\na=mediaclk:direct=0\na=recvonly\na=midi-pre2:50040 0,0;0,1\n";
// XCEL 1201
// static const char SDP[] = "v=0\r\no=- 18311622000 18311622019 IN IP4 192.168.178.114\r\ns=XCEL-1201 : 32\r\ni=2 channels: DANTE TX 01, DANTE TX 02\r\nc=IN IP4 239.69.224.56/32\r\nt=0 0\r\na=keywds:Dante\r\na=recvonly\r\nm=audio 5004 RTP/AVP 97\r\na=rtpmap:97 L24/48000/2\r\na=ptime:1\r\na=ts-refclk:ptp=IEEE1588-2008:2C-CF-67-FF-FE-75-93-93:0\r\na=mediaclk:direct=0\r\n";
// NUC
// static const char SDP[] = "v=0\r\no=- 12043261674 12043261683 IN IP4 192.168.178.190\r\ns=NUC : 2\r\ni=2 channels: Left, Right\r\nc=IN IP4 239.69.143.213/32\r\nt=0 0\r\na=keywds:Dante\r\na=recvonly\r\nm=audio 5004 RTP/AVP 97\r\na=rtpmap:97 L24/48000/2\r\na=ptime:1\r\na=ts-refclk:ptp=IEEE1588-2008:2C-CF-67-FF-FE-75-93-93:0\r\na=mediaclk:direct=0\r\n";

typedef struct audio_format
{
    unsigned int sample_rate;
    char *sample_format;
    unsigned int channels;
} audio_format_t;

typedef struct media
{
    audio_format_t audio_format;
    unsigned int port;
} media_t;

regex_t media_regex, rtpmap_regex;

int parse_sdp(char *sdp, media_t *media)
{

    fprintf(stderr, "Reading SDP file …\n");

    int reti;

    size_t max_media_groups = 3;
    regmatch_t media_match[max_media_groups];
    reti = regcomp(&media_regex, MEDIA_REGEX, REG_EXTENDED);
    if (reti)
    {
        fprintf(stderr, "Could not compile media regex\n");
        return 1;
    }
    reti = regexec(&media_regex, sdp, max_media_groups, media_match, 0);
    if (reti)
    {
        fprintf(stderr, "No match\n");
        regfree(&media_regex);
        return reti;
    }

    int port_group = 1;
    char port_str[strlen(sdp) + 1];
    strcpy(port_str, sdp);
    port_str[media_match[port_group].rm_eo] = 0;
    media->port = atoi(port_str + media_match[port_group].rm_so);

    int payload_type_group = 2;
    char payload_type_str[strlen(sdp) + 1];
    strcpy(payload_type_str, sdp);
    payload_type_str[media_match[payload_type_group].rm_eo] = 0;
    int payload_type_len = media_match[payload_type_group].rm_eo - media_match[payload_type_group].rm_so;

    int rtpmap_regex_len = strlen(RTPMAP_REGEX_PREFIX) + payload_type_len + strlen(RTPMAP_REGEX_SUFFIX);
    char *rtpmap_regex_str = malloc(rtpmap_regex_len + 1);
    memcpy(rtpmap_regex_str, RTPMAP_REGEX_PREFIX, strlen(RTPMAP_REGEX_PREFIX));
    memcpy(rtpmap_regex_str + strlen(RTPMAP_REGEX_PREFIX), payload_type_str + media_match[payload_type_group].rm_so, payload_type_len);
    memcpy(rtpmap_regex_str + strlen(RTPMAP_REGEX_PREFIX) + payload_type_len, RTPMAP_REGEX_SUFFIX, strlen(RTPMAP_REGEX_SUFFIX));
    rtpmap_regex_str[rtpmap_regex_len] = 0;

    regfree(&media_regex);

    size_t max_rtpmap_groups = 4;
    regmatch_t rtpmap_match[max_rtpmap_groups];
    reti = regcomp(&rtpmap_regex, rtpmap_regex_str, REG_EXTENDED);
    if (reti)
    {
        fprintf(stderr, "Could not compile rtpmap regex\n");
        free(rtpmap_regex_str);
        return 1;
    }
    reti = regexec(&rtpmap_regex, sdp, max_rtpmap_groups, rtpmap_match, 0);
    if (reti)
    {
        fprintf(stderr, "No match\n");
        free(rtpmap_regex_str);
        regfree(&rtpmap_regex);
        return reti;
    }

    int sample_format_group = 1;
    char sample_format_str[strlen(sdp) + 1];
    strcpy(sample_format_str, sdp);
    sample_format_str[rtpmap_match[sample_format_group].rm_eo] = 0;
    media->audio_format.sample_format = sample_format_str + rtpmap_match[sample_format_group].rm_so;

    int sample_rate_group = 2;
    char sample_rate_str[strlen(sdp) + 1];
    strcpy(sample_rate_str, sdp);
    sample_rate_str[rtpmap_match[sample_rate_group].rm_eo] = 0;
    media->audio_format.sample_rate = atoi(sample_rate_str + rtpmap_match[sample_rate_group].rm_so);

    int channels_group = 3;
    char channels_str[strlen(sdp) + 1];
    strcpy(channels_str, sdp);
    channels_str[rtpmap_match[channels_group].rm_eo] = 0;
    media->audio_format.channels = atoi(channels_str + rtpmap_match[channels_group].rm_so);

    regfree(&rtpmap_regex);

    free(rtpmap_regex_str);

    fprintf(stderr, "Port: %d\n", media->port);
    fprintf(stderr, "Payload type: %s\n", payload_type_str + media_match[payload_type_group].rm_so);
    fprintf(stderr, "Sample format: %s\n", media->audio_format.sample_format);
    fprintf(stderr, "Sample rate: %d\n", media->audio_format.sample_rate);
    fprintf(stderr, "Channels: %d\n", media->audio_format.channels);

    return 0;
}

Aes67VscReceiverConfig_t receiver_config = {
    name : RECEIVER_ID,
    sdp : SDP,
    link_offset : LINK_OFFSET,
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

uint64_t current_time_media(struct timespec *now, unsigned int srate)
{
    clock_gettime(CLOCK_TAI, now);
    return (uint64_t)(*now).tv_sec * (uint64_t)srate + ((uint64_t)(*now).tv_nsec * srate) / 1000000000;
}

void mute(int *mute)
{
    if (!*mute)
    {
        fprintf(stderr, "mute ON\n");
    }
    *mute = 200;
}

int main(int argc, char *argv[])
{

    media_t media;

    int reti = parse_sdp(SDP, &media);
    if (reti)
    {

        fprintf(stderr, "Could not parse SDP\n");
        exit(1);
    }

    unsigned int channels = media.audio_format.channels;
    unsigned int srate = media.audio_format.sample_rate;

    snd_pcm_hw_params_t *params;
    int dir;
    int rc;

    // Catch Ctrl-C to exit cleanly
    signal(SIGINT, int_handler);
    signal(SIGTERM, int_handler);

    int32_t maybe_receiver = aes67_vsc_create_receiver(&receiver_config);
    if (maybe_receiver < 0)
    {
        int err = -maybe_receiver;
        fprintf(stderr, "Error creating receiver: %d\n", err);
        return err;
    }
    uint32_t receiver = (uint32_t)maybe_receiver;

    // create and zero playout buffer
    uint64_t buffer_len = ALSA_FRAMES_PER_CYCLE * channels;
    float buffer[ALSA_FRAMES_PER_CYCLE * channels];
    slice_mut_float_t buffer_ptr;
    buffer_ptr.ptr = buffer;
    buffer_ptr.len = buffer_len;

    uint64_t link_offset_frames = LINK_OFFSET * srate / 1000;

    // used to keep track of current time
    struct timespec now;

    // warmup, wait for receiver to actually receive data
    while (keep_running && aes67_vsc_receive(receiver, current_time_media(&now, srate) - link_offset_frames, buffer_ptr) == AES_VSC_ERROR_RECEIVER_NOT_READY_YET)
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
    snd_pcm_hw_params_set_channels(pcm_handle, params, media.audio_format.channels);
    unsigned int rate = srate;
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

    // pre-roll for the duration of the link offset, from then on we will just play the latest packets as fast as possible
    for (int i = 0; i < link_offset_frames;)
    {
        int written = snd_pcm_writei(pcm_handle, buffer, ALSA_FRAMES_PER_CYCLE);
        if (written > 0)
        {
            i += written;
        }
    }

    // uint64_t media_time = (current_time_media(&now, srate) / ALSA_FRAMES_PER_CYCLE) * ALSA_FRAMES_PER_CYCLE;
    uint64_t playout_time = current_time_media(&now, srate);

    int muted = 0;

    while (keep_running)
    {

        uint8_t res = aes67_vsc_receive(receiver, playout_time, buffer_ptr);

        if (res == AES_VSC_ERROR_CLOCK_SYNC_ERROR)
        {
            fprintf(stderr, "we are out of sync with the receiver's clock. something is very wrong here\n");
            return 1;
        }

        if (res == AES_VSC_ERROR_NO_DATA)
        {
            // TODO calculate minimum required sleep time
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

        if (rc > 0)
        {
            playout_time += rc;
        }
        else
        {
            playout_time += ALSA_FRAMES_PER_CYCLE;
        }
    }

    snd_pcm_drain(pcm_handle);
    snd_pcm_close(pcm_handle);

    aes67_vsc_destroy_receiver(receiver);

    return 0;
}
