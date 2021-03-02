#include <stdint.h>

typedef struct {
    int64_t protocol_id;
    int32_t action;
    int32_t transaction_id;
} connect_req;

typedef struct {
    int32_t action;
    int32_t transaction_id;
    int64_t connection_id;
} connect_resp;

typedef struct {
    int64_t connection_id;
    int32_t action;
    int32_t transaction_id;
    int8_t info_hash[20];
    int8_t peer_id[20];
    int64_t downloaded;
    int64_t left;
    int64_t uploaded;
    int32_t event;
    uint32_t ip_address;
    uint32_t key;
    int32_t num_want;
    uint16_t port;
} announce_req;

typedef struct {
    int32_t ip_address;
    int16_t tcp_port;
} ip_port;

typedef struct {
    int32_t action;
    int32_t transaction_id;
    int32_t interval;
    int32_t leechers;
    int32_t seeders;
    ip_port ip_port[50];
} announce_resp;

void udp_init(void);

int64_t udp_connect(char *url, char *port);

void udp_announce(announce_resp *resp, int64_t cid, int8_t *hash, char *url, char *port);