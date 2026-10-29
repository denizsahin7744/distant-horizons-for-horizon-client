#!/usr/bin/env python3
"""Build Distant Horizons for Horizon Client into Horizon, test it there, package it.

    python3 tools/horizon.py stage      # apply the bundle on the pinned Horizon: target/stage/
    python3 tools/horizon.py check      # ... and cargo check the game
    python3 tools/horizon.py test       # ... and run the mod's tests inside the game's test binary
    python3 tools/horizon.py build      # ... and build the game (fast profile); prints the binary
    python3 tools/horizon.py package    # target/distant_horizons-<version>.hnmod, as the launcher installs it
    python3 tools/horizon.py set --with ~/Library/Application\ Support/horizon/instances/native-mods/horizon-native-mods
                                        # target/native-set/: this mod and the others, for `horizon-launcher native-build`
    --horizon PATH   a Horizon git checkout holding the pinned commit (else GitHub's archive of it)
    --with PATH      another native mod to build in with it (a bundle folder, an .hnmod, or a folder of them;
                     repeatable), as the launcher builds a player's set: on this mod's commit

Adapted from horizon-mods' native/tools/native.py (MIT). Horizon's source is exported from the pinned commit,
never a working tree; horizon-mixins is built once at its pinned commit. Everything goes under target/.
"""
import argparse
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
BUNDLE = ROOT / "mixins" / "distant_horizons"
COMPAT = json.loads((ROOT / "compatibility.json").read_text())
MANIFEST = tomllib.loads((BUNDLE / "horizon-mixin.toml").read_text())
ID = MANIFEST["mod"]["id"]
VERSION = MANIFEST["mod"]["version"]
# Our modules' warnings fail the build; Horizon's own (one on macOS) don't.
OURS = re.compile(r"^(warning|error)(\[\w+\])?: .*\n\s*--> crates/horizon-client/src/game\.rs", re.M)


def log(*args):
    print(*args, file=sys.stderr, flush=True)


def source(horizon):
    """Horizon at the pinned commit, exported once into target/horizon/<revision>/."""
    revision = COMPAT["horizon_revision"]
    out = TARGET / "horizon" / revision
    if (out / ".horizon-revision").is_file():
        return out
    partial = TARGET / "horizon" / f".{revision}.partial"
    shutil.rmtree(partial, ignore_errors=True)
    partial.mkdir(parents=True)
    if horizon:
        log(f"Exporting Horizon {revision[:7]} from {horizon}")
        archive = subprocess.run(["git", "-C", horizon, "archive", revision], check=True, capture_output=True).stdout
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(partial, filter="data")
        top = partial
    else:
        log(f"Downloading Horizon {revision[:7]}")
        url = f"https://codeload.github.com/horizon-rust/horizon/tar.gz/{revision}"
        request = urllib.request.Request(url, headers={"User-Agent": "horizon-native-mod"})
        with urllib.request.urlopen(request, timeout=300) as response:
            with tarfile.open(fileobj=io.BytesIO(response.read()), mode="r:gz") as tar:
                tar.extractall(partial, filter="data")
        top = next(partial.iterdir())
    # horizon-mixins refuses links; the source has a few (docs), none needed to build.
    for path in top.rglob("*"):
        if path.is_symlink():
            path.unlink()
    (top / ".horizon-revision").write_text(revision)
    shutil.rmtree(out, ignore_errors=True)
    top.rename(out)
    shutil.rmtree(partial, ignore_errors=True)
    return out


def tool():
    """horizon-mixins at the pinned commit, built once."""
    rev = COMPAT["horizon_mixins_revision"]
    root = TARGET / "horizon-mixins" / rev
    binary = root / "bin" / "horizon-mixins"
    if not binary.is_file():
        log(f"Building horizon-mixins {rev[:7]}")
        subprocess.run(["cargo", "install", "--locked", "--quiet", "--git", COMPAT["horizon_mixins_repository"],
                        "--rev", rev, "--root", str(root), "horizon-mixins-cli"], check=True)
    return binary


HOOKS_LINE = re.compile(r"^pub\(crate\) const NATIVE_HOOKS_API: u32 = (\d+);$", re.M)


def others(paths):
    """The other native mods' bundles: (folder, id, hooks or None), unpacking .hnmod files into target/with/."""
    found = []
    unpacked = TARGET / "with"
    shutil.rmtree(unpacked, ignore_errors=True)
    for path in map(Path, paths):
        if path.suffix == ".hnmod":
            with zipfile.ZipFile(path) as archive:
                manifest = json.loads(archive.read("horizon-native.json"))
                out = unpacked / manifest["id"]
                for name in archive.namelist():
                    if name.startswith(f"mixins/{manifest['id']}/") and name.endswith((".rs", ".toml")):
                        target = out / Path(name).name
                        target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes(archive.read(name))
            found.append((out, manifest["id"], manifest.get("hooks")))
        elif (path / "horizon-mixin.toml").is_file():
            manifest = tomllib.loads((path / "horizon-mixin.toml").read_text())
            compat = next((p / "compatibility.json" for p in [path, *path.parents] if (p / "compatibility.json").is_file()),
                          None)
            hooks = json.loads(compat.read_text()).get("hooks") if compat else None
            found.append((path, manifest["mod"]["id"], hooks))
        elif path.is_dir():
            found += others(sorted(p for p in path.iterdir() if p.is_dir() or p.suffix == ".hnmod"))
    return found


def stage(horizon, extra=()):
    """The bundle (and `extra` mods) applied on Horizon: target/stage/ (only changed files are replaced)."""
    pinned = MANIFEST["compatibility"]["horizon_revision"]
    if pinned != COMPAT["horizon_revision"]:
        sys.exit(f"horizon-mixin.toml pins {pinned}, compatibility.json {COMPAT['horizon_revision']}")
    tree = source(horizon)
    bundles = TARGET / "mods"
    shutil.rmtree(bundles, ignore_errors=True)
    shutil.copytree(BUNDLE, bundles / ID)
    found = HOOKS_LINE.search((tree / "crates/horizon-client/src/game/native_hooks.rs").read_text())
    api = int(found.group(1)) if found else 0
    for folder, mod_id, hooks in others(extra):
        if mod_id == ID:
            continue
        # As the launcher: a mod on the hook points builds on any Horizon with its hook points, re-pinned to the
        # commit of the build; one pinned elsewhere doesn't.
        if hooks is None:
            text = (folder / "horizon-mixin.toml").read_text()
            if pinned not in text:
                sys.exit(f"{mod_id} pins another Horizon commit than {pinned[:7]}: they can't be built together")
        elif hooks > api:
            sys.exit(f"{mod_id} needs native hook points v{hooks}; Horizon {pinned[:7]} has v{api}")
        shutil.copytree(folder, bundles / mod_id, ignore=shutil.ignore_patterns("*.py", "target"))
        toml = bundles / mod_id / "horizon-mixin.toml"
        toml.write_text(re.sub(r'horizon_revision = "[0-9a-f]{40}"', f'horizon_revision = "{pinned}"',
                               toml.read_text()))
        log(f"With {mod_id}")
    fresh = (TARGET / "stage.new").resolve()
    shutil.rmtree(fresh, ignore_errors=True)
    subprocess.run([str(tool()), "apply", "--workspace", str(tree), "--mods", str(bundles), "--out", str(fresh)],
                   check=True, stdout=subprocess.DEVNULL)
    out = TARGET / "stage"
    out.mkdir(parents=True, exist_ok=True)
    subprocess.run(["rsync", "-a", "--checksum", "--delete", f"{fresh}/", f"{out}/"], check=True)
    shutil.rmtree(fresh)
    log(f"Staged {out}")
    return out


def cargo(stage_dir, action):
    command = ["cargo", action, "--locked", "-p", "horizon-client"]
    if action == "test":
        command += ["--bin", "horizon-client", "--", "distant_horizons::tests"]
    else:
        command += ["--bin", "horizon-client"]
    if action == "build":
        command += ["--bin", "horizon-ctl", "--profile", "fast"]
    # The WebAssembly mods' runtime, when this Horizon still has it: the launcher builds with it.
    features = tomllib.loads((stage_dir / "crates/horizon-client/Cargo.toml").read_text()).get("features", {})
    if "mods" in features:
        # Before the test binary's own arguments (`--`).
        at = command.index("--") if "--" in command else len(command)
        command[at:at] = ["--features", "horizon-client/mods"]
    jobs = os.environ.get("CARGO_BUILD_JOBS")
    if jobs:
        command[2:2] = ["-j", jobs]
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET / "cargo"))
    logfile = TARGET / f"horizon-{action}.log"
    log(f"cargo {action} (log: {logfile})")
    with open(logfile, "w") as out:
        result = subprocess.run(command, cwd=stage_dir, env=env, stdout=out, stderr=subprocess.STDOUT)
    text = logfile.read_text()
    ours = OURS.findall(text)
    if result.returncode != 0 or ours:
        sys.stderr.write(text[-12000:])
        sys.exit(f"cargo {action} failed" if result.returncode else f"{len(ours)} warnings in the mod's code")
    tail = [line for line in text.splitlines() if "Finished" in line or "test result" in line]
    log("\n".join(tail))
    return TARGET / "cargo" / "fast"


