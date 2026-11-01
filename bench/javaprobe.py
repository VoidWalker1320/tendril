"""Minimal offline-mode Java Edition login probe for protocol 777 (26.3).

Walks handshake -> login -> configuration -> play and reports how long each
step took. Packet IDs come from Pumpkin's generated table
(crates/pumpkin-data/src/generated/packet.rs), not from memory.
"""
import hashlib
import json
import socket
import struct
import sys
import time
import zlib

PROTOCOL = 777  # 26.3, per minecraft-data protocolVersions.json and Pumpkin's version.rs

# serverbound
SB_HANDSHAKE = 0x00
SB_STATUS_REQUEST = 0
SB_LOGIN_HELLO, SB_LOGIN_CUSTOM_QUERY_ANSWER, SB_LOGIN_ACK, SB_LOGIN_COOKIE = 0, 2, 3, 4
SB_CFG_COOKIE, SB_CFG_FINISH, SB_CFG_KEEP_ALIVE, SB_CFG_PONG = 1, 3, 4, 5
SB_CFG_RESOURCE_PACK, SB_CFG_KNOWN_PACKS, SB_CFG_ACCEPT_COC = 6, 7, 9
SB_PLAY_ACCEPT_TELEPORT, SB_PLAY_KEEP_ALIVE, SB_PLAY_PONG = 0, 28, 45
# clientbound
CB_STATUS_RESPONSE = 0
CB_LOGIN_DISCONNECT, CB_LOGIN_HELLO, CB_LOGIN_FINISHED = 0, 1, 2
CB_LOGIN_COMPRESSION, CB_LOGIN_CUSTOM_QUERY, CB_LOGIN_COOKIE = 3, 4, 5
CB_CFG_COOKIE, CB_CFG_DISCONNECT, CB_CFG_FINISH, CB_CFG_KEEP_ALIVE, CB_CFG_PING = 0, 2, 3, 4, 5
CB_CFG_RESOURCE_PACK_PUSH, CB_CFG_KNOWN_PACKS, CB_CFG_CODE_OF_CONDUCT = 9, 15, 20
CB_PLAY_DISCONNECT, CB_PLAY_KEEP_ALIVE, CB_PLAY_LOGIN = 32, 45, 50
CB_PLAY_PING, CB_PLAY_PLAYER_POSITION = 62, 73


class ProbeError(Exception):
    pass


def varint(n):
    n &= 0xFFFFFFFF
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def mc_string(s):
    raw = s.encode()
    return varint(len(raw)) + raw


def offline_uuid(name):
    # Java's UUID.nameUUIDFromBytes("OfflinePlayer:" + name), as vanilla does in offline mode.
    h = bytearray(hashlib.md5(f"OfflinePlayer:{name}".encode()).digest())
    h[6] = (h[6] & 0x0F) | 0x30
    h[8] = (h[8] & 0x3F) | 0x80
    return bytes(h)


class Reader:
    def __init__(self, data):
        self.data, self.pos = data, 0

    def varint(self):
        result = shift = 0
        while True:
            if self.pos >= len(self.data):
                raise ProbeError("truncated varint")
            b = self.data[self.pos]
            self.pos += 1
            result |= (b & 0x7F) << shift
            if not b & 0x80:
                break
            shift += 7
            if shift > 35:
                raise ProbeError("varint too long")
        return result - (1 << 32) if result & (1 << 31) else result

    def take(self, n):
        if self.pos + n > len(self.data):
            raise ProbeError("truncated field")
        chunk = self.data[self.pos:self.pos + n]
        self.pos += n
        return chunk

    def string(self):
        return self.take(self.varint()).decode(errors="replace")

    def rest(self):
        return self.data[self.pos:]


class Conn:
    def __init__(self, sock):
        self.sock = sock
        self.buf = bytearray()
        self.threshold = -1

    def _recv_exact(self, n):
        while len(self.buf) < n:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise ProbeError("connection closed by server")
            self.buf += chunk
        out = bytes(self.buf[:n])
        del self.buf[:n]
        return out

    def _read_varint_stream(self):
        result = shift = 0
        while True:
            b = self._recv_exact(1)[0]
            result |= (b & 0x7F) << shift
            if not b & 0x80:
                return result
            shift += 7
            if shift > 35:
                raise ProbeError("frame length varint too long")

    def read(self):
        length = self._read_varint_stream()
        if length > 1 << 23:
            raise ProbeError(f"frame too large: {length}")
        r = Reader(self._recv_exact(length))
        if self.threshold >= 0:
            data_len = r.varint()
            body = r.rest()
            if data_len:
                body = zlib.decompress(body)
            r = Reader(body)
        return r.varint(), r

    def send(self, packet_id, payload=b""):
        body = varint(packet_id) + payload
        if self.threshold >= 0:
            if len(body) >= self.threshold:
                body = varint(len(body)) + zlib.compress(body)
            else:
                body = varint(0) + body
        self.sock.sendall(varint(len(body)) + body)


