#include <time.h>
#include <netdb.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <byteswap.h>
#include <sys/time.h>
#include <arpa/inet.h>
#include <sys/types.h>
#include <sys/socket.h>

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

int s;

void udp_init(void) {
    s = socket(AF_INET, SOCK_DGRAM, 0);
    if(s < 0) perror("socket error");
    struct sockaddr_in src;
    src.sin_family = AF_UNSPEC;
    src.sin_port = htons(25565);
    src.sin_addr.s_addr = INADDR_ANY;
    memset(&src.sin_zero, 0, 8);
    int ret = bind(s, (struct sockaddr*)&src, sizeof(src)); 
    if(ret < 0) perror("bind error");
    struct timeval tv;
    tv.tv_sec = 15; tv.tv_usec = 0;
    ret = setsockopt(s, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
    if(ret < 0) perror("setsockopt error");
}

int64_t udp_connect(char *url, char *port) {
    srand(time(NULL));
    connect_req req = {bswap_64(0x41727101980LL), 0, rand()};

    struct addrinfo *res;
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_DGRAM;
    getaddrinfo(url, port, &hints, &res);

    int ret = sendto(s, &req, sizeof(req), 0, res->ai_addr, res->ai_addrlen);
    if(ret == -1)
        perror("sendto error");

    connect_resp resp = {0,0,0};
    ret = recvfrom(s, &resp, sizeof(resp), 0, NULL, NULL);
    if(ret < 1) perror("recv error");

    if(resp.transaction_id != req.transaction_id || resp.action != 0) return -1;
    freeaddrinfo(res);
    
    return resp.connection_id;
}

void udp_announce(announce_resp *resp, int64_t cid, int8_t *hash, char *url, char *port) {
    srand(time(NULL));
    announce_req req = {cid, bswap_32(1), rand(), 
                            {0}, {0}, 0, 0, 0, 0, 0, 0, bswap_32(50), 0};
    memcpy(req.info_hash, hash, 20);

    struct addrinfo *res;
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_DGRAM;
    getaddrinfo(url, port, &hints, &res);

    int ret = sendto(s, &req, sizeof(req), 0, res->ai_addr, res->ai_addrlen);
    if(ret == -1)
        perror("sendto error");

    ret = recvfrom(s, resp, sizeof(announce_resp), 0, NULL, NULL);
    if(ret < 1) perror("recv error");

    if(resp->transaction_id != req.transaction_id || resp->action != bswap_32(1)) resp = NULL;
    freeaddrinfo(res);
}