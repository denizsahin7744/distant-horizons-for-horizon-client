#!/usr/bin/env python3
"""Distant Horizons on a Distant Horizons server, in the running modded game on fresh data (tools/game.py): the
game joins the server, asks it for the sections around the player, draws them past the render distance (the view
differs with them off) and keeps them on disk under the server's folder; joined again, it draws them from disk and
the server finds them up to date (sync on load).

    python3 tools/server_proof.py --resources /path/to/.local/horizon-client/26.3 [--server 127.0.0.1:25565]

The server: a Fabric or NeoForge one with Distant Horizons 3.3.4 (protocol 16), or Paper with the Distant Horizons
Support plugin (0.14, protocol 15, or 0.15), offline mode, no whitelist, the
player `DhProof` an operator (spectator mode, noon and clear weather are set from the game's chat). Linux without a display: run under
Xvfb (`xvfb-run -a`), with Mesa's lavapipe for Vulkan.
"""
import argparse
import json
import os
import re
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from game import Game, difference  # noqa: E402
from proof import CTL, HORIZON, ROOT, dh  # noqa: E402

STATUS = re.compile(r"Distant Horizons [\d.]+: (\d+) of (\d+) sections drawn \((\d+) faces\), (\d+) jobs queued.*?"
                    r"LODs kept in ([^;]+); server ([^:]+): (\d+) received, (\d+) up to date, (\d+) waiting, "
                    r"(\d+) asked, (\d+) failed")
OFF = re.compile(r"Distant Horizons [\d.]+: (\d+) of (\d+) sections drawn .*?LODs kept in ([^;]+); server LODs off")


class ServerGame(Game):
    """The game joining a server; `dirs` again: the same game folders as an earlier run."""

    def __init__(self, ctl, resources, out, server, dirs=None):
        super().__init__(ctl, resources, out)
        self.server, self.reuse = server, dirs

    def __enter__(self):
        self.out.mkdir(parents=True, exist_ok=True)
        self.dirs = self.reuse or [tempfile.mkdtemp(prefix=f"dh-server-{n}-") for n in ("home", "launcher", "data",
                                                                                          "config")]
        self.home = Path(self.dirs[0])
        self.socket = str(Path(tempfile.mkdtemp(prefix="dh-ctl-")) / "ctl.sock")
        self.env = dict(os.environ, HORIZON_HOME=self.dirs[0], HORIZON_LAUNCHER_HOME=self.dirs[1],
                        XDG_DATA_HOME=self.dirs[2], XDG_CONFIG_HOME=self.dirs[3],
                        HORIZON_LAUNCHER_NO_SELF_UPDATE="1", HORIZON_NO_AUDIO="1")
        self.run("launch", "--server", self.server, "--name", "DhProof", "--offscreen", "--wait",
                 "--resources", str(self.resources))
        time.sleep(3)
        self.since = time.strftime("%H:%M:%S")
        return self

    def __exit__(self, *exc):
        super().__exit__(*exc)
        for _ in range(60):
            try:
                self.run("status")
            except Exception:
                break
            time.sleep(1)
        time.sleep(5)


def status(g):
    dh(g)
    lines = str(g.run("logs", "--lines", "100000")).splitlines()
    for found in reversed([STATUS.search(line) for line in lines if line[1:9] >= g.since]):
        if not found:
            continue
        shown, wanted, faces, queued, folder, state, received, up_to_date, waiting, asked, failed = found.groups()
        return {"shown": int(shown), "wanted": int(wanted), "faces": int(faces), "queued": int(queued),
                "folder": folder, "server": state, "received": int(received), "up_to_date": int(up_to_date),
                "waiting": int(waiting), "asked": int(asked), "failed": int(failed)}
    return None


def status_off(g):
    dh(g)
    lines = str(g.run("logs", "--lines", "100000")).splitlines()
    for found in reversed([OFF.search(line) for line in lines if line[1:9] >= g.since]):
        if found:
            return {"shown": int(found[1]), "wanted": int(found[2]), "folder": found[3]}
    return None


def drawn(g, limit=300):
    start, s = time.time(), None
    while time.time() - start < limit:
        s = status(g)
        if s and s["wanted"] > 0 and s["shown"] >= s["wanted"] and s["waiting"] == 0 and s["asked"] == 0:
            break
        time.sleep(3)
    return s, round(time.time() - start, 1)


def run(resources, out, server, limit):
    out = Path(out)
    with ServerGame(CTL, resources, out, server) as g:
        dirs = g.dirs
        for command in ["/gamemode spectator", "/time set noon", "/weather clear",
                        "/execute in minecraft:overworld run tp @s 106 200 -35 0 15"]:
            g.run("key", "t", "tap")
            time.sleep(0.4)
            g.run("type", command)
            g.run("key", "enter", "tap")
            time.sleep(1)
        g.run("key", "f1", "tap")
        first, seconds = drawn(g, limit)
        assert first and first["server"].startswith("connected") and first["failed"] == 0, first
        assert first["shown"] == first["wanted"] > 0 and first["received"] > 0, first
        time.sleep(2)
        on = g.shot("server-dh-on")
        dh(g, "off")
        time.sleep(3)
        off = g.shot("server-dh-off")
        dh(g, "on")
        time.sleep(2)
    folder = Path(first["folder"])
    files = [p for p in folder.rglob("*") if p.is_file()]
    with ServerGame(CTL, resources, out, server, dirs) as g:
        g.run("key", "f1", "tap")
        again, seconds_again = drawn(g)
        dh(g, "server off")
        time.sleep(5)
        off_status = status_off(g)
        dh(g, "bandwidth 1000")
        dh(g, "server on")
        back, _ = drawn(g, 120)
        dh(g, "bandwidth 500")
    results = {
        "first_join": first,
        "seconds_to_draw": seconds,
        "diff_horizon_on_off": round(difference(on, off, HORIZON), 1),
        "section_files": len(files),
        "second_join": again,
        "seconds_to_draw_again": seconds_again,
        "server_lods_off": off_status,
        "server_lods_on_again": back,
    }
    level = Path(first["folder"]).name
    assert (level.endswith("minecraft_overworld") or level == "world") and "/servers/" in first["folder"], results
    assert results["section_files"] > 0, results
    assert results["diff_horizon_on_off"] > 4, results
    assert again and again["shown"] == again["wanted"] and again["up_to_date"] >= again["wanted"] * 0.9, results
    assert off_status and off_status["folder"] == first["folder"] and off_status["shown"] >= off_status["wanted"] * 0.9, results
    assert back and back["server"].startswith("connected") and back["failed"] == 0 and back["shown"] == back["wanted"], results
    print(json.dumps({"proof": "distant_horizons_server", "ok": True, **results}, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--resources", required=True, help="prepared Horizon resources for Minecraft 26.3")
    parser.add_argument("--server", default="127.0.0.1:25565", help="the Distant Horizons server")
    parser.add_argument("--limit", type=int, default=300, help="seconds for the first join's sections to be drawn")
    parser.add_argument("--out", default=str(ROOT / "target" / "proof"), help="where the screenshots go")
    args = parser.parse_args()
    run(args.resources, args.out, args.server, args.limit)


if __name__ == "__main__":
    main()