def login(host="127.0.0.1", port=25565, name="probe", timeout=30.0):
    """Join once; return timings in ms measured from the start of the TCP connect."""
    t0 = time.monotonic()
    ms = lambda: round((time.monotonic() - t0) * 1000, 1)
    marks = {}
    sock = socket.create_connection((host, port), timeout=timeout)
    sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    marks["connected"] = ms()
    conn = Conn(sock)
    deadline = t0 + timeout
    try:
        conn.send(SB_HANDSHAKE, varint(PROTOCOL) + mc_string(host) + struct.pack(">H", port) + varint(2))
        conn.send(SB_LOGIN_HELLO, mc_string(name) + offline_uuid(name))
        state = "login"
        while time.monotonic() < deadline:
            pid, r = conn.read()
            if state == "login":
                if pid == CB_LOGIN_COMPRESSION:
                    conn.threshold = r.varint()
                elif pid == CB_LOGIN_FINISHED:
                    marks["login_finished"] = ms()
                    conn.send(SB_LOGIN_ACK)
                    state = "config"
                elif pid == CB_LOGIN_HELLO:
                    raise ProbeError("server asked for encryption (online mode); probe needs online_mode = false")
                elif pid == CB_LOGIN_DISCONNECT:
                    raise ProbeError(f"disconnected during login: {r.rest()[:300]!r}")
                elif pid == CB_LOGIN_CUSTOM_QUERY:
                    conn.send(SB_LOGIN_CUSTOM_QUERY_ANSWER, varint(r.varint()) + b"\x00")
                elif pid == CB_LOGIN_COOKIE:
                    conn.send(SB_LOGIN_COOKIE, mc_string(r.string()) + b"\x00")
            elif state == "config":
                if pid == CB_CFG_KNOWN_PACKS:
                    # Answer like a vanilla 26.3 client: we know every pack the server lists.
                    count = r.varint()
                    packs = [(r.string(), r.string(), r.string()) for _ in range(count)]
                    conn.send(SB_CFG_KNOWN_PACKS, varint(len(packs)) + b"".join(mc_string(a) + mc_string(b) + mc_string(c) for a, b, c in packs))
                elif pid == CB_CFG_KEEP_ALIVE:
                    conn.send(SB_CFG_KEEP_ALIVE, r.take(8))
                elif pid == CB_CFG_PING:
                    conn.send(SB_CFG_PONG, r.take(4))
                elif pid == CB_CFG_CODE_OF_CONDUCT:
                    conn.send(SB_CFG_ACCEPT_COC)
                elif pid == CB_CFG_COOKIE:
                    conn.send(SB_CFG_COOKIE, mc_string(r.string()) + b"\x00")
                elif pid == CB_CFG_RESOURCE_PACK_PUSH:
                    conn.send(SB_CFG_RESOURCE_PACK, r.take(16) + varint(0))
                elif pid == CB_CFG_FINISH:
                    marks["config_finished"] = ms()
                    conn.send(SB_CFG_FINISH)
                    state = "play"
                elif pid == CB_CFG_DISCONNECT:
                    raise ProbeError(f"disconnected during configuration: {r.rest()[:300]!r}")
            else:
                if pid == CB_PLAY_LOGIN:
                    marks["play_login"] = ms()
                elif pid == CB_PLAY_PLAYER_POSITION:
                    conn.send(SB_PLAY_ACCEPT_TELEPORT, varint(r.varint()))
                    marks["spawn_position"] = ms()
                    return marks
                elif pid == CB_PLAY_KEEP_ALIVE:
                    conn.send(SB_PLAY_KEEP_ALIVE, r.take(8))
                elif pid == CB_PLAY_PING:
                    conn.send(SB_PLAY_PONG, r.take(4))
                elif pid == CB_PLAY_DISCONNECT:
                    raise ProbeError(f"disconnected during play: {r.rest()[:300]!r}")
        raise ProbeError(f"timed out in state {state}; marks so far {marks}")
    finally:
        sock.close()


def status(host="127.0.0.1", port=25565, timeout=10.0):
    """Server-list ping: returns the status JSON (version, players, description)."""
    sock = socket.create_connection((host, port), timeout=timeout)
    try:
        conn = Conn(sock)
        conn.send(SB_HANDSHAKE, varint(PROTOCOL) + mc_string(host) + struct.pack(">H", port) + varint(1))
        conn.send(SB_STATUS_REQUEST)
        pid, r = conn.read()
        if pid != CB_STATUS_RESPONSE:
            raise ProbeError(f"unexpected status packet {pid}")
        return json.loads(r.string())
    finally:
        sock.close()


if __name__ == "__main__":
    host = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1"
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 25565
    try:
        print(json.dumps(login(host, port)))
    except (ProbeError, OSError) as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)
