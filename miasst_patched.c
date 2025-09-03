// miasst_patched.c
// Patched MiAssistantTool with robust libusb interface detection, better I/O, safer packet parsing.
// VERSION 1.3 (patched)

#define VERSION "1.3"
#define REPOSITORY "https://github.com/offici5l/MiAssistantTool"

#ifdef _WIN32
  #include <io.h>
  #define PATH_SEP "\\"
#else
  #include <unistd.h>
  #define PATH_SEP "/"
#endif

#include <libusb-1.0/libusb.h>

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <openssl/evp.h>
#include <curl/curl.h>

#include "tiny-json/tiny-json.h"

// ---- ADB constants ----
#define ADB_CLASS 0xff
#define ADB_SUB_CLASS 0x42
#define ADB_PROTOCOL_CODE 1

#define ADB_CONNECT 0x4E584E43  // 'CNXN'
#define ADB_OPEN    0x4E45504F  // 'OPEN'
#define ADB_OKAY    0x59414B4F  // 'OKAY'
#define ADB_WRTE    0x45545257  // 'WRTE'
#define ADB_CLSE    0x45534C43  // 'CLSE'

#define ADB_MAX_DATA            (1024 * 1024)
#define ADB_SIDELOAD_CHUNK_SIZE (1024 * 64)

typedef struct {
    uint32_t cmd;
    uint32_t arg0;
    uint32_t arg1;
    uint32_t len;
    uint32_t checksum;
    uint32_t magic;
} adb_usb_packet;

// Helper function to get readable ADB command names
static const char* adb_cmd_name(uint32_t c) {
    switch (c) {
        case 0x4E584E43: return "CNXN";
        case 0x4E45504F: return "OPEN";
        case 0x59414B4F: return "OKAY";
        case 0x45545257: return "WRTE";
        case 0x45534C43: return "CLSE";
        default: return "????";
    }
}

// Add to the top of the source (miasst_patched.c), after includes:
static void *memrchr_portable(const void *s, int c, size_t n) {
    const unsigned char *p = (const unsigned char *)s + n;
    while (p != (const unsigned char *)s) {
        if (*(--p) == (unsigned char)c) {
            return (void *)p;
        }
    }
    return NULL;
}

// ---- globals (copied from original) ----
char device[80], version[80], sn[80], codebase[80], branch[80], language[80], region[80], romzone[80];
int bulk_in = -1, bulk_out = -1, interface_num = -1;
libusb_context *ctx = NULL;
libusb_device_handle *dev_handle = NULL;

static char response[4096]; // adb_cmd reply buffer

// ---- USB I/O ----
static int usb_read(void *data, int datalen) {
    int read_len = 0;
    int r = libusb_bulk_transfer(dev_handle, bulk_in, (unsigned char*)data, datalen, &read_len, 5000);
    return (r == LIBUSB_SUCCESS) ? read_len : -1;
}

static int usb_write(const void *data, int datalen) {
    int write_len = 0;
    int r = libusb_bulk_transfer(dev_handle, bulk_out, (unsigned char*)data, datalen, &write_len, 5000);
    return (r == LIBUSB_SUCCESS) ? write_len : -1;
}

static int send_command(uint32_t cmd, uint32_t arg0, uint32_t arg1, const void *data, int datalen) {
    adb_usb_packet pkt = {0};
    pkt.cmd = cmd;
    pkt.arg0 = arg0;
    pkt.arg1 = arg1;
    pkt.len = (uint32_t)datalen;
    pkt.checksum = 0;
    pkt.magic = cmd ^ 0xffffffff;

    if (usb_write(&pkt, sizeof(pkt)) < 0) return 1;
    if (datalen > 0 && data) {
        if (usb_write(data, datalen) < 0) return 1;
    }
    return 0;
}