def bundle_files():
    """The .hnmod's files: horizon-native.json and the bundle's .rs and .toml files."""
    manifest = {"schema": 1, "id": ID, "version": VERSION, "game": COMPAT["game"],
                "horizon": COMPAT["horizon_revision"], "mixins": COMPAT["horizon_mixins_revision"]}
    # No `hooks`: the mod reads more than the native hook points, so the launcher must build it on the commit it
    # pins, not on whichever Horizon the player has.
    files = {"horizon-native.json": (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()}
    for path in sorted(BUNDLE.iterdir()):
        if path.suffix in (".rs", ".toml"):
            files[f"mixins/{ID}/{path.name}"] = path.read_bytes()
    return files


def package():
    """A deterministic .hnmod of `bundle_files`."""
    files = bundle_files()
    out = TARGET / f"{ID}-{VERSION}.hnmod"
    TARGET.mkdir(exist_ok=True)
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, content in sorted(files.items()):
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, content)
    print(json.dumps({"file": str(out), "size": out.stat().st_size,
                      "sha256": hashlib.sha256(out.read_bytes()).hexdigest()}, indent=2))


def native_set(extra):
    """target/native-set/: this mod, unpacked as an .hnmod, and the other mods (`--with`: .hnmod files, bundle
    folders, or folders of them) beside it. `horizon-launcher native-build FOLDER` builds exactly these: an
    instance's own build takes only the native mods its Mods page installed, never one copied in by hand."""
    out = TARGET / "native-set"
    shutil.rmtree(out, ignore_errors=True)
    for name, content in bundle_files().items():
        (out / ID / name).parent.mkdir(parents=True, exist_ok=True)
        (out / ID / name).write_bytes(content)
    def hnmod(path):
        with zipfile.ZipFile(path) as archive:
            mod_id = json.loads(archive.read("horizon-native.json"))["id"]
            if mod_id != ID:
                # Whole, horizon-native.json included: its `hooks` lets the launcher build it on this mod's commit.
                archive.extractall(out / mod_id)
                log(f"With {mod_id}")

    def walk(path):
        if path.suffix == ".hnmod":
            hnmod(path)
        elif (path / "horizon-mixin.toml").is_file():
            folder, mod_id, hooks = others([path])[0]
            if mod_id == ID:
                return
            manifest = tomllib.loads((folder / "horizon-mixin.toml").read_text())
            compat = next((p / "compatibility.json" for p in [folder, *folder.parents]
                           if (p / "compatibility.json").is_file()), None)
            compat = json.loads(compat.read_text()) if compat else {}
            native = {"schema": 1, "id": mod_id, "version": manifest["mod"]["version"], "game": "26.3",
                      "horizon": manifest["compatibility"]["horizon_revision"],
                      "mixins": compat.get("horizon_mixins_revision", COMPAT["horizon_mixins_revision"])}
            if hooks is not None:
                native["hooks"] = hooks
            shutil.copytree(folder, out / mod_id / "mixins" / mod_id, ignore=shutil.ignore_patterns("*.py", "target"))
            (out / mod_id / "horizon-native.json").write_text(json.dumps(native, indent=2, sort_keys=True) + "\n")
            log(f"With {mod_id}")
        elif path.is_dir():
            for child in sorted(path.iterdir()):
                if child.is_dir() or child.suffix == ".hnmod":
                    walk(child)

    for path in extra:
        walk(Path(path).expanduser())
    print(out)
    print("Build it (it runs these mods' code with your rights: read them first):")
    print(f'  horizon-launcher native-build "{out}" --instance native-mods --version <the installed Nightly of '
          f'{COMPAT["horizon_revision"][:7]}> --trust-native-code')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("action", choices=["stage", "check", "test", "build", "package", "set"])
    parser.add_argument("--horizon", help="a Horizon git checkout holding the pinned commit")
    parser.add_argument("--with", dest="extra", action="append", default=[], help="another native mod to build in")
    args = parser.parse_args()
    if args.action == "package":
        package()
        return
    if args.action == "set":
        native_set(args.extra)
        return
    staged = stage(args.horizon, args.extra)
    if args.action == "stage":
        print(staged)
    elif args.action == "build":
        print(cargo(staged, "build") / "horizon-client")
    else:
        cargo(staged, args.action)


if __name__ == "__main__":
    main()
