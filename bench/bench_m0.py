"""M0 baseline: startup (process exec -> first Java login) and idle RSS for a Pumpkin binary.

Usage: bench_m0.py <binary> <workdir> [runs=3] [idle_seconds=60]

Run 0 boots a fresh world (generation included); runs 1..N reuse that world.
Each run: exec -> poll the probe until a bot reaches the spawn-position packet,
disconnect, idle with 0 players, then sample /proc. Median of runs 1..N is reported.
"""
import json
import os
import re
import shutil
import socket
import statistics
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import javaprobe  # noqa: E402

READY = re.compile(r"Started server; took (\d+)ms")
CLK_TCK = os.sysconf("SC_CLK_TCK")

OVERRIDES = {
    # Same world for every binary under test.
    ("", "seed"): '"20261101"',
    # Probe is an offline-mode client; telemetry would add network traffic to every run.
    ("networking.java", "online_mode"): "false",
    ("networking.java", "encryption"): "false",
    ("networking.bedrock", "online_mode"): "false",
    ("telemetry", "enabled"): "false",
    ("logging", "color"): "false",
    ("commands", "use_tty"): "false",
}


def patch_config(path):
    """Apply OVERRIDES to a generated pumpkin.toml without a TOML library (keeps comments/order)."""
    out, section = [], ""
    for line in open(path).read().splitlines():
        m = re.match(r"^\[([^\]]+)\]\s*$", line)
        if m:
            section = m.group(1)
        else:
            k = line.split("=", 1)[0].strip()
            if (section, k) in OVERRIDES:
                line = f"{k} = {OVERRIDES[(section, k)]}"
        out.append(line)
    open(path, "w").write("\n".join(out) + "\n")


def proc_status(pid):
    fields = {}
    with open(f"/proc/{pid}/status") as f:
        for line in f:
            k, _, v = line.partition(":")
            # VmSwap matters on phones: Android swaps idle processes, which shrinks VmRSS.
            if k in ("VmRSS", "VmHWM", "VmSwap", "Threads", "RssAnon", "RssFile"):
                fields[k] = int(v.split()[0])
    return fields


def cpu_ticks(pid):
    with open(f"/proc/{pid}/stat") as f:
        parts = f.read().rsplit(")", 1)[1].split()
    return int(parts[11]) + int(parts[12])  # utime + stime