// Safer: also specify max_data_len, and return the actual data size
static int recv_packet(adb_usb_packet *pkt, void *data, int max_data_len, int *out_data_len) {
    int r = usb_read(pkt, sizeof(adb_usb_packet));
    if (r != sizeof(adb_usb_packet)) return 1;

    int want = (int)pkt->len;
    if (want < 0) return 1;

    if (want > 0) {
        if (data == NULL || max_data_len <= 0) return 1; // no place to write
        int toread = want;
        if (toread > max_data_len) toread = max_data_len; // truncate, but don't write over
        int got = usb_read(data, toread);
        if (got != toread) return 1;

        // if the device promised more data than we request, discard the rest
        int remaining = want - toread;
        char dump[512];
        while (remaining > 0) {
            int chunk = remaining > (int)sizeof(dump) ? (int)sizeof(dump) : remaining;
            int g = usb_read(dump, chunk);
            if (g != chunk) return 1;
            remaining -= g;
        }
        if (out_data_len) *out_data_len = want;
    } else {
        if (out_data_len) *out_data_len = 0;
    }
    return 0;
}

// Simplified ADB command - OPEN → WRTE → (text) → CLSE
static char* adb_cmd(const char *command) {
    int cmd_len = (int)strlen(command);
    if (send_command(ADB_OPEN, 1, 0, command, cmd_len) != 0) {
        fprintf(stderr, "device did not accept OPEN\n");
        return NULL;
    }

    adb_usb_packet pkt;
    char buf[1024];
    int data_len = 0;

    // wait for WRTE with response
    if (recv_packet(&pkt, buf, sizeof(buf)-1, &data_len) != 0) {
        fprintf(stderr, "Failed to read response (WRTE)\n");
        return NULL;
    }
    if (pkt.cmd != ADB_WRTE) {
        // sometimes OKAY comes first - handle it
        if (pkt.cmd == ADB_OKAY) {
            if (recv_packet(&pkt, buf, sizeof(buf)-1, &data_len) != 0 || pkt.cmd != ADB_WRTE) {
                fprintf(stderr, "Unexpected packet sequence\n");
                return NULL;
            }
        } else {
            fprintf(stderr, "Unexpected ADB cmd: 0x%08x\n", pkt.cmd);
            return NULL;
        }
    }

    buf[data_len] = 0;
    strncpy(response, buf, sizeof(response)-1);
    response[sizeof(response)-1] = 0;

    // send OKAY back
    send_command(ADB_OKAY, pkt.arg1, pkt.arg0, NULL, 0);

    // wait for CLSE (discard its content)
    recv_packet(&pkt, buf, sizeof(buf), &data_len);

    // trim newline
    size_t n = strlen(response);
    if (n && response[n-1] == '\n') response[n-1] = 0;

    return response;
}

// ---- MD5 and validate_check: unchanged logic, minimal cleanup ----
static void calculate_md5(char *filePath, char *md5) {
    FILE *file;

    while (1) {
        printf("Enter .zip file path: ");
        if (fgets(filePath, 256, stdin)) {
            filePath[strcspn(filePath, "\n")] = '\0';
            if (strstr(filePath, ".zip") && (file = fopen(filePath, "rb"))) {
                fclose(file);
                break;
            }
        }
        printf("Invalid file, try again.\n");
    }

    file = fopen(filePath, "rb");
    if (!file) { perror("open zip"); exit(1); }
    EVP_MD_CTX *mdctx = EVP_MD_CTX_new();
    EVP_DigestInit_ex(mdctx, EVP_md5(), NULL);
    unsigned char data[8192], md5hash[EVP_MAX_MD_SIZE];
    size_t bytesRead;
    unsigned int md5len;

    while ((bytesRead = fread(data, 1, sizeof(data), file)) > 0)
        EVP_DigestUpdate(mdctx, data, bytesRead);

    EVP_DigestFinal_ex(mdctx, md5hash, &md5len);
    fclose(file);
    EVP_MD_CTX_free(mdctx);

    for (unsigned int i = 0; i < md5len; i++)
        sprintf(&md5[i * 2], "%02x", md5hash[i]);

    md5[md5len * 2] = '\0';
}

// curl write callback -> write to file
static size_t fwrite_cb(void *ptr, size_t size, size_t nmemb, void *stream) {
    return fwrite(ptr, size, nmemb, (FILE*)stream);
}

