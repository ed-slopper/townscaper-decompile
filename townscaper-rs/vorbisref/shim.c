/* ts_vorbisref: the identification and setup headers libvorbis writes for (channels, rate, quality). */
#include <string.h>
#include <vorbis/vorbisenc.h>

static long copy_packet(const ogg_packet *p, unsigned char *out, long cap) {
    if (p->bytes > cap) return -1;
    memcpy(out, p->packet, p->bytes);
    return p->bytes;
}

/* Writes the identification header to ident and the setup header to setup; returns 0 on success. */
int ts_vorbis_headers(int channels, long rate, float quality,
                      unsigned char *ident, long ident_cap, long *ident_len,
                      unsigned char *setup, long setup_cap, long *setup_len) {
    vorbis_info vi;
    vorbis_comment vc;
    vorbis_dsp_state vd;
    ogg_packet h0, h1, h2;
    int ok = -1;
    vorbis_info_init(&vi);
    if (vorbis_encode_init_vbr(&vi, channels, rate, quality) == 0) {
        vorbis_comment_init(&vc);
        vorbis_analysis_init(&vd, &vi);
        if (vorbis_analysis_headerout(&vd, &vc, &h0, &h1, &h2) == 0) {
            *ident_len = copy_packet(&h0, ident, ident_cap);
            *setup_len = copy_packet(&h2, setup, setup_cap);
            ok = (*ident_len < 0 || *setup_len < 0) ? -1 : 0;
        }
        vorbis_dsp_clear(&vd);
        vorbis_comment_clear(&vc);
    }
    vorbis_info_clear(&vi);
    return ok;
}