def machine_load():
    with open("/proc/meminfo") as f:
        mem = {l.split(":")[0]: int(l.split()[1]) for l in f}
    return {"loadavg": open("/proc/loadavg").read().split()[:3], "mem_available_mb": mem["MemAvailable"] // 1024,
            "swap_used_mb": (mem["SwapTotal"] - mem["SwapFree"]) // 1024}


def one_run(binary, workdir, idle_s, label):
    log = open(os.path.join(workdir, f"{label}.log"), "w")
    marks = {"label": label, "machine_before": machine_load()}
    t0 = time.monotonic()
    proc = subprocess.Popen([binary], cwd=workdir, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, bufsize=1)

    def pump():
        for line in proc.stdout:
            log.write(line)
            m = READY.search(line)
            if m and "ready_log_ms" not in marks:
                marks["ready_log_ms"] = round((time.monotonic() - t0) * 1000, 1)
                marks["self_reported_ms"] = int(m.group(1))
        log.flush()

    threading.Thread(target=pump, daemon=True).start()
    try:
        deadline = t0 + 120
        while time.monotonic() < deadline:
            try:
                socket.create_connection(("127.0.0.1", 25565), timeout=0.2).close()
                marks["port_open_ms"] = round((time.monotonic() - t0) * 1000, 1)
                break
            except OSError:
                time.sleep(0.005)
        while time.monotonic() < deadline:
            attempt_start = time.monotonic()
            try:
                p = javaprobe.login(name="bench", timeout=60)
                base = (attempt_start - t0) * 1000
                marks["first_play_login_ms"] = round(base + p["play_login"], 1)
                marks["first_spawn_ms"] = round(base + p["spawn_position"], 1)
                marks["login_breakdown_ms"] = p
                break
            except OSError:
                time.sleep(0.01)
            except javaprobe.ProbeError as e:
                marks["probe_error"] = str(e)
                break
        marks["rss_after_login_kb"] = proc_status(proc.pid)["VmRSS"]
        time.sleep(1)  # let the server process the disconnect before the idle window
        c0, w0 = cpu_ticks(proc.pid), time.monotonic()
        time.sleep(idle_s)
        c1, w1 = cpu_ticks(proc.pid), time.monotonic()
        marks["idle_cpu_pct"] = round((c1 - c0) / CLK_TCK / (w1 - w0) * 100, 2)
        marks["idle"] = proc_status(proc.pid)
        marks["machine_after"] = machine_load()
        # A run whose pages were pushed to swap reports a VmRSS that is too low to compare.
        marks["swap_contaminated"] = marks["idle"].get("VmSwap", 0) > 1024
        ts = time.monotonic()
        proc.stdin.write("stop\n")
        proc.stdin.flush()
        proc.wait(timeout=120)
        marks["shutdown_ms"] = round((time.monotonic() - ts) * 1000, 1)
    finally:
        if proc.poll() is None:
            proc.kill()
        log.close()
    return marks


def main():
    binary = os.path.abspath(sys.argv[1])
    workdir = os.path.abspath(sys.argv[2])
    runs = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    idle_s = int(sys.argv[4]) if len(sys.argv) > 4 else 60
    os.makedirs(workdir, exist_ok=True)
    cfg = os.path.join(workdir, "pumpkin.toml")
    if not os.path.exists(cfg):
        # Let the binary write its own default config, then patch it.
        p = subprocess.Popen([binary], cwd=workdir, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL, text=True)
        for _ in range(600):
            if os.path.exists(cfg):
                break
            time.sleep(0.05)
        p.kill()
        p.wait()
        shutil.rmtree(os.path.join(workdir, "world"), ignore_errors=True)
    patch_config(cfg)

    results = []
    for i in range(runs + 1):
        label = "run0_fresh_world" if i == 0 else f"run{i}_existing_world"
        r = one_run(binary, workdir, idle_s, label)
        print(json.dumps(r), flush=True)
        results.append(r)

    warm = [r for r in results[1:] if "first_spawn_ms" in r]
    if warm:
        med = lambda key: statistics.median(r[key] for r in warm)
        summary = {
            "binary": binary, "binary_bytes": os.path.getsize(binary), "runs": len(warm),
            "median_port_open_ms": med("port_open_ms"), "median_ready_log_ms": med("ready_log_ms"),
            "median_self_reported_ms": med("self_reported_ms"),
            "median_first_play_login_ms": med("first_play_login_ms"),
            "median_first_spawn_ms": med("first_spawn_ms"),
            "median_idle_rss_mb": round(statistics.median(r["idle"]["VmRSS"] for r in warm) / 1024, 1),
            "median_idle_hwm_mb": round(statistics.median(r["idle"]["VmHWM"] for r in warm) / 1024, 1),
            "median_idle_swap_mb": round(statistics.median(r["idle"].get("VmSwap", 0) for r in warm) / 1024, 1),
            "median_idle_rss_plus_swap_mb": round(statistics.median(
                r["idle"]["VmRSS"] + r["idle"].get("VmSwap", 0) for r in warm) / 1024, 1),
            "swap_contaminated_runs": sum(r.get("swap_contaminated", False) for r in warm),
            "median_idle_cpu_pct": statistics.median(r["idle_cpu_pct"] for r in warm),
            "fresh_world_first_spawn_ms": results[0].get("first_spawn_ms"),
            "fresh_world_idle_rss_mb": round(results[0]["idle"]["VmRSS"] / 1024, 1) if "idle" in results[0] else None,
        }
        print("SUMMARY " + json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
