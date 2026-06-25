// packet_logger.h - Redis-based packet logger for SuperSS-Dev
// Fire-and-forget XADD to Redis Stream "pangya:packets"
#pragma once
#ifndef _PACKET_LOGGER_H
#define _PACKET_LOGGER_H

#include <hiredis/hiredis.h>
#include <string>
#include <cstring>
#include <cstdio>

namespace packet_logger {

    // Global connection state
    static redisContext* g_redis = nullptr;

    inline void init() {
        if (g_redis != nullptr) return;
        struct timeval tv = { 0, 200000 }; // 200ms timeout
        g_redis = redisConnectWithTimeout("pangya_redis", 6379, tv);
        if (g_redis == nullptr || g_redis->err) {
            if (g_redis) { redisFree(g_redis); g_redis = nullptr; }
        }
    }

    // Convert buffer to hex string (compact, no spaces)
    inline std::string to_hex(const unsigned char* buf, size_t len) {
        if (!buf || len == 0) return "";
        // Cap hex dump to 2000 bytes (4000 hex chars) to keep entries small
        size_t cap = len > 2000 ? 2000 : len;
        static const char hex[] = "0123456789ABCDEF";
        std::string out;
        out.reserve(cap * 2);
        for (size_t i = 0; i < cap; i++) {
            out += hex[(buf[i] >> 4) & 0xF];
            out += hex[buf[i] & 0xF];
        }
        return out;
    }

    // Main logging function - fire and forget
    // dir: "C2S" (client to server) or "S2C" (server to client)
    // srv: server name ("GS", "LS", "MS", "AS", "RS")
    // packet_id: the packet type/opcode (first 2 bytes of plaintext)
    // is_known: whether this packet has a registered handler
    inline void log(const char* dir, const char* srv,
                    unsigned short packet_id, bool is_known,
                    const unsigned char* payload, size_t payload_size,
                    uint32_t uid, const char* ip) {

        if (g_redis == nullptr) init();
        if (g_redis == nullptr || g_redis->err) return;

        char id_str[8], size_str[16], uid_str[16], known_str[4];
        snprintf(id_str, sizeof(id_str), "0x%04X", packet_id);
        snprintf(size_str, sizeof(size_str), "%zu", payload_size);
        snprintf(uid_str, sizeof(uid_str), "%u", uid);
        snprintf(known_str, sizeof(known_str), "%d", is_known ? 1 : 0);

        std::string hex = to_hex(payload, payload_size);
        const char* ip_str = (ip && ip[0]) ? ip : "";

        // XADD pangya:packets MAXLEN ~5000 *
        //   dir  <C2S|S2C>  srv  <name>  pid  <0xXXXX>
        //   known  <0|1>  size  <N>  hex  <dump>
        //   uid  <N>  ip  <addr>
        const char* argv[20];
        size_t argvlen[20];
        int argc = 0;

        #define ADD(s) argv[argc] = (s); argvlen[argc] = strlen(s); argc++;

        ADD("XADD");
        ADD("pangya:packets");
        ADD("MAXLEN");
        ADD("~5000");
        ADD("*");
        ADD("dir");    ADD(dir);
        ADD("srv");    ADD(srv);
        ADD("pid");    ADD(id_str);
        ADD("known");  ADD(known_str);
        ADD("size");   ADD(size_str);
        ADD("hex");    argv[argc] = hex.c_str(); argvlen[argc] = hex.size(); argc++;
        ADD("uid");    ADD(uid_str);
        ADD("ip");     ADD(ip_str);

        #undef ADD

        redisReply* reply = (redisReply*)redisCommandArgv(g_redis, argc, argv, argvlen);
        if (reply) freeReplyObject(reply);

        // If connection broke, reset so next call tries to reconnect
        if (g_redis->err) {
            redisFree(g_redis);
            g_redis = nullptr;
        }
    }

    // Convenience wrappers
    inline void log_recv(unsigned short packet_id, bool is_known,
                         const unsigned char* payload, size_t payload_size,
                         uint32_t uid, const char* ip,
                         const char* srv = "GS") {
        log("C2S", srv, packet_id, is_known, payload, payload_size, uid, ip);
    }

    inline void log_send(unsigned short packet_id,
                         const unsigned char* payload, size_t payload_size,
                         uint32_t uid, const char* ip,
                         const char* srv = "GS") {
        log("S2C", srv, packet_id, true, payload, payload_size, uid, ip);
    }
}

#endif
