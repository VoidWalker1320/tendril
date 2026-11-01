"""M1 gate: an empty folder becomes a joinable server with one command.

Usage: first_run_test.py <tendril-binary>

1. Empty folder, `tendril --accept-eula` (defaults, online mode): the server must create
   server.toml, eula.txt, plugins/, mods/, logs/, write no pumpkin.toml, answer a status ping
   as protocol 777, and stop cleanly on `stop`.
2. Another empty folder, `tendril --accept-eula --offline`: the probe bot must spawn. The
   probe has no Microsoft account, so this is the only way a scripted client can join.

Prints one JSON line per scenario and exits non-zero on the first failure.
"""
import json
import os
import re
import socket
import subprocess
import sys
import tempfile
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import javaprobe  # noqa: E402

READY = re.compile(r"Ready in (\d+) ms, RSS ([\d.]+) MiB")
EXPECTED_FILES = ["server.toml", "eula.txt", "plugins", "mods", "logs"]


class Server:
    def __init__(self, binary, folder, args):
        self.lines = []
        self.t0 = time.monotonic()
        self.proc = subprocess.Popen([binary, *args], cwd=folder, stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                     text=True, bufsize=1, encoding="utf-8", errors="replace")
        threading.Thread(target=self._pump, daemon=True).start()

    def _pump(self):
        for line in self.proc.stdout:
            self.lines.append(line.rstrip("\n"))

    def wait_for(self, pattern, timeout):
        rx = re.compile(pattern)
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            for line in self.lines:
                m = rx.search(line)
                if m:
                    return m
            if self.proc.poll() is not None:
                raise AssertionError(f"server exited with {self.proc.returncode}:\n" + "\n".join(self.lines[-30:]))
            time.sleep(0.05)
        raise AssertionError(f"no line matching {pattern!r} within {timeout}s:\n" + "\n".join(self.lines[-30:]))

    def wait_port(self, port, timeout):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                socket.create_connection(("127.0.0.1", port), timeout=0.2).close()
                return round((time.monotonic() - self.t0) * 1000, 1)
            except OSError:
                if self.proc.poll() is not None:
                    raise AssertionError(f"server exited with {self.proc.returncode}:\n" + "\n".join(self.lines[-30:]))
                time.sleep(0.02)
        raise AssertionError(f"port {port} not open within {timeout}s")

    def stop(self, timeout=120):
        self.proc.stdin.write("stop\n")
        self.proc.stdin.flush()
        code = self.proc.wait(timeout=timeout)
        return code

    def kill(self):
        if self.proc.poll() is None:
            self.proc.kill()


def defaults_scenario(binary):
    with tempfile.TemporaryDirectory(prefix="tendril-defaults-") as folder:
        assert not os.listdir(folder)
        server = Server(binary, folder, ["--accept-eula"])
        try:
            port_ms = server.wait_port(25565, 180)
            ready = server.wait_for(READY.pattern, 180)
            server.wait_for(r"Tier: Lite \(no \.jar files", 5)
            server.wait_for(r"Profile: ", 5)
            st = javaprobe.status()
            assert st["version"]["protocol"] == javaprobe.PROTOCOL, st
            motd = json.dumps(st.get("description"))
            assert "lightweight 26.3 server" in motd, motd
            for name in EXPECTED_FILES:
                assert os.path.exists(os.path.join(folder, name)), f"{name} missing"
            assert not os.path.exists(os.path.join(folder, "pumpkin.toml")), "pumpkin.toml written"
            code = server.stop()
            assert code == 0, f"exit code {code}"
        finally:
            server.kill()
        return {"scenario": "defaults", "port_open_ms": port_ms, "ready_ms": int(ready.group(1)),
                "rss_mib": float(ready.group(2)), "status_protocol": st["version"]["protocol"]}


def offline_join_scenario(binary):
    with tempfile.TemporaryDirectory(prefix="tendril-join-") as folder:
        server = Server(binary, folder, ["--accept-eula", "--offline"])
        try:
            server.wait_port(25565, 180)
            attempt = time.monotonic()
            marks = javaprobe.login(name="firstrun", timeout=120)
            spawn_ms = round((attempt - server.t0) * 1000 + marks["spawn_position"], 1)
            code = server.stop()
            assert code == 0, f"exit code {code}"
        finally:
            server.kill()
        return {"scenario": "offline_join", "exec_to_spawn_ms": spawn_ms, "login_breakdown_ms": marks}


def main():
    binary = os.path.abspath(sys.argv[1])
    for scenario in (defaults_scenario, offline_join_scenario):
        try:
            print(json.dumps(scenario(binary)), flush=True)
        except (AssertionError, javaprobe.ProbeError, OSError, subprocess.TimeoutExpired) as e:
            print(json.dumps({"scenario": scenario.__name__, "error": str(e)}), flush=True)
            sys.exit(1)


if __name__ == "__main__":
    main()
