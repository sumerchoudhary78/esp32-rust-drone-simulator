/* Aerolab functional board adapter for Flexe. MIT; see THIRD_PARTY_NOTICES.
 * No network or host filesystem devices are exposed by this adapter.
 * stdin: dt_us ax ay az gx gy gz adc powered uart_hex (one bounded line)
 * stdout: one JSON reply per step. Library diagnostics go to stderr.
 */
#define _POSIX_C_SOURCE 200809L
#include "flexe_session.h"
#include "target.h"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

volatile int emu_app_running = 1;
typedef struct {
    flexe_session_t *session;
    mmio_handler_t efuse, syscon;
    uint8_t regs[128], cursor;
    int sda, scl, address, adc_channel, powered;
    float accel[3], gyro[3];
    float pwm[40];
    struct {int gpio;float pulse;} sources[16];
    uint64_t transfers, samples;
    unsigned char uart[2048];
    size_t uart_len;
} board_t;

/* WROOM-32E is an ECO3 board. Preserve all other modeled eFuse / clock
 * registers. These identity bits are read by esp-hal's hardware revision
 * check; the firmware itself keeps its real revision requirement. */
static uint32_t efuse_read(void *ctx, uint32_t addr) {
    board_t *b = ctx;
    uint32_t value = b->efuse.read ? b->efuse.read(b->efuse.ctx, addr) : 0;
    if (addr == 0x3ff5a00c) value |= 1u << 15;
    if (addr == 0x3ff5a014) value |= 1u << 20;
    return value;
}
static uint32_t syscon_read(void *ctx, uint32_t addr) {
    board_t *b = ctx;
    uint32_t value = b->syscon.read ? b->syscon.read(b->syscon.ctx, addr) : 0;
    return addr == 0x3ff6607c ? value | 0x80000000u : value;
}
static void forward_write(mmio_handler_t *h, uint32_t addr, uint32_t val) {
    if (h->write) h->write(h->ctx, addr, val);
    else if (h->masked_write) h->masked_write(h->ctx, addr, val, UINT32_MAX);
}
static void efuse_write(void *ctx, uint32_t a, uint32_t v) { forward_write(&((board_t *)ctx)->efuse, a, v); }
static void syscon_write(void *ctx, uint32_t a, uint32_t v) { forward_write(&((board_t *)ctx)->syscon, a, v); }
static void uart(void *ctx, uint8_t byte) {
    board_t *b = ctx;
    if (b->uart_len < sizeof(b->uart)) b->uart[b->uart_len++] = byte;
}
static void rom_uart(xtensa_cpu_t *cpu, void *ctx) {
    unsigned ci = XT_PS_CALLINC(cpu->ps);
    uart(ctx, (uint8_t)ar_read(cpu, ci * 4 + 2));
    uint32_t ret = ar_read(cpu, ci * 4);
    ar_write(cpu, ci * 4 + 2, 0);
    cpu->pc = 0x40000000u | (ret & 0x3fffffffu);
    XT_PS_SET_CALLINC(cpu->ps, 0);
}
static void mpu_reset(board_t *b) {
    memset(b->regs, 0, sizeof(b->regs));
    b->regs[0x75] = 0x68; b->regs[0x6b] = 0x40; b->cursor = 0;
}
static void put16(uint8_t *out, float value) {
    int16_t n = (int16_t)fmaxf(-32768, fminf(32767, value));
    out[0] = (uint16_t)n >> 8; out[1] = n & 255;
}
static void sample(board_t *b) {
    if (b->regs[0x6b] & 0x40) return;
    float ascale = 16384.0f / (1u << ((b->regs[0x1c] >> 3) & 3));
    const float gyro_scales[] = {131.0f,65.5f,32.8f,16.4f};
    float gscale = gyro_scales[(b->regs[0x1b] >> 3) & 3];
    for (int i = 0; i < 3; i++) {
        put16(&b->regs[0x3b + i * 2], b->accel[i] / 9.80665f * ascale);
        put16(&b->regs[0x43 + i * 2], b->gyro[i] * 57.2957795f * gscale);
    }
    put16(&b->regs[0x41], (32.0f - 36.53f) * 340.0f);
    b->regs[0x3a] = 1;
}
static int mpu(void *ctx, int port, uint8_t addr, const uint8_t *wr,
               size_t wn, uint8_t *rd, size_t rn) {
    board_t *b = ctx;
    esp32_periph_t *p = flexe_session_periph(b->session);
    int scl_signal = port == 0 ? 29 : 95, sda_signal = port == 0 ? 30 : 96;
    if (!b->powered || addr != b->address || b->sda < 0 || b->scl < 0 ||
        periph_gpio_out_signal(p, b->sda) != sda_signal ||
        periph_gpio_out_signal(p, b->scl) != scl_signal) return -1;
    if (rn || wn>1) b->transfers++;
    if (wn) {
        b->cursor = wr[0] & 127;
        for (size_t i = 1; i < wn; i++) {
            uint8_t reg = b->cursor++ & 127;
            if (reg == 0x6b && (wr[i] & 0x80)) mpu_reset(b);
            else if (reg != 0x75 && !(reg >= 0x3b && reg <= 0x48)) b->regs[reg] = wr[i];
        }
    }
    sample(b);
    if (rn && b->cursor == 0x3b) b->samples++;
    for (size_t i = 0; i < rn; i++) { uint8_t reg = b->cursor++ & 127; rd[i] = b->regs[reg]; if (reg == 0x3a) b->regs[reg] = 0; }
    return 0;
}
static void ledc(void *ctx, int mode, int channel, int gpio, uint32_t hz,
                uint32_t duty, uint32_t max, bool enabled, bool inverted) {
    board_t *b = ctx;
    float ratio = max ? (float)duty / (max + 1.0f) : 0;
    if (inverted) ratio = 1 - ratio;
    unsigned source=mode*8+channel; b->sources[source].gpio=gpio;
    b->sources[source].pulse = enabled && hz >= 40 && hz <= 500 ? 1000000.0f / hz * ratio : 0;
}
static void attach(board_t *b) {
    xtensa_mem_t *m = flexe_session_mem(b->session);
    /* The image loader enters the application directly. Model the ESP-IDF
     * bootloader handoff with TIMG0 clock enabled (esp-hal uses its LACT). */
    mem_write32(m, 0x3ff000c0, mem_read32(m, 0x3ff000c0) | (1u << 13));
    b->efuse = m->mmio[(0x3ff5a000 - m->target->peripheral_start) / 4096];
    b->syscon = m->mmio[(0x3ff66000 - m->target->peripheral_start) / 4096];
    mem_register_mmio_range(m, 0x3ff5a000, 4096, efuse_read, efuse_write, b);
    mem_register_mmio_range(m, 0x3ff66000, 4096, syscon_read, syscon_write, b);
    rom_stubs_register_ctx(flexe_session_rom(b->session), 0x40009200, rom_uart, "uart_tx_one_char", b);
    esp32_periph_t *p = flexe_session_periph(b->session);
    for (int channel=0;channel<40;channel++) periph_set_adc_value(p,channel,0);
    for (int port = 0; port < 2; port++) periph_i2c_attach_device(p, port, b->address, mpu, b);
    for (int mode = 0; mode < 2; mode++) for (int ch = 0; ch < 8; ch++) periph_set_ledc_output_callback(p, mode, ch, ledc, b);
}
static int hex_digit(char c) { return c >= '0' && c <= '9' ? c-'0' : c >= 'a' && c <= 'f' ? c-'a'+10 : -1; }
int main(int argc, char **argv) {
    if (argc != 7) { fprintf(stderr, "Usage: aerolab-esp32 image.bin sda scl address adc_channel imu_powered\n"); return 2; }
    FILE *protocol = fdopen(dup(STDOUT_FILENO), "w");
    if (!protocol || dup2(STDERR_FILENO, STDOUT_FILENO) < 0) return 2;
    board_t b = { .sda=atoi(argv[2]), .scl=atoi(argv[3]), .address=atoi(argv[4]), .adc_channel=atoi(argv[5]), .powered=atoi(argv[6]) };
    if (b.sda < -1 || b.sda > 39 || b.scl < -1 || b.scl > 39 || b.address < 0x68 || b.address > 0x69 || b.adc_channel < -1 || b.adc_channel > 7) return 2;
    mpu_reset(&b);
    for(unsigned i=0;i<16;i++) b.sources[i].gpio=-1;
    flexe_session_config_t config = { .bin_path=argv[1], .elf_path=NULL,
        .native_freertos=1, .single_core=1, .uart_cb=uart, .uart_ctx=&b, .unhandled_audit=1 };
    b.session = flexe_session_create(&config);
    if (!b.session) return 3;
    attach(&b);
    unsigned resets = flexe_session_reset_count(b.session);
    char line[2048];
    while (fgets(line, sizeof(line), stdin)) {
        unsigned dt, adc, powered; char input[513];
        if (!strchr(line,'\n') || sscanf(line, "%u %f %f %f %f %f %f %u %u %512s", &dt,
            &b.accel[0], &b.accel[1], &b.accel[2], &b.gyro[0], &b.gyro[1], &b.gyro[2], &adc, &powered, input) != 10 || dt > 20000 || adc > 4095 || powered > 1) break;
        bool finite = true; for (int i=0;i<3;i++) if (!isfinite(b.accel[i]) || !isfinite(b.gyro[i])) finite=false;
        if (!finite) break;
        if (!b.powered && powered) mpu_reset(&b);
        b.powered = powered;
        if (b.adc_channel >= 0) periph_set_adc_value(flexe_session_periph(b.session), b.adc_channel, adc);
        if (strcmp(input,"-")) {
            size_t len = strlen(input); uint8_t bytes[256]; bool valid = len%2==0;
            for (size_t i=0; i<len/2; i++) {int a=hex_digit(input[i*2]),c=hex_digit(input[i*2+1]);if(a<0||c<0){valid=false;break;}bytes[i]=(a<<4)|c;}
            if (!valid) break;
            periph_uart_rx_inject_num(flexe_session_periph(b.session), 0, bytes, len/2);
        }
        xtensa_cpu_t *cpu = flexe_session_cpu(b.session,0);
        uint64_t target = xtensa_guest_time_us(cpu, xtensa_cpu_freq_mhz(cpu)) + dt;
        while (!cpu->halted && xtensa_guest_time_us(cpu, xtensa_cpu_freq_mhz(cpu)) < target) {
            flexe_session_run_core(b.session,0,1000); flexe_session_post_batch(b.session,1000);
            if (flexe_session_reset_count(b.session) != resets) { memset(b.pwm,0,sizeof(b.pwm)); attach(&b); resets=flexe_session_reset_count(b.session); break; }
        }
        memset(b.pwm,0,sizeof(b.pwm));
        for(unsigned i=0;i<16;i++) {int pin=b.sources[i].gpio;if(pin>=0 && pin<40 && periph_gpio_output_enabled(flexe_session_periph(b.session),pin)) b.pwm[pin]=b.sources[i].pulse;}
        fprintf(protocol,"{\"cycles\":%llu,\"guest_us\":%llu,\"i2c_transactions\":%llu,\"imu_reads\":%llu,\"halted\":%s,\"unmapped\":%llu,\"resets\":%u,\"pwm_us\":[",
            (unsigned long long)cpu->cycle_count, (unsigned long long)xtensa_guest_time_us(cpu,xtensa_cpu_freq_mhz(cpu)),
            (unsigned long long)b.transfers,(unsigned long long)b.samples,cpu->halted?"true":"false",(unsigned long long)mem_unmapped_count(flexe_session_mem(b.session)),resets);
        for (int i=0;i<40;i++) fprintf(protocol,"%s%.3f",i?",":"",b.pwm[i]);
        bool reserved_low=true;for(unsigned i=0;i<4;i++){int pin=(int[]){25,26,27,14}[i];if(!periph_gpio_output_enabled(flexe_session_periph(b.session),pin)||periph_gpio_pin_level(flexe_session_periph(b.session),pin)) reserved_low=false;}
        fprintf(protocol,"],\"reserved_gpio_outputs_low\":%s,\"unhandled\":%d,\"uart_hex\":\"",reserved_low?"true":"false",periph_unhandled_count(flexe_session_periph(b.session))); for(size_t i=0;i<b.uart_len;i++) fprintf(protocol,"%02x",b.uart[i]);
        fprintf(protocol,"\"}\n");fflush(protocol);b.uart_len=0;
    }
    flexe_session_destroy(b.session);fclose(protocol);return 0;
}
