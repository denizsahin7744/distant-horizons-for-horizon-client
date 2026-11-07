# Changelog

## Unreleased

- **Distant Horizons servers** (3.3.4, protocol 16), client side: LODs requested around the player by distance,
  decoded, drawn; real-time block updates; server generation past visited areas.
- **Paper with Distant Horizons Support**: 0.15 snapshots (protocol 16) and 0.14 (protocol 15, used after the
  server's "Client is outdated"; its generation field is a boolean). Reads its unfinished LZMA2 streams and
  int-length close reasons. Before: "no answer".
- **Server LODs on disk** per server and level under the game's mod data; rejoin draws from disk and fetches only
  changes (sync on load, by timestamp).
- **All compressions in plain Rust**: none, LZ4 frames, Zstandard (RFC 8878), LZMA2 (.xz), tested against the
  reference compressors.
- **Server settings** (page and `/dh`, applied live): Server LODs, Real-time updates, Sync on load, Server
  bandwidth (default 500 KB/s), Server folder (Distant Horizons' five naming modes). Distant generation also
  controls server generation.
- `/dh` shows server state: received, up to date, waiting, asked, failed.
- RETURN hook on `net::Session::poll` moves the server's messages into an inbox module in Horizon's library.
- Fix: with any network delay, the first messages were sent during configuration (Horizon drops them) and never
  again, so every remote server showed "no answer". They now wait for the play login; the session config is
  re-sent (5 s, doubling to 1 min) until answered.
- Tools: `tools/horizon.py interop` and `tools/server_proof.py` against a live server.

## 0.1.0 (2026-10-30)

First release, experimental.

- Distant Horizons' LOD core in Rust: section positions and full data points (same bit layouts), chunk-to-LOD
  building, 2 x 2 quadtree merging to 16,384-block sections, hidden-run culling, distance-based detail with five
  horizontal-quality presets, render columns with cave culling, column-box meshing with neighbour-aware sides and
  face merging.
- Distant generation from Horizon's world generator (single player, LAN host).
- LODs saved per world and dimension.
- Drawn by Horizon's far-terrain renderer with block textures and biome tints; the game's own chunks cut out.
- Settings page (Options > Mods), `/dh` commands with Tab completion, debug view colouring detail levels.
- Builds with Horizon's official native mods; WebAssembly mods run beside it.
- Release zip with installer for macOS and Linux (`install.command`, `install.sh`), building via the Horizon
  launcher.
- Pinned to Horizon `d009ebd` (Nightly 2026-10-29) and horizon-mixins `0d6b103`.