static const char *validate_check(const char *md5, int flash) {
    static char validate_out[256]; // returnable pointer (stable buffer)

    const unsigned char key[16] = { 0x6D,0x69,0x75,0x69,0x6F,0x74,0x61,0x76,0x61,0x6C,0x69,0x64,0x65,0x64,0x31,0x31 };
    const unsigned char iv [16] = { 0x30,0x31,0x30,0x32,0x30,0x33,0x30,0x34,0x30,0x35,0x30,0x36,0x30,0x37,0x30,0x38 };

    char json_request[1024];
    snprintf(json_request, sizeof(json_request),
        "{\"d\":\"%s\",\"v\":\"%s\",\"c\":\"%s\",\"b\":\"%s\",\"sn\":\"%s\",\"l\":\"en-US\",\"f\":\"1\",\"options\":{\"zone\":%s},\"pkg\":\"%s\"}",
        device, version, codebase, branch, sn, romzone, md5 ? md5 : "");

    // PKCS#7 padding
    int len = (int)strlen(json_request);
    int pad = 16 - (len % 16);
    if (pad == 16) pad = 16;
    for (int i=0; i<pad; i++) json_request[len+i] = (char)pad;
    len += pad;

    unsigned char enc[2048]; int enc_len=0, tmp=0;
    EVP_CIPHER_CTX *e = EVP_CIPHER_CTX_new();
    if (!e) return NULL;

    if (EVP_EncryptInit_ex(e, EVP_aes_128_cbc(), NULL, key, iv) != 1) { EVP_CIPHER_CTX_free(e); return NULL; }
    if (EVP_EncryptUpdate(e, enc, &enc_len, (unsigned char*)json_request, len) != 1) { EVP_CIPHER_CTX_free(e); return NULL; }
    if (EVP_EncryptFinal_ex(e, enc+enc_len, &tmp) != 1) { EVP_CIPHER_CTX_free(e); return NULL; }
    enc_len += tmp;
    EVP_CIPHER_CTX_free(e);

    char b64[4096];
    EVP_EncodeBlock((unsigned char*)b64, enc, enc_len);

    CURL *curl = curl_easy_init();
    if (!curl) return NULL;

    char *q = curl_easy_escape(curl, b64, 0);
    if (!q) { curl_easy_cleanup(curl); return NULL; }

    char post[8192];
    snprintf(post, sizeof(post), "q=%s&t=&s=1", q);
    curl_free(q);

    FILE *rf = fopen("response.tmp", "wb");
    if (!rf) { curl_easy_cleanup(curl); return NULL; }

    curl_easy_setopt(curl, CURLOPT_URL, "http://update.miui.com/updates/miotaV3.php");
    curl_easy_setopt(curl, CURLOPT_USERAGENT, "MiTunes_UserAgent_v3.0");
    curl_easy_setopt(curl, CURLOPT_POST, 1L);
    curl_easy_setopt(curl, CURLOPT_POSTFIELDS, post);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, fwrite_cb);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, rf);
    CURLcode cr = curl_easy_perform(curl);
    fclose(rf);
    curl_easy_cleanup(curl);
    if (cr != CURLE_OK) return NULL;

    // read it
    rf = fopen("response.tmp", "rb");
    if (!rf) return NULL;
    fseek(rf, 0, SEEK_END);
    long rs = ftell(rf);
    fseek(rf, 0, SEEK_SET);
    char *resp = (char*)malloc(rs+1);
    if (!resp) { fclose(rf); return NULL; }
    fread(resp,1,rs,rf);
    fclose(rf);
    remove("response.tmp");
    resp[rs]=0;

    // base64 decode → decrypt
    unsigned char *decbuf = (unsigned char*)malloc(rs);
    if (!decbuf) { free(resp); return NULL; }
    int dec_b64 = EVP_DecodeBlock(decbuf, (unsigned char*)resp, (int)strlen(resp));
    free(resp);
    if (dec_b64 <= 0) { free(decbuf); return NULL; }

    EVP_CIPHER_CTX *d = EVP_CIPHER_CTX_new();
    if (!d) { free(decbuf); return NULL; }
    unsigned char *plain = (unsigned char*)malloc(dec_b64 + 32);
    int plen=0; tmp=0;

    EVP_DecryptInit_ex(d, EVP_aes_128_cbc(), NULL, key, iv);
    EVP_DecryptUpdate(d, plain, &plen, decbuf, dec_b64);
    if (EVP_DecryptFinal_ex(d, plain+plen, &tmp) != 1) { EVP_CIPHER_CTX_free(d); free(decbuf); free(plain); return NULL; }
    plen += tmp;
    EVP_CIPHER_CTX_free(d);
    free(decbuf);

    // JSON extraction
    char *start = memchr(plain, '{', plen);
	char *end   = (char*)memrchr_portable(plain, '}', plen);
    if (!start || !end || end < start) { free(plain); return NULL; }

    size_t jlen = (size_t)(end - start + 1);
    char *json_text = (char*)malloc(jlen+1);
    memcpy(json_text, start, jlen); json_text[jlen]=0;

    json_t pool[10000];
    json_t const *root = json_create(json_text, pool, 10000);
    if (!root) { free(json_text); free(plain); return NULL; }

    if (flash == 1) {
        json_t const *pkg_rom = json_getProperty(root, "PkgRom");
        if (pkg_rom) {
            int Erase = atoi(json_getValue(json_getProperty(pkg_rom, "Erase")));
            if (Erase == 1) {
                printf("NOTICE: Data will be erased during flashing.\nPress Enter to continue...");
                getchar();
            }
            const char *val = json_getValue(json_getProperty(pkg_rom, "Validate"));
            if (val) {
                strncpy(validate_out, val, sizeof(validate_out)-1);
                validate_out[sizeof(validate_out)-1] = 0;
                free(json_text); free(plain);
                return validate_out;
            }
        } else {
            json_t const *code = json_getProperty(root, "Code");
            json_t const *message = code ? json_getProperty(code, "message") : NULL;
            if (message) printf("\n%s\n", json_getValue(message));
        }
    } else {
        // Listing: unchanged, print found packages + md5
        if (json_getType(root) == JSON_OBJ) {
            for (json_t const *child = json_getChild(root); child; child = json_getSibling(child)) {
                const char *name = json_getName(child);
                if (!name) continue;
                if (strcmp(name, "Icon") == 0) continue;
                json_t const *cA = json_getProperty(root, name);
                if (!cA) continue;
                json_t const *md5j = json_getProperty(cA, "md5");
                if (md5j) {
                    printf("\n%s: %s\nmd5: %s\n", name,
                           json_getValue(json_getProperty(cA, "name")),
                           json_getValue(md5j));
                }
            }
        }
    }

    free(json_text);
    free(plain);
    return NULL;
}

