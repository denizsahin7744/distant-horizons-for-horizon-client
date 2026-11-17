# Changelog

## Unreleased

- LODs from servers running Distant Horizons: the client side of its networking (protocol 16, the 3.3 releases)
  on the `distant_horizons:msg` plugin channel. The session config and level key handshake, full data requests
  for the sections in view (nearest first, within the server's rate and distance limits, split when the server
  asks, re-asked for what it has when it generates nothing), split transfers, real-time partial updates.
- Distant Horizons' network data read as it writes it (`FullDataSourceV2DTO`, both formats), with Zstandard and
  LZ4 frame decoders of the mod's own; block states and biomes matched to Horizon's by name, Nether and End
  heights moved to Horizon's.
- A "Server LODs" setting and `/dh server on|off`; `/dh` shows the server's status and what it sent.

## 0.1.0 (2026-10-30)

First release, experimental.

- Distant Horizons' level-of-detail core ported to Rust: section positions and full data points (same bit layouts),
  chunk-to-LOD building, 2 x 2 merging up a quadtree to 16,384-block sections, hidden-run culling, distance-based
  detail with the five horizontal-quality presets, render columns with cave culling, column-box meshing with
  neighbour-aware side faces and face merging.
- Distant generation from Horizon's world generator in single player and on LAN hosts.
- LODs kept on disk per world and dimension.
- Drawn by Horizon's far-terrain renderer with block textures and biome tints; the game's own chunks cut out.
- Settings page (Options > Mods) and `/dh` commands, completed by Tab in chat; a debug view colouring each
  detail level.
- Builds together with Horizon's official native mods; WebAssembly mods run beside it.
- A release zip with an installer for macOS and Linux (`install.command`, `install.sh`) that builds it through
  the Horizon launcher.
- Pinned to Horizon `d009ebd` (Nightly 2026-10-29) and horizon-mixins `0d6b103`.
