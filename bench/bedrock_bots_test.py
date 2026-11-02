"""Bedrock smoke test: BotMark's NetherNet bots join, spawn, move and chat on a fresh server.

Usage: bedrock_bots_test.py <tendril-binary> <botmark-binary> [bots=3] [seconds=45]

Starts `tendril --accept-eula --offline` in an empty folder, runs `botmark --bedrock` against
UDP/TCP 19132 and checks that every bot spawned, that their chat reached the server console and
that the server logged no panic. Prints a JSON summary; exits non-zero on failure.
"""
import json
import os
import subprocess
import sys
import tempfile
import threading
import time

CHAT = "hello from bedrock"


def reader(stream, sink):
    for line in stream:
        sink.append(line.rstrip("\n"))


def main():
    tendril, botmark = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])
    bots = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    seconds = int(sys.argv[4]) if len(sys.argv) > 4 else 45
    folder = tempfile.mkdtemp(prefix="tendril-bedrock-")
    server_lines, bot_lines = [], []
    server = subprocess.Popen([tendril, "--accept-eula", "--offline"], cwd=folder, stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace")
    threading.Thread(target=reader, args=(server.stdout, server_lines), daemon=True).start()
    result = {"bots": bots}
    try:
        deadline = time.monotonic() + 180
        while not any("Ready in" in l for l in server_lines):
            if time.monotonic() > deadline or server.poll() is not None:
                raise AssertionError("server never became ready:\n" + "\n".join(server_lines[-30:]))
            time.sleep(0.1)
        bot = subprocess.Popen([botmark, "--ip", "127.0.0.1:19132", "--bedrock", "--count", str(bots),
                                "--delay", "500", "--timeout", "30000", "--spam_message", CHAT,
                                "--spam_message_delay_min", "2000", "--spam_message_delay_max", "4000"],
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace")
        threading.Thread(target=reader, args=(bot.stdout, bot_lines), daemon=True).start()
        time.sleep(seconds)
        bot.kill()
        bot.wait(timeout=30)
        spawned = sum("spawned into world" in l for l in bot_lines)
        chats = sum(CHAT in l for l in server_lines)
        panics = [l for l in server_lines if "panicked" in l or "Crash report" in l]
        result.update({"spawned": spawned, "chat_lines_on_server": chats, "panics": len(panics)})
        problems = []
        if spawned < bots:
            problems.append(f"only {spawned}/{bots} bots spawned")
        if chats == 0:
            problems.append("no Bedrock chat reached the server")
        if panics:
            problems.append(f"server panicked: {panics[:3]}")
        if problems:
            raise AssertionError("; ".join(problems) + "\n--- bot log\n" + "\n".join(bot_lines[-40:])
                                 + "\n--- server log\n" + "\n".join(server_lines[-40:]))
    except AssertionError as e:
        result["error"] = str(e)
        print(json.dumps(result))
        sys.exit(1)
    finally:
        if server.poll() is None:
            server.stdin.write("stop\n")
            server.stdin.flush()
            try:
                server.wait(timeout=60)
            except subprocess.TimeoutExpired:
                server.kill()
    print(json.dumps(result))


if __name__ == "__main__":
    main()