// ---- sideload ----
static int start_sideload(const char *sideload_file, const char *validate) {
    FILE *fp = fopen(sideload_file, "rb");
    if (!fp) { perror("open sideload file"); return 1; }

    fseek(fp, 0, SEEK_END);
    long file_size = ftell(fp);
    fseek(fp, 0, SEEK_SET);

    char sideload_host_command[256];
    snprintf(sideload_host_command, sizeof(sideload_host_command),
             "sideload-host:%ld:%d:%s:0",
             file_size, ADB_SIDELOAD_CHUNK_SIZE, validate);

    if (send_command(ADB_OPEN, 1, 0, sideload_host_command, (int)strlen(sideload_host_command)+1)) {
        fclose(fp);
        fprintf(stderr, "Failed to OPEN sideload-host\n");
        return 1;
    }

    uint8_t *work = (uint8_t*)malloc(ADB_SIDELOAD_CHUNK_SIZE);
    if (!work) { fclose(fp); return 1; }

    adb_usb_packet pkt;
    char small[64];
    int small_len = 0;
    long total_sent = 0;

    for (;;) {
        if (recv_packet(&pkt, small, sizeof(small)-1, &small_len) != 0) {
            fprintf(stderr, "\nRead error during sideload\n");
            break;
        }
        small[small_len] = 0;

        if (small_len > 8) {
            printf("\n\n%s\n\n", small);
            break; // recovery message
        }

        if (pkt.cmd == ADB_OKAY) {
            send_command(ADB_OKAY, pkt.arg1, pkt.arg0, NULL, 0);
            continue;
        }
        if (pkt.cmd != ADB_WRTE) continue;

        long block = strtol(small, NULL, 10);
        long offset = block * ADB_SIDELOAD_CHUNK_SIZE;
        if (offset > file_size) break;

        int to_write = ADB_SIDELOAD_CHUNK_SIZE;
        if (offset + ADB_SIDELOAD_CHUNK_SIZE > file_size)
            to_write = (int)(file_size - offset);

        fseek(fp, offset, SEEK_SET);
        fread(work, 1, to_write, fp);

        send_command(ADB_WRTE, pkt.arg1, pkt.arg0, work, to_write);
        send_command(ADB_OKAY, pkt.arg1, pkt.arg0, NULL, 0);
        total_sent += to_write;

        int pct = (int)((double)total_sent / (double)file_size * 100.0);
        if (pct > 100) pct = 100;
        printf("\rFlashing in progress ... %d%%", pct);
        fflush(stdout);
    }

    free(work);
    fclose(fp);
    printf("\nDone.\n");
    return 0;
}

// ---- interface discovery (PATCHED) ----
static int check_device(libusb_device *dev) {
    struct libusb_config_descriptor *cfg = NULL;
    if (libusb_get_active_config_descriptor(dev, &cfg) != 0 || !cfg) return 1;

    int found = 1;
    bulk_in = bulk_out = -1;
    interface_num = -1;

    for (int i = 0; i < cfg->bNumInterfaces; i++) {
        const struct libusb_interface *intf = &cfg->interface[i];
        for (int a = 0; a < intf->num_altsetting; a++) {
            const struct libusb_interface_descriptor *d = &intf->altsetting[a];

            if (d->bInterfaceClass == ADB_CLASS &&
                d->bInterfaceSubClass == ADB_SUB_CLASS /* &&
                d->bInterfaceProtocol == ADB_PROTOCOL_CODE */) {

                int in = -1, out = -1;
                for (int e = 0; e < d->bNumEndpoints; e++) {
                    const struct libusb_endpoint_descriptor *ep = &d->endpoint[e];
                    if ((ep->bmAttributes & LIBUSB_TRANSFER_TYPE_MASK) != LIBUSB_TRANSFER_TYPE_BULK) continue;
                    if ((ep->bEndpointAddress & LIBUSB_ENDPOINT_DIR_MASK) == LIBUSB_ENDPOINT_IN) in = ep->bEndpointAddress;
                    else out = ep->bEndpointAddress;
                }
                if (in != -1 && out != -1) {
                    bulk_in = in;
                    bulk_out = out;
                    interface_num = d->bInterfaceNumber;
                    found = 0;
                    goto out;
                }
            }
        }
    }
out:
    libusb_free_config_descriptor(cfg);
    return found;
}

// ---- main ----
int main(int argc, char *argv[]) {
    if (argc == 1) {
        printf("\nVERSION: %s\nRepository: %s\n\n", VERSION, REPOSITORY);
        const char *choices[] = {"Read Info", "ROMs that can be flashed", "Flash Official Recovery ROM", "Format Data", "Reboot"};
        printf("Usage: %s <choice>\n\n  choice > description\n\n", argv[0]);
        for (int i = 0; i < 5; i++) printf("  %d > %s\n\n", i+1, choices[i]);
        return 0;
    }

    int choice = atoi(argv[1]);
    if (choice < 1 || choice > 5) { printf("Invalid choice\n"); return 1; }

    if (libusb_init(&ctx) != 0) { fprintf(stderr, "libusb_init failed\n"); return 1; }

#ifdef _WIN32
    int method = 2;
#else
    int method = (getenv("PREFIX") && access("/data/data/com.termux", F_OK) != -1) ? (geteuid() == 0 ? 2 : 1) : 2;
#endif

    if (method == 1) {
        const char *fd = getenv("TERMUX_USB_FD");
        if (!fd) { printf("\n\nWithout root (termux-usb must be used)\n\n"); libusb_exit(ctx); return 1; }
        if (libusb_wrap_sys_device(ctx, (intptr_t)atoi(fd), &dev_handle) != 0 || check_device(libusb_get_device(dev_handle))) {
            printf("\n\ndevice is not connected, or not in mi assistant mode\n\n");
            libusb_exit(ctx);
            return 1;
        }
    } else {
        libusb_device **devs = NULL;
        ssize_t cnt = libusb_get_device_list(ctx, &devs);
        libusb_device *dev = NULL;
        for (ssize_t i = 0; i < cnt; i++) {
            if (check_device(devs[i]) == 0) { dev = devs[i]; break; }
        }
        if (!dev) {
            printf("\n\ndevice is not connected, or not in mi assistant mode\n\n");
            libusb_free_device_list(devs, 1);
            libusb_exit(ctx);
            return 1;
        }
        int err = libusb_open(dev, &dev_handle);
        if (err != 0 || !dev_handle) {
            printf("libusb_open failed: %d\n", err);
            libusb_free_device_list(devs, 1);
            libusb_exit(ctx);
            return 1;
        }
        // No kernel driver on Windows, but helps on Linux:
        libusb_set_auto_detach_kernel_driver(dev_handle, 1);
        err = libusb_claim_interface(dev_handle, interface_num);
        if (err != 0) {
            printf("claim failed: %d\n", err);
            libusb_close(dev_handle);
            libusb_free_device_list(devs, 1);
            libusb_exit(ctx);
            return 1;
        }
        libusb_free_device_list(devs, 1);
    }

    // ADB CONNECT
    if (send_command(ADB_CONNECT, 0x01000001, ADB_MAX_DATA, "host::\x0", 7) != 0) {
        printf("\nFailed to send CONNECT\n");
        goto cleanup_err;
    }

    adb_usb_packet pkt;
    char buf[512];
    int data_len = 0;
    if (recv_packet(&pkt, buf, sizeof(buf)-1, &data_len) != 0) {
        printf("\nFailed to connect with device\n");
        goto cleanup_err;
    }
    buf[data_len] = 0;

    // DEBUG: print banner and first packet details
    printf("Banner raw: %.*s\n", data_len, buf);
    printf("First packet cmd: %s (0x%08x), arg0=0x%08x, arg1=0x%08x, len=%u\n",
           adb_cmd_name(pkt.cmd), pkt.cmd, pkt.arg0, pkt.arg1, pkt.len);

    // Determine if only sideload is available
    int only_sideload = 0;
    if (strstr(buf, "sideload::") || strstr(buf, "sideload")) {
        only_sideload = 1;
    }

    // queries
    if (!only_sideload) {
        #define SAFE_CP(dest, src) do { char* r = adb_cmd(src); if (!r) { printf("Failed: %s\n", src); goto cleanup_err; } strncpy(dest, r, sizeof(dest)-1); dest[sizeof(dest)-1]=0; } while(0)

        SAFE_CP(device,   "getdevice:");
        SAFE_CP(version,  "getversion:");
        SAFE_CP(sn,       "getsn:");
        SAFE_CP(codebase, "getcodebase:");
        SAFE_CP(branch,   "getbranch:");
        SAFE_CP(language, "getlanguage:");
        SAFE_CP(region,   "getregion:");
        SAFE_CP(romzone,  "getromzone:");
    } else {
        printf("Note: Recovery reports sideload-only banner → skipping get* queries.\n");
        strcpy(device,   "unknown");
        strcpy(version,  "unknown");
        strcpy(sn,       "unknown");
        strcpy(codebase, "unknown");
        strcpy(branch,   "unknown");
        strcpy(language, "unknown");
        strcpy(region,   "unknown");
        strcpy(romzone,  "unknown");
    }

    switch (choice) {
        case 1:
            printf("\n\nDevice: %s\nVersion: %s\nSerial Number: %s\nCodebase: %s\nBranch: %s\nLanguage: %s\nRegion: %s\nROM Zone: %s\n\n",
                   device, version, sn, codebase, branch, language, region, romzone);
            break;
        case 2:
            validate_check("", 0);
            break;
        case 3: {
            char filePath[256], md5[65];
            calculate_md5(filePath, md5);
            const char *val = validate_check(md5, 1);
            if (val) start_sideload(filePath, val);
            break;
        }
        case 4: {
            char *format = adb_cmd("format-data:");
            printf("\n%s\n", format ? format : "(no reply)");
            char *reboot = adb_cmd("reboot:");
            printf("\n%s\n", reboot ? reboot : "(no reply)");
            break;
        }
        case 5: {
            char *reboot = adb_cmd("reboot:");
            printf("\n%s\n", reboot ? reboot : "(no reply)");
            break;
        }
        default:
            printf("Invalid option selected.\n");
            break;
    }

    // cleanup
    libusb_release_interface(dev_handle, interface_num);
    libusb_close(dev_handle);
    libusb_exit(ctx);
    return 0;

cleanup_err:
    if (dev_handle) {
        if (interface_num >= 0) libusb_release_interface(dev_handle, interface_num);
        libusb_close(dev_handle);
    }
    libusb_exit(ctx);
    return 1;
}
