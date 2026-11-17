// Distant Horizons for Horizon Client: the level-of-detail core of the
// Distant Horizons mod (https://gitlab.com/distant-horizons-team/distant-horizons,
// LGPL-3.0, Copyright (C) 2020 James Seibel and contributors), ported to
// Rust. This file is both the mod's Cargo library (its tests run with
// `cargo test`) and the module the bundle adds to Horizon's game as
// `game::distant_horizons`. It knows nothing of Horizon: chunks, the world
// generator and meshing reach it through the `ChunkView`, `ColumnSampler`
// and `Host` traits, which `bridge.rs` implements on Horizon's types.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU Lesser General Public License as published by
// the Free Software Foundation, version 3. It is distributed WITHOUT ANY
// WARRANTY; see LICENSE for the full terms.
//
// The Java class each part ports is named on it. Where the port differs from
// Distant Horizons, the comment there says how and why.

/// The mod's id (its settings folder), name and version.
pub const ID: &str = "distant_horizons";
pub const NAME: &str = "Distant Horizons";
pub const VERSION: &str = "0.1.0";
pub const DESCRIPTION: &str = "Distant Horizons' level-of-detail terrain, ported to Rust: the world past your render distance, built from the chunks you visit and the world generator, kept on disk and drawn at lower detail the farther it is.";

/// `DhSectionPos`: a quadtree node, packed in a long as Distant Horizons
/// does (detail level in bits 0-7, x in 8-35, z in 36-63). A section at
/// detail level `d` is `2^d` blocks wide and holds 64 x 64 data columns of
/// `2^(d-6)` blocks each.
#[allow(dead_code)]
pub mod pos {
    pub const SECTION_MINIMUM_DETAIL_LEVEL: u8 = 6;
    /// `SECTION_MINIMUM_DETAIL_LEVEL + LodUtil.BLOCK_DETAIL_LEVEL`.
    pub const SECTION_BLOCK_DETAIL_LEVEL: u8 = 6;
    /// `FullDataSourceProviderV2.ROOT_SECTION_DETAIL_LEVEL`: 16384 blocks.
    pub const ROOT_DETAIL_LEVEL: u8 = SECTION_MINIMUM_DETAIL_LEVEL + 8;
    /// `FullDataSourceV2.WIDTH`: data columns per side.
    pub const WIDTH: i32 = 64;

    const DETAIL_LEVEL_MASK: i64 = i8::MAX as i64;
    const POS_X_OFFSET: u32 = 8;
    const POS_Z_OFFSET: u32 = 8 + 28;
    const POS_MASK: i32 = (1 << 28) - 1;
    const POS_MISSING_WIDTH: u32 = 32 - 28;

    pub fn encode(detail_level: u8, x: i32, z: i32) -> i64 {
        let mut data = 0i64;
        data |= i64::from(detail_level) & DETAIL_LEVEL_MASK;
        data |= i64::from(x & POS_MASK) << POS_X_OFFSET;
        data |= i64::from(z & POS_MASK) << POS_Z_OFFSET;
        data
    }
    pub fn detail_level(pos: i64) -> u8 {
        (pos & DETAIL_LEVEL_MASK) as u8
    }
    pub fn x(pos: i64) -> i32 {
        let x = ((pos >> POS_X_OFFSET) as i32) & POS_MASK;
        (x << POS_MISSING_WIDTH) >> POS_MISSING_WIDTH
    }
    pub fn z(pos: i64) -> i32 {
        let z = ((pos >> POS_Z_OFFSET) as i32) & POS_MASK;
        (z << POS_MISSING_WIDTH) >> POS_MISSING_WIDTH
    }
    /// The section at `detail_level` holding block column `(x, z)`
    /// (`encodeContaining`).
    pub fn containing_block(detail_level: u8, x: i32, z: i32) -> i64 {
        let block = encode(SECTION_BLOCK_DETAIL_LEVEL, x.div_euclid(WIDTH), z.div_euclid(WIDTH));
        convert_to_detail_level(block, detail_level)
    }
    pub fn convert_to_detail_level(pos: i64, new_detail_level: u8) -> i64 {
        let detail = detail_level(pos);
        let (mut x, mut z) = (x(pos), z(pos));
        if new_detail_level >= detail {
            let scale = 1 << (new_detail_level - detail);
            x = x.div_euclid(scale);
            z = z.div_euclid(scale);
        } else {
            let scale = 1 << (detail - new_detail_level);
            x *= scale;
            z *= scale;
        }
        encode(new_detail_level, x, z)
    }
    pub fn block_width(pos: i64) -> i32 {
        1 << detail_level(pos)
    }
    /// Blocks per data column.
    pub fn data_width(pos: i64) -> i32 {
        1 << (detail_level(pos) - SECTION_MINIMUM_DETAIL_LEVEL)
    }
    fn center_block(pos: i64, coordinate: i32) -> i32 {
        let detail = detail_level(pos);
        if detail == 0 {
            return coordinate;
        }
        let offset = if detail != 1 { 1 << (detail - 1) } else { 0 };
        coordinate * (1 << detail) + offset
    }
    pub fn center_block_x(pos: i64) -> i32 {
        center_block(pos, x(pos))
    }
    pub fn center_block_z(pos: i64) -> i32 {
        center_block(pos, z(pos))
    }
    pub fn min_corner_block_x(pos: i64) -> i32 {
        let half = if detail_level(pos) != 1 {
            block_width(pos) / 2
        } else {
            0
        };
        center_block_x(pos) - half
    }
    pub fn min_corner_block_z(pos: i64) -> i32 {
        let half = if detail_level(pos) != 1 {
            block_width(pos) / 2
        } else {
            0
        };
        center_block_z(pos) - half
    }
    pub fn parent(pos: i64) -> i64 {
        encode(detail_level(pos) + 1, x(pos) >> 1, z(pos) >> 1)
    }
    /// Child 0 to 3: bit 0 is +x, bit 1 is +z (`getChildByIndex`).
    pub fn child(pos: i64, index: usize) -> i64 {
        let index = index as i32;
        encode(
            detail_level(pos) - 1,
            x(pos) * 2 + (index & 1),
            z(pos) * 2 + ((index & 2) >> 1),
        )
    }
    pub fn adjacent(pos: i64, dx: i32, dz: i32) -> i64 {
        encode(detail_level(pos), x(pos) + dx, z(pos) + dz)
    }
    /// Whether `a` covers `b`'s minimum corner (`contains`).
    pub fn contains(a: i64, b: i64) -> bool {
        let (ax, az) = (min_corner_block_x(a), min_corner_block_z(a));
        let (bx, bz) = (min_corner_block_x(b), min_corner_block_z(b));
        let width = block_width(a) - 1;
        ax <= bx && bx <= ax + width && az <= bz && bz <= az + width
    }
    /// `getChebyshevSignedBlockDistance`: negative inside the section.
    pub fn chebyshev_signed_block_distance(pos: i64, x: i32, z: i32) -> i32 {
        (center_block_x(pos) - x).abs().max((center_block_z(pos) - z).abs()) - block_width(pos) / 2
    }
    pub fn to_string(pos: i64) -> String {
        format!("{}*{},{}", detail_level(pos), x(pos), z(pos))
    }
}

/// `FullDataPointUtil`: one vertical run of a full data column in a long.
/// Bits 0-31 the id (in the source's `IdMap`), 32-43 the height, 44-55 the
/// bottom relative to the world's minimum y, 56-59 sky light, 60-63 block
/// light.
#[allow(dead_code)]
pub mod full_point {
    pub const EMPTY_DATA_POINT: u64 = 0;
    const ID_MASK: u64 = i32::MAX as u64;
    const HEIGHT_OFFSET: u32 = 32;
    const MIN_Y_OFFSET: u32 = 44;
    const SKY_LIGHT_OFFSET: u32 = 56;
    const BLOCK_LIGHT_OFFSET: u32 = 60;
    const HEIGHT_MASK: u64 = (1 << 12) - 1;
    const MIN_Y_MASK: u64 = (1 << 12) - 1;
    const LIGHT_MASK: u64 = 15;
    /// `RenderDataPointUtil.MAX_WORLD_Y_SIZE`.
    pub const MAX_WORLD_Y_SIZE: i32 = 2031 + 4064;

    /// `encode` with `validateData`: Distant Horizons throws
    /// `DataCorruptedException`, this returns `None`.
    pub fn encode(id: u32, height: i32, rel_min_y: i32, block_light: u8, sky_light: u8) -> Option<u64> {
        let valid = id <= i32::MAX as u32
            && (0..MAX_WORLD_Y_SIZE).contains(&rel_min_y)
            && height > 0
            && height < MAX_WORLD_Y_SIZE
            && rel_min_y + height <= MAX_WORLD_Y_SIZE
            && block_light <= 15
            && sky_light <= 15;
        valid.then(|| {
            (u64::from(id) & ID_MASK)
                | ((height as u64 & HEIGHT_MASK) << HEIGHT_OFFSET)
                | ((rel_min_y as u64 & MIN_Y_MASK) << MIN_Y_OFFSET)
                | (u64::from(block_light) << BLOCK_LIGHT_OFFSET)
                | (u64::from(sky_light) << SKY_LIGHT_OFFSET)
        })
    }
    pub fn id(data: u64) -> u32 {
        (data & ID_MASK) as u32
    }
    pub fn height(data: u64) -> i32 {
        ((data >> HEIGHT_OFFSET) & HEIGHT_MASK) as i32
    }
    pub fn bottom_y(data: u64) -> i32 {
        ((data >> MIN_Y_OFFSET) & MIN_Y_MASK) as i32
    }
    pub fn top_y(data: u64) -> i32 {
        bottom_y(data) + height(data)
    }
    pub fn block_light(data: u64) -> u8 {
        ((data >> BLOCK_LIGHT_OFFSET) & LIGHT_MASK) as u8
    }
    pub fn sky_light(data: u64) -> u8 {
        ((data >> SKY_LIGHT_OFFSET) & LIGHT_MASK) as u8
    }
    pub fn set_id(data: u64, id: u32) -> u64 {
        (data & !ID_MASK) | u64::from(id)
    }
    pub fn set_height(data: u64, height: i32) -> u64 {
        (data & !(HEIGHT_MASK << HEIGHT_OFFSET)) | ((height as u64 & HEIGHT_MASK) << HEIGHT_OFFSET)
    }
    pub fn set_bottom_y(data: u64, bottom: i32) -> u64 {
        (data & !(MIN_Y_MASK << MIN_Y_OFFSET)) | ((bottom as u64 & MIN_Y_MASK) << MIN_Y_OFFSET)
    }
}

/// `EDhApiWorldGenerationStep`: how complete a column's data is. Newer data
/// replaces older only when it is at least as complete.
#[allow(dead_code)]
pub mod gen_step {
    pub const DOWN_SAMPLED: i8 = -1;
    pub const EMPTY: i8 = 0;
    pub const SURFACE: i8 = 5;
    pub const FEATURES: i8 = 8;
    pub const LIGHT: i8 = 9;
}

/// `EDhApiBlockMaterial`, the render points' 4-bit material.
#[allow(dead_code)]
pub mod material {
    pub const UNKNOWN: u8 = 0;
    pub const LEAVES: u8 = 1;
    pub const STONE: u8 = 2;
    pub const LAVA: u8 = 6;
    pub const SNOW: u8 = 8;
    pub const SAND: u8 = 9;
    pub const WATER: u8 = 12;
    pub const GRASS: u8 = 13;
    pub const AIR: u8 = 14;
    pub const ILLUMINATED: u8 = 15;
}

/// What the LOD code asks of a block state (`IBlockStateWrapper`), read
/// from the game once per state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockInfo {
    pub air: bool,
    /// 0 to 15, 16 for fully opaque (`LodUtil.BLOCK_FULLY_OPAQUE`).
    pub opacity: u8,
    /// Has a collision shape.
    pub solid: bool,
    pub liquid: bool,
    pub material: u8,
    /// 255, or less for a see-through block (water, ice, glass).
    pub alpha: u8,
    /// A snow layer: drawn as the colour of the block under it.
    pub snow_layer: bool,
    /// Frozen water: water directly under it is drawn as ice.
    pub ice: bool,
}

/// Every block state's `BlockInfo`, by state id. Unknown ids are air.
#[derive(Clone, Debug, Default)]
pub struct BlockTable {
    pub states: Vec<BlockInfo>,
}

impl BlockTable {
    pub fn get(&self, state: u32) -> BlockInfo {
        self.states.get(state as usize).copied().unwrap_or(BlockInfo {
            air: true,
            material: material::AIR,
            ..BlockInfo::default()
        })
    }
}

/// `FullDataPointIdMap`: the (biome, block state) pairs a data source uses;
/// data points name them by index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdMap {
    entries: Vec<(u16, u32)>,
    index: std::collections::HashMap<(u16, u32), u32>,
}

#[allow(dead_code)]
impl IdMap {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn add_if_not_present(&mut self, biome: u16, state: u32) -> u32 {
        if let Some(&id) = self.index.get(&(biome, state)) {
            return id;
        }
        let id = self.entries.len() as u32;
        self.entries.push((biome, state));
        self.index.insert((biome, state), id);
        id
    }
    /// The (biome, block state) of `id`; air in biome 0 for an id out of
    /// range (Distant Horizons logs it once and skips the point).
    pub fn get(&self, id: u32) -> (u16, u32) {
        self.entries.get(id as usize).copied().unwrap_or((0, 0))
    }
    pub fn state(&self, id: u32) -> u32 {
        self.get(id).1
    }
    /// `mergeAndReturnRemappedEntityIds`: adds `other`'s entries, and gives
    /// the id each of its ids has here.
    pub fn merge_and_remap(&mut self, other: &IdMap) -> Vec<u32> {
        other
            .entries
            .iter()
            .map(|&(biome, state)| self.add_if_not_present(biome, state))
            .collect()
    }
    pub fn entries(&self) -> &[(u16, u32)] {
        &self.entries
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
    }
}

/// `FullDataSourceV2`: a section's 64 x 64 full data columns. Each column
/// lists its runs from the top down (index 0 is the highest), as Distant
/// Horizons keeps them.
#[derive(Clone, Debug, PartialEq)]
pub struct FullDataSource {
    pub pos: i64,
    pub mapping: IdMap,
    /// By `index(x, z)`; an empty column has no data.
    pub columns: Vec<Vec<u64>>,
    /// Each column's `gen_step`.
    pub gen_steps: Vec<i8>,
    pub is_empty: bool,
}

/// `FullDataSourceV2.relativePosToIndex`.
pub fn index(x: i32, z: i32) -> usize {
    debug_assert!((0..pos::WIDTH).contains(&x) && (0..pos::WIDTH).contains(&z));
    (x * pos::WIDTH + z) as usize
}

const COLUMNS: usize = (pos::WIDTH * pos::WIDTH) as usize;

#[allow(dead_code)]
impl FullDataSource {
    pub fn empty(pos: i64) -> Self {
        Self {
            pos,
            mapping: IdMap::default(),
            columns: vec![Vec::new(); COLUMNS],
            gen_steps: vec![gen_step::EMPTY; COLUMNS],
            is_empty: true,
        }
    }
    pub fn column(&self, x: i32, z: i32) -> &[u64] {
        &self.columns[index(x, z)]
    }
    fn try_column(&self, x: i32, z: i32) -> Option<&Vec<u64>> {
        ((0..pos::WIDTH).contains(&x) && (0..pos::WIDTH).contains(&z)).then(|| &self.columns[index(x, z)])
    }
    pub fn set_column(&mut self, x: i32, z: i32, column: Vec<u64>, step: i8) {
        let i = index(x, z);
        self.columns[i] = column;
        self.gen_steps[i] = step;
        self.is_empty = false;
    }
    /// Columns with no data yet (`EMPTY`), which the generator may fill.
    pub fn missing_columns(&self) -> usize {
        self.gen_steps.iter().filter(|&&s| s == gen_step::EMPTY).count()
    }

    /// `updateFromDataSource`: merges `input`, of this section, one of its
    /// children or its parent. Returns whether this source changed.
    pub fn update_from(&mut self, input: &FullDataSource, blocks: &BlockTable) -> bool {
        if input.mapping.is_empty() {
            return false;
        }
        let this_detail = pos::detail_level(self.pos);
        let input_detail = pos::detail_level(input.pos);
        let remapped = self.mapping.merge_and_remap(&input.mapping);
        let changed: Vec<usize> = if input_detail == this_detail {
            self.update_from_same_detail_level(input, &remapped)
        } else if input_detail + 1 == this_detail {
            self.update_from_one_below_detail_level(input, &remapped, blocks)
        } else if input_detail == this_detail + 1 {
            self.downsample_from_one_above_detail_level(input, &remapped)
        } else {
            panic!(
                "unsupported data source update: {} into {}",
                pos::to_string(input.pos),
                pos::to_string(self.pos)
            );
        };
        if changed.is_empty() {
            return false;
        }
        self.remove_unused_ids_and_remap();
        // `EDhApiWorldCompressionMode.VISUALLY_EQUAL`, the default: hidden
        // runs are merged into the run above them. Distant Horizons culls
        // every column; only the changed ones and their neighbours can change.
        let mut culled = vec![false; COLUMNS];
        for i in changed {
            let (x, z) = ((i as i32) / pos::WIDTH, (i as i32) % pos::WIDTH);
            for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if !(0..pos::WIDTH).contains(&nx) || !(0..pos::WIDTH).contains(&nz) || culled[index(nx, nz)] {
                    continue;
                }
                culled[index(nx, nz)] = true;
                if self.column(nx, nz).len() > 1 {
                    cull_hidden_data_points(self, nx, nz, blocks);
                }
            }
        }
        true
    }

    fn update_from_same_detail_level(&mut self, input: &FullDataSource, remapped: &[u32]) -> Vec<usize> {
        let mut changed = Vec::new();
        for i in 0..COLUMNS {
            let this_step = self.gen_steps[i];
            let input_step = input.gen_steps[i];
            // Down-sampled input only replaces empty or down-sampled data;
            // anything else replaces data that is no more complete.
            let allowed = if input_step == gen_step::DOWN_SAMPLED {
                this_step == gen_step::EMPTY || this_step == gen_step::DOWN_SAMPLED
            } else {
                input_step != gen_step::EMPTY && this_step <= input_step
            };
            if !allowed {
                continue;
            }
            let column: Vec<u64> = input.columns[i]
                .iter()
                .map(|&p| full_point::set_id(p, remapped[full_point::id(p) as usize]))
                .collect();
            if self.columns[i] != column || self.gen_steps[i] != input_step {
                changed.push(i);
            }
            self.columns[i] = column;
            self.gen_steps[i] = input_step;
            self.is_empty = false;
        }
        changed
    }

    /// `updateFromOneBelowDetailLevel`: each 2 x 2 of the child's columns
    /// becomes one column here.
    ///
    /// Unlike Distant Horizons, a 2 x 2 with any empty column leaves the
    /// column here as it is: this port generates far sections at their own
    /// detail level, so a parent often has data its children lack, and a
    /// single visited chunk must not blank the generated terrain around it.
    fn update_from_one_below_detail_level(
        &mut self,
        input: &FullDataSource,
        remapped: &[u32],
        blocks: &BlockTable,
    ) -> Vec<usize> {
        let half = pos::WIDTH / 2;
        let first = pos::child(self.pos, 0);
        let offset_x = if pos::x(input.pos) == pos::x(first) { 0 } else { half };
        let offset_z = if pos::z(input.pos) == pos::z(first) { 0 } else { half };
        let mut changed = Vec::new();
        for x in (0..pos::WIDTH).step_by(2) {
            for z in (0..pos::WIDTH).step_by(2) {
                let step = (0..4)
                    .map(|i| input.gen_steps[index(x + (i & 1), z + (i >> 1))])
                    .min()
                    .unwrap_or(gen_step::EMPTY);
                if step == gen_step::EMPTY {
                    continue;
                }
                let recipient = index(x / 2 + offset_x, z / 2 + offset_z);
                let merged: Vec<u64> = merge_two_by_two_column(input, x, z, blocks)
                    .into_iter()
                    .map(|p| full_point::set_id(p, remapped[full_point::id(p) as usize]))
                    .collect();
                if self.columns[recipient] != merged || self.gen_steps[recipient] != step {
                    changed.push(recipient);
                }
                self.columns[recipient] = merged;
                self.gen_steps[recipient] = step;
                self.is_empty = false;
            }
        }
        changed
    }

    /// `downsampleFromOneAboveDetailLevel`: fills only the columns that have
    /// no data, so higher-detail data is never covered.
    fn downsample_from_one_above_detail_level(&mut self, input: &FullDataSource, remapped: &[u32]) -> Vec<usize> {
        let half = pos::WIDTH / 2;
        let first = pos::child(input.pos, 0);
        let offset_x = if pos::x(self.pos) == pos::x(first) { 0 } else { half };
        let offset_z = if pos::z(self.pos) == pos::z(first) { 0 } else { half };
        let mut changed = Vec::new();
        for x in 0..pos::WIDTH {
            for z in 0..pos::WIDTH {
                let recipient = index(x, z);
                let has_data = self.columns[recipient]
                    .iter()
                    .any(|&p| p != full_point::EMPTY_DATA_POINT);
                if has_data {
                    continue;
                }
                let source = &input.columns[index(x / 2 + offset_x, z / 2 + offset_z)];
                self.columns[recipient] = source
                    .iter()
                    .map(|&p| full_point::set_id(p, remapped[full_point::id(p) as usize]))
                    .collect();
                self.gen_steps[recipient] = gen_step::DOWN_SAMPLED;
                self.is_empty = false;
                changed.push(recipient);
            }
        }
        changed
    }

    /// `removeUnusedIdsAndRemap`: keeps the map to the ids in use.
    fn remove_unused_ids_and_remap(&mut self) {
        let mut fresh = IdMap::default();
        let mut new_by_old = std::collections::HashMap::new();
        for column in &self.columns {
            for &p in column {
                let old = full_point::id(p);
                new_by_old.entry(old).or_insert_with(|| {
                    let (biome, state) = self.mapping.get(old);
                    fresh.add_if_not_present(biome, state)
                });
            }
        }
        for column in &mut self.columns {
            for p in column.iter_mut() {
                *p = full_point::set_id(*p, new_by_old[&full_point::id(*p)]);
            }
        }
        self.mapping = fresh;
    }
}

/// `FullDataSourceV2.mergeInputTwoByTwoDataColumn`: the 2 x 2 columns at
/// `(x, z)`, sliced at every height where one of them changes; each slice
/// takes the most common non-air block, and the light of the columns that
/// have that block.
fn merge_two_by_two_column(input: &FullDataSource, x: i32, z: i32, blocks: &BlockTable) -> Vec<u64> {
    let columns: [&[u64]; 4] = [
        input.column(x, z),
        input.column(x, z + 1),
        input.column(x + 1, z),
        input.column(x + 1, z + 1),
    ];
    let mut transitions: Vec<i32> = Vec::new();
    for column in columns {
        for &p in column {
            transitions.push(full_point::bottom_y(p));
            transitions.push(full_point::top_y(p));
        }
    }
    if transitions.is_empty() {
        return Vec::new();
    }
    transitions.sort_unstable();
    transitions.dedup();
    // From each column's bottom-most run up.
    let mut current: [isize; 4] = columns.map(|c| c.len() as isize - 1);
    let mut out = Vec::new();
    let (mut last_id, mut last_block, mut last_sky) = (0u32, 0u8, 0u8);
    let mut current_min_y = transitions[0];
    let mut accumulated = 0;
    for pair in transitions.windows(2) {
        let (slice_min, slice_max) = (pair[0], pair[1]);
        let slice_height = slice_max - slice_min;
        let sample_y = slice_min + slice_height / 2;
        let mut ids = [0u32; 4];
        let mut block_lights = [0u8; 4];
        let mut sky_lights = [0u8; 4];
        for i in 0..4 {
            while current[i] >= 0 {
                let p = columns[i][current[i] as usize];
                if sample_y >= full_point::top_y(p) {
                    current[i] -= 1;
                } else {
                    if sample_y >= full_point::bottom_y(p) {
                        ids[i] = full_point::id(p);
                        block_lights[i] = full_point::block_light(p);
                        sky_lights[i] = full_point::sky_light(p);
                    }
                    break;
                }
            }
        }
        let id = most_common_in_slice(ids, &input.mapping, blocks);
        let block_light = average_in_slice_with_id(block_lights, ids, id);
        let sky_light = average_in_slice_with_id(sky_lights, ids, id);
        if accumulated == 0 {
            (last_id, last_block, last_sky) = (id, block_light, sky_light);
            current_min_y = slice_min;
            accumulated = slice_height;
        } else if id != last_id || block_light != last_block || sky_light != last_sky {
            out.extend(full_point::encode(
                last_id,
                accumulated,
                current_min_y,
                last_block,
                last_sky,
            ));
            (last_id, last_block, last_sky) = (id, block_light, sky_light);
            current_min_y = slice_min;
            accumulated = slice_height;
        } else {
            accumulated += slice_height;
        }
    }
    if accumulated > 0 {
        out.extend(full_point::encode(
            last_id,
            accumulated,
            current_min_y,
            last_block,
            last_sky,
        ));
    }
    // Built from the bottom up; columns are kept from the top down.
    out.reverse();
    out
}

/// `determineMostCommonValueInColumnSlice`: air never wins (it would hole
/// hollow structures); ties go to the first column.
fn most_common_in_slice(ids: [u32; 4], mapping: &IdMap, blocks: &BlockTable) -> u32 {
    let mut counts = [0; 4];
    for &id in &ids {
        if blocks.get(mapping.state(id)).air {
            continue;
        }
        let slot = if id == ids[0] {
            0
        } else if id == ids[1] {
            1
        } else if id == ids[2] {
            2
        } else {
            3
        };
        counts[slot] += 1;
    }
    let max = *counts.iter().max().unwrap_or(&0);
    let winner = counts.iter().position(|&c| c == max).unwrap_or(0);
    ids[winner]
}

/// `determineAverageValueInColumnSliceWithId`: the light of the columns with
/// the winning block, so surface water doesn't light the sand below it.
fn average_in_slice_with_id(values: [u8; 4], ids: [u32; 4], winner: u32) -> u8 {
    let (mut sum, mut count) = (0u32, 0u32);
    for i in 0..4 {
        if ids[i] == winner {
            sum += u32::from(values[i]);
            count += 1;
        }
    }
    // With no column of the winner's, all four (smoother light).
    sum.checked_div(count)
        .unwrap_or_else(|| values.iter().map(|&v| u32::from(v)).sum::<u32>() / 4) as u8
}

/// `FullDataOcclusionCuller.cullHiddenDatapointsInColumn`: a run with opaque
/// runs above, below and on all four sides over its whole height is merged
/// into the run above it.
fn cull_hidden_data_points(source: &mut FullDataSource, x: i32, z: i32, blocks: &BlockTable) {
    let translucent =
        |source: &FullDataSource, p: u64| blocks.get(source.mapping.state(full_point::id(p))).opacity < 16;
    let neighbors: [Vec<u64>; 4] = match [
        source.try_column(x + 1, z),
        source.try_column(x - 1, z),
        source.try_column(x, z + 1),
        source.try_column(x, z - 1),
    ] {
        [Some(a), Some(b), Some(c), Some(d)] if !a.is_empty() && !b.is_empty() && !c.is_empty() && !d.is_empty() => {
            [a.clone(), b.clone(), c.clone(), d.clone()]
        }
        _ => return,
    };
    let mut center = std::mem::take(&mut source.columns[index(x, z)]);
    let mut adjacent: [isize; 4] = neighbors.each_ref().map(|c| c.len() as isize - 1);
    let mut i = center.len() as isize - 1;
    'points: while i >= 0 {
        let at = i as usize;
        let current = center[at];
        let skip = translucent(source, current)
            || at == 0
            || translucent(source, center[at - 1])
            || (at + 1 < center.len() && translucent(source, center[at + 1]))
            || at + 1 == center.len();
        if skip {
            i -= 1;
            continue;
        }
        for side in 0..4 {
            let (covered, next) = check_occlusion(source, current, &neighbors[side], adjacent[side], &translucent);
            adjacent[side] = next;
            if !covered {
                i -= 1;
                continue 'points;
            }
        }
        center.remove(at);
        let above = center[at - 1];
        let above = full_point::set_bottom_y(above, full_point::bottom_y(current));
        let above = full_point::set_height(above, full_point::height(current) + full_point::height(above));
        center[at - 1] = above;
        i -= 1;
    }
    source.columns[index(x, z)] = center;
}

/// `FullDataOcclusionCuller.checkOcclusion`: whether opaque runs of
/// `adjacent` cover `center`'s whole height, and where to continue from.
fn check_occlusion(
    source: &FullDataSource,
    center: u64,
    adjacent: &[u64],
    mut at: isize,
    translucent: &impl Fn(&FullDataSource, u64) -> bool,
) -> (bool, isize) {
    if adjacent.is_empty() || (adjacent.len() == 1 && adjacent[0] == full_point::EMPTY_DATA_POINT) {
        return (false, at);
    }
    let bottom = full_point::bottom_y(center);
    let top = full_point::top_y(center);
    while at >= 0 {
        let p = adjacent[at as usize];
        let adjacent_top = full_point::top_y(p);
        if adjacent_top <= bottom {
            at -= 1;
            continue;
        } else if translucent(source, p) {
            return (false, at);
        } else if adjacent_top >= top {
            return (true, at);
        }
        at -= 1;
    }
    (false, at.max(0))
}

/// A chunk as `LodDataBuilder` reads it (`IChunkWrapper`).
pub trait ChunkView: Send {
    fn chunk_x(&self) -> i32;
    fn chunk_z(&self) -> i32;
    /// `getInclusiveMinBuildHeight`.
    fn min_build_height(&self) -> i32;
    /// `getExclusiveMaxBuildHeight`.
    fn max_build_height(&self) -> i32;
    /// `getMinNonEmptyHeight`: the bottom of the lowest section with blocks.
    fn min_non_empty_height(&self) -> i32;
    /// The highest of the heightmaps at `(x, z)` (one above the top block).
    fn top(&self, x: i32, z: i32) -> i32;
    fn state(&self, x: i32, y: i32, z: i32) -> u32;
    fn biome(&self, x: i32, y: i32, z: i32) -> u16;
    /// `(sky, block)` light, 0 to 15 each.
    fn light(&self, x: i32, y: i32, z: i32) -> (u8, u8);
}

/// `LodDataBuilder.createFromChunk`: a chunk's 16 x 16 columns in its leaf
/// section (detail 6). Runs are read from the top down; a block that colours
/// the top of what is under it (snow, flowers) is kept as its own run.
pub fn create_from_chunk(chunk: &dyn ChunkView, blocks: &BlockTable) -> FullDataSource {
    let (cx, cz) = (chunk.chunk_x(), chunk.chunk_z());
    let section = pos::encode(pos::SECTION_BLOCK_DETAIL_LEVEL, cx.div_euclid(4), cz.div_euclid(4));
    let mut source = FullDataSource::empty(section);
    let offset_x = (cx & 3) * 16;
    let offset_z = (cz & 3) * 16;
    let min_build = chunk.min_build_height();
    let max_build = chunk.max_build_height();
    let min_non_empty = chunk.min_non_empty_height();
    let air_state = 0;
    for rx in 0..16 {
        for rz in 0..16 {
            let mut column = Vec::with_capacity(16);
            let mut last_y = max_build;
            let mut current_biome = chunk.biome(rx, last_y, rz);
            let mut current_state = air_state;
            let mut mapped = source.mapping.add_if_not_present(current_biome, current_state);
            let (mut block_light, mut sky_light) = (0u8, 15u8);
            let mut y = chunk.top(rx, rz);
            while !blocks.get(chunk.state(rx, y, rz)).air && y < max_build {
                y += 1;
            }
            let mut force_single_block = false;
            while y >= min_non_empty {
                let biome = chunk.biome(rx, y, rz);
                let state = chunk.state(rx, y, rz);
                let (sky, block) = chunk.light(rx, y + 1, rz);
                if biome != current_biome || state != current_state || force_single_block {
                    let info = blocks.get(current_state);
                    force_single_block = !info.air && !info.solid && !info.liquid && info.opacity != 16;
                    column.extend(full_point::encode(
                        mapped,
                        last_y - y,
                        y + 1 - min_build,
                        block_light,
                        sky_light,
                    ));
                    current_biome = biome;
                    current_state = state;
                    mapped = source.mapping.add_if_not_present(current_biome, current_state);
                    block_light = block;
                    sky_light = sky;
                    last_y = y;
                }
                y -= 1;
            }
            column.extend(full_point::encode(
                mapped,
                last_y - y,
                y + 1 - min_build,
                block_light,
                sky_light,
            ));
            source.set_column(offset_x + rx, offset_z + rz, column, gen_step::LIGHT);
        }
    }
    source
}

/// A run of one sampled column, from the world generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampledRun {
    /// Absolute block heights, `min..max`.
    pub min: i32,
    pub max: i32,
    pub state: u32,
    pub biome: u16,
    pub sky: u8,
    pub block: u8,
}

/// The world generator, as far terrain samples it: one column at a time,
/// without generating chunks. Runs come from the bottom up; gaps are air.
pub trait ColumnSampler: Send {
    fn column(&mut self, x: i32, z: i32, out: &mut Vec<SampledRun>);
}

/// Distant generation (`WorldGenerationQueue` with a generator's
/// `generateLod`): fills the columns of `source` that have no data, one
/// sample per data column at its centre, at the section's own detail
/// level. Returns how many columns it filled.
pub fn generate_missing(
    source: &mut FullDataSource,
    sampler: &mut dyn ColumnSampler,
    min_build: i32,
    max_build: i32,
) -> usize {
    let width = pos::data_width(source.pos);
    let (base_x, base_z) = (pos::min_corner_block_x(source.pos), pos::min_corner_block_z(source.pos));
    let mut runs = Vec::new();
    let mut filled = 0;
    for x in 0..pos::WIDTH {
        for z in 0..pos::WIDTH {
            if source.gen_steps[index(x, z)] != gen_step::EMPTY {
                continue;
            }
            runs.clear();
            sampler.column(
                base_x + x * width + width / 2,
                base_z + z * width + width / 2,
                &mut runs,
            );
            let column = column_from_runs(&mut source.mapping, &runs, min_build, max_build);
            source.set_column(x, z, column, gen_step::SURFACE);
            filled += 1;
        }
    }
    filled
}

/// A full data column (top down) from sampled runs (bottom up), with air
/// over and between them.
pub fn column_from_runs(mapping: &mut IdMap, runs: &[SampledRun], min_build: i32, max_build: i32) -> Vec<u64> {
    let mut column = Vec::with_capacity(runs.len() * 2 + 1);
    let mut top = max_build;
    let mut above_light = (15u8, 0u8);
    for run in runs.iter().rev() {
        let (min, max) = (run.min.max(min_build), run.max.min(max_build));
        if min >= max {
            continue;
        }
        if max < top {
            let air = mapping.add_if_not_present(run.biome, 0);
            column.extend(full_point::encode(
                air,
                top - max,
                max - min_build,
                above_light.1,
                above_light.0,
            ));
        }
        let id = mapping.add_if_not_present(run.biome, run.state);
        column.extend(full_point::encode(id, max - min, min - min_build, run.block, run.sky));
        top = min;
        above_light = (run.sky, run.block);
    }
    if column.is_empty() {
        let air = mapping.add_if_not_present(0, 0);
        column.extend(full_point::encode(air, max_build - min_build, 0, 0, 15));
    }
    column
}

/// `EDhApiHorizontalQuality`: how far apart the drops in detail are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizontalQuality {
    Lowest,
    Low,
    Medium,
    High,
    Extreme,
}

#[allow(dead_code)]
impl HorizontalQuality {
    pub const ALL: [Self; 5] = [Self::Lowest, Self::Low, Self::Medium, Self::High, Self::Extreme];
    /// `(quadraticBase, distanceUnitInBlocks)`.
    pub fn values(self) -> (f64, i32) {
        match self {
            Self::Lowest => (2.0, 4),
            Self::Low => (2.0, 8),
            Self::Medium => (2.0, 12),
            Self::High => (2.2, 16),
            Self::Extreme => (2.2, 20),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Lowest => "lowest",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Extreme => "extreme",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Lowest => "Lowest",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Extreme => "Extreme",
        }
    }
}

/// `LodQuadTree`'s detail-level logic (`updateDetailLevelVariables`,
/// `calcExpectedDetailLevel`) and its walk from the root nodes down
/// (`updateNode`): which sections to draw, at which detail, around a player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailPlan {
    drop_off_unit: f64,
    drop_off_log_base: f64,
    /// `maxHorizontalResolution`: 0 draws single blocks nearest.
    max_leaf_detail: u8,
    min_root_detail: u8,
    /// The LOD render distance, in blocks.
    pub radius: i32,
}

#[allow(dead_code)]
impl DetailPlan {
    pub fn new(quality: HorizontalQuality, radius_chunks: i32) -> Self {
        let (base, unit) = quality.values();
        let mut plan = Self {
            drop_off_unit: f64::from(unit * 16),
            drop_off_log_base: base.ln(),
            max_leaf_detail: 0,
            min_root_detail: 0,
            radius: radius_chunks * 16,
        };
        let tree_root = pos::ROOT_DETAIL_LEVEL - pos::SECTION_BLOCK_DETAIL_LEVEL;
        let min = plan.detail_from_distance(f64::from(plan.radius * 2)).saturating_sub(1);
        plan.min_root_detail = min.min(tree_root).max(plan.max_leaf_detail);
        plan
    }
    /// `calcDetailLevelFromDistance` (Java's int cast truncates toward 0).
    fn detail_from_distance(&self, distance: f64) -> u8 {
        let level = ((distance / self.drop_off_unit).ln() / self.drop_off_log_base) as i64;
        level.clamp(i64::from(self.max_leaf_detail), i64::from(pos::ROOT_DETAIL_LEVEL)) as u8
    }
    /// The section detail level a section should be drawn at.
    pub fn expected_detail_level(&self, player: [f64; 2], section: i64) -> u8 {
        let dx = f64::from(pos::center_block_x(section)) - player[0];
        let dz = f64::from(pos::center_block_z(section)) - player[1];
        let detail = self.detail_from_distance((dx * dx + dz * dz).sqrt());
        detail.min(self.min_root_detail) + pos::SECTION_MINIMUM_DETAIL_LEVEL
    }
    /// The sections to draw: the root nodes over the render distance's
    /// square, split while finer than their expected detail. `skip` leaves
    /// out sections the game draws itself (in its render distance).
    pub fn sections(&self, player: [f64; 2], skip: impl Fn(i64) -> bool) -> Vec<i64> {
        let (px, pz) = (player[0].floor() as i32, player[1].floor() as i32);
        let root = pos::ROOT_DETAIL_LEVEL;
        let lo = pos::containing_block(root, px - self.radius, pz - self.radius);
        let hi = pos::containing_block(root, px + self.radius, pz + self.radius);
        let mut out = Vec::new();
        for x in pos::x(lo)..=pos::x(hi) {
            for z in pos::z(lo)..=pos::z(hi) {
                self.walk(pos::encode(root, x, z), player, &skip, &mut out);
            }
        }
        out
    }
    fn walk(&self, node: i64, player: [f64; 2], skip: &impl Fn(i64) -> bool, out: &mut Vec<i64>) {
        let (px, pz) = (player[0].floor() as i32, player[1].floor() as i32);
        if pos::chebyshev_signed_block_distance(node, px, pz) > self.radius || skip(node) {
            return;
        }
        let detail = pos::detail_level(node);
        if detail > self.expected_detail_level(player, node) && detail > pos::SECTION_MINIMUM_DETAIL_LEVEL {
            for i in 0..4 {
                self.walk(pos::child(node, i), player, skip, out);
            }
        } else {
            out.push(node);
        }
    }
}

/// `RenderDataPointUtil`: one drawn box of a render column, in a long.
/// Bits 0-3 sky light, 4-7 block light, 8-19 bottom, 20-31 top (relative to
/// the world's minimum y), 32-55 colour, 56-59 alpha / 16, 60-63 material.
///
/// The colour bits differ from Distant Horizons: it stores the block's
/// averaged texture colour there, this port the block's id in its source's
/// `IdMap`, which Horizon draws with the block's own texture. Merging,
/// transparency and face culling compare it as Distant Horizons compares
/// colours.
#[allow(dead_code)]
pub mod render_point {
    pub const EMPTY_DATA: u64 = 0;
    const ALPHA_DOWNSIZE_SHIFT: u32 = 4;
    const MATERIAL_SHIFT: u32 = 60;
    const COLOR_SHIFT: u32 = 32;
    const ALPHA_SHIFT: u32 = 56;
    const MAX_Y_SHIFT: u32 = 20;
    const MIN_Y_SHIFT: u32 = 8;
    const BLOCK_LIGHT_SHIFT: u32 = 4;
    const MASK_12: u64 = 0xFFF;
    const COLOR_MASK: u64 = 0xFF_FFFF;

    /// `color` is `alpha << 24 | id`.
    pub fn create(max_y: i32, min_y: i32, color: u32, sky: u8, block: u8, material: u8) -> u64 {
        let alpha = u64::from(color >> 24);
        ((alpha >> ALPHA_DOWNSIZE_SHIFT) << ALPHA_SHIFT)
            | ((u64::from(color) & COLOR_MASK) << COLOR_SHIFT)
            | ((max_y as u64 & MASK_12) << MAX_Y_SHIFT)
            | ((min_y as u64 & MASK_12) << MIN_Y_SHIFT)
            | ((u64::from(block) & 15) << BLOCK_LIGHT_SHIFT)
            | (u64::from(sky) & 15)
            | ((u64::from(material) & 15) << MATERIAL_SHIFT)
    }
    pub fn y_max(p: u64) -> i32 {
        ((p >> MAX_Y_SHIFT) & MASK_12) as i32
    }
    pub fn y_min(p: u64) -> i32 {
        ((p >> MIN_Y_SHIFT) & MASK_12) as i32
    }
    pub fn set_y_min(p: u64, min_y: i32) -> u64 {
        (p & !(MASK_12 << MIN_Y_SHIFT)) | ((min_y as u64 & MASK_12) << MIN_Y_SHIFT)
    }
    pub fn alpha(p: u64) -> u8 {
        ((((p >> ALPHA_SHIFT) & 15) << ALPHA_DOWNSIZE_SHIFT) | 0b1111) as u8
    }
    pub fn id(p: u64) -> u32 {
        ((p >> COLOR_SHIFT) & COLOR_MASK) as u32
    }
    pub fn color(p: u64) -> u32 {
        (u32::from(alpha(p)) << 24) | id(p)
    }
    pub fn sky(p: u64) -> u8 {
        (p & 15) as u8
    }
    pub fn block(p: u64) -> u8 {
        ((p >> BLOCK_LIGHT_SHIFT) & 15) as u8
    }
    pub fn material(p: u64) -> u8 {
        ((p >> MATERIAL_SHIFT) & 15) as u8
    }
    pub fn exists(p: u64) -> bool {
        p != EMPTY_DATA
    }
    pub fn has_zero_height(p: u64) -> bool {
        ((p >> MIN_Y_SHIFT) & 0xFF_FFFF) == 0
    }
    pub fn is_fluid(p: u64) -> bool {
        let m = material(p);
        m == super::material::WATER || m == super::material::LAVA
    }
}

/// A render column: its boxes from the top down, and the void under them
/// (`ColumnRenderView` with `getVoid`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderColumn {
    pub points: Vec<u64>,
    pub void: u64,
}

/// What the render transform takes from the player's settings and the
/// dimension.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderSettings {
    /// `enableCaveCulling` (off in dimensions with a ceiling and the End).
    pub cave_culling: bool,
    /// `caveCullingHeight`, absolute.
    pub cave_culling_height: i32,
    /// The world's minimum y.
    pub min_y: i32,
}

/// `FullDataToRenderDataTransformer.setRenderColumnView`: the boxes to draw
/// for one full data column. Air and non-colliding blocks (grass, flowers)
/// are left out (`EDhApiBlocksToAvoid.NON_COLLIDING`); a snow layer colours
/// the block under it; dark runs under the cave culling height that only
/// connect to other dark runs are folded into the box above.
pub fn render_column(full: &[u64], mapping: &IdMap, blocks: &BlockTable, settings: &RenderSettings) -> RenderColumn {
    let mut out = RenderColumn::default();
    let cave_max_y = settings.cave_culling_height - settings.min_y;
    let mut last_color = 0u32;
    let mut last_bottom = -10_000;
    let mut last_block: Option<BlockInfo> = None;
    let mut air_after_last = false;
    let mut apply_to_next: Option<(u32, BlockInfo)> = None;
    let mut sky_to_next: Option<u8> = None;
    let mut block_to_next: Option<u8> = None;
    for (i, &p) in full.iter().enumerate() {
        let bottom = full_point::bottom_y(p);
        let mut height = full_point::height(p);
        let top = bottom + height;
        let id = full_point::id(p);
        let mut sky = full_point::sky_light(p);
        let mut light = full_point::block_light(p);
        let mut info = blocks.get(mapping.state(id));
        let mut color_id = id;
        if info.air {
            apply_to_next = None;
            sky_to_next = None;
            block_to_next = None;
            air_after_last = true;
        }
        if info.material == material::WATER
            && let Some(last) = last_block
            && last.ice
            && !air_after_last
        {
            info = last;
        }
        if !info.solid || info.liquid || info.opacity < 16 {
            if settings.cave_culling
                && sky == 0
                && top < cave_max_y
                && !out.points.is_empty()
                && i != 0
                && i + 1 < full.len()
            {
                let next_sky = full_point::sky_light(full[i + 1]);
                if next_sky == 0 && (last_color >> 24) == 255 {
                    let last = out.points.len() - 1;
                    out.points[last] = render_point::set_y_min(out.points[last], bottom);
                }
                continue;
            }
            if info.air {
                if i == full.len() - 1 {
                    set_void_air(&mut out, bottom, height, sky, light);
                }
                continue;
            }
        }
        let mut ignore = !info.solid && !info.liquid && info.opacity != 16;
        if info.snow_layer {
            height -= 1;
            if height == 0 {
                ignore = true;
                apply_to_next = Some((id, info));
            }
        }
        if ignore {
            // Flowers and grass only pass on their light: with block textures
            // their colour on the ground below would draw them as a carpet.
            if info.alpha != 0 {
                sky_to_next = Some(sky);
                block_to_next = Some(light);
            }
            if i == full.len() - 1 {
                set_void_air(&mut out, bottom, height, sky, light);
            }
            continue;
        }
        if let Some((carried, carried_info)) = apply_to_next.take() {
            color_id = carried;
            info = BlockInfo {
                alpha: 255,
                ..carried_info
            };
        }
        if let Some(s) = sky_to_next.take() {
            sky = s;
        }
        if let Some(b) = block_to_next.take() {
            light = b;
        }
        let color = (u32::from(info.alpha) << 24) | (color_id & 0xFF_FFFF);
        if color == last_color && bottom + height == last_bottom && !out.points.is_empty() {
            let last = out.points.len() - 1;
            out.points[last] = render_point::set_y_min(out.points[last], bottom);
        } else {
            out.points.push(render_point::create(
                bottom + height,
                bottom,
                color,
                sky,
                light,
                info.material,
            ));
        }
        last_bottom = bottom;
        last_color = color;
        last_block = Some(info);
        air_after_last = false;
    }
    out
}

/// `setVoidAir`: the void under a column that ends in air or ignored blocks,
/// for the light of the faces next to it.
fn set_void_air(out: &mut RenderColumn, bottom: i32, height: i32, sky: u8, light: u8) {
    let max_y = match out.points.last() {
        Some(&p) => render_point::y_min(p),
        None => bottom + height,
    };
    out.void = render_point::create(max_y, 0, 0, sky, light, material::AIR);
}

/// A section's render columns (`ColumnRenderSource`).
#[derive(Clone, Debug, PartialEq)]
pub struct RenderSource {
    pub pos: i64,
    pub columns: Vec<RenderColumn>,
}

impl RenderSource {
    /// `transformFullDataToRenderSource`.
    pub fn from_full(full: &FullDataSource, blocks: &BlockTable, settings: &RenderSettings) -> Self {
        let columns = full
            .columns
            .iter()
            .map(|c| render_column(c, &full.mapping, blocks, settings))
            .collect();
        Self { pos: full.pos, columns }
    }
    pub fn column(&self, x: i32, z: i32) -> &RenderColumn {
        &self.columns[index(x, z)]
    }
}

/// A face direction, in Distant Horizons' `EDhDirection` sense.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Dir {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

/// A face to draw, in blocks from its section's minimum corner (y is
/// absolute): `u`/`v` span x/z for up and down, x/y for north and south,
/// z/y for west and east; `plane` is the other coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Quad {
    pub dir: Dir,
    pub plane: i32,
    pub transparent: bool,
    /// The block's id in the section's `IdMap`.
    pub id: u32,
    pub sky: u8,
    pub block: u8,
    pub u0: i32,
    pub v0: i32,
    pub u1: i32,
    pub v1: i32,
}

/// A vertical range of a side face and the sky light it gets; `covered`
/// when an adjacent box hides it (`YSegmentUtil`).
#[derive(Clone, Copy, Debug)]
struct Segment {
    start: i32,
    end: i32,
    /// -1: covered by an adjacent box (`SKYLIGHT_COVERED`).
    sky: i8,
    covered: bool,
}

const SKYLIGHT_COVERED: i8 = -1;

/// `ColumnRenderBufferBuilder` with `ColumnBox.addBoxQuadsToBuilder`: the
/// faces of a section's boxes. Side faces are cut where the adjacent
/// column's boxes hide them. `neighbors` holds the sections north, south,
/// west and east of it at the same detail level, when loaded; columns at a
/// missing neighbor draw their side faces whole. Columns `cutout` names are
/// left out (the game draws that terrain itself).
pub fn build_quads(
    source: &RenderSource,
    neighbors: [Option<&RenderSource>; 4],
    settings: &RenderSettings,
    cutout: impl Fn(i32, i32) -> bool,
) -> Vec<Quad> {
    let width = pos::data_width(source.pos);
    let cave_max_y = settings.cave_culling_height - settings.min_y;
    let mut quads = Vec::new();
    let adjacent = |x: i32, z: i32| -> Option<&RenderColumn> {
        let (nx, nz, side) = match (x, z) {
            (_, -1) => (x, pos::WIDTH - 1, 0),
            (_, z) if z == pos::WIDTH => (x, 0, 1),
            (-1, _) => (pos::WIDTH - 1, z, 2),
            (x, _) if x == pos::WIDTH => (0, z, 3),
            _ => {
                if cutout(x, z) {
                    return None;
                }
                return Some(source.column(x, z));
            }
        };
        neighbors[side].map(|s| s.column(nx, nz))
    };
    for x in 0..pos::WIDTH {
        for z in 0..pos::WIDTH {
            if cutout(x, z) {
                continue;
            }
            let column = source.column(x, z);
            let adj = [
                adjacent(x, z - 1),
                adjacent(x, z + 1),
                adjacent(x - 1, z),
                adjacent(x + 1, z),
            ];
            for (i, &p) in column.points.iter().enumerate() {
                if !render_point::exists(p) || render_point::has_zero_height(p) {
                    break;
                }
                let top = if i > 0 {
                    column.points[i - 1]
                } else {
                    render_point::EMPTY_DATA
                };
                let bottom = column.points.get(i + 1).copied().unwrap_or(column.void);
                add_box(
                    &mut quads,
                    BoxAt {
                        width,
                        min_x: x * width,
                        min_z: z * width,
                        min_y: render_point::y_min(p),
                        max_y: render_point::y_max(p),
                        min_world_y: settings.min_y,
                        cave_max_y,
                    },
                    p,
                    top,
                    bottom,
                    adj,
                );
            }
        }
    }
    merge_quads(&mut quads);
    quads
}

#[derive(Clone, Copy)]
struct BoxAt {
    width: i32,
    min_x: i32,
    min_z: i32,
    /// Relative to the world's minimum y.
    min_y: i32,
    max_y: i32,
    min_world_y: i32,
    cave_max_y: i32,
}

fn is_same_transparent_block(p: u64, color: u32, material: u8) -> bool {
    render_point::material(p) == material && render_point::color(p) == color
}

fn add_box(quads: &mut Vec<Quad>, at: BoxAt, p: u64, top: u64, bottom: u64, adj: [Option<&RenderColumn>; 4]) {
    let material = render_point::material(p);
    if material == material::AIR {
        return;
    }
    let color = render_point::color(p);
    let id = render_point::id(p);
    let sky = render_point::sky(p) as i8;
    let block = render_point::block(p);
    let over_void = !render_point::exists(bottom);
    let transparent = render_point::alpha(p) < 255 && !over_void;
    let top_transparent = render_point::alpha(top) < 255;
    let bottom_transparent = render_point::alpha(bottom) < 255;
    let (x0, z0, w) = (at.min_x, at.min_z, at.width);
    let y0 = at.min_y + at.min_world_y;
    let y1 = at.max_y + at.min_world_y;
    let quad = |dir, plane, u0, v0, u1, v1, sky: i8| Quad {
        dir,
        plane,
        transparent,
        id,
        sky: sky.max(0) as u8,
        block,
        u0,
        v0,
        u1,
        v1,
    };
    let skip_top = render_point::exists(top)
        && render_point::y_min(top) == at.max_y
        && (!top_transparent || (transparent && is_same_transparent_block(top, color, material)));
    if !skip_top {
        quads.push(quad(Dir::Up, y1, x0, z0, x0 + w, z0 + w, sky));
    }
    let skip_bottom = render_point::exists(bottom)
        && render_point::y_max(bottom) == at.min_y
        && (!bottom_transparent || (transparent && is_same_transparent_block(bottom, color, material)))
        && !(render_point::is_fluid(bottom) && render_point::material(bottom) != material);
    if !skip_bottom {
        let bottom_sky = if render_point::exists(bottom) {
            render_point::sky(bottom)
        } else {
            0
        };
        quads.push(quad(Dir::Down, y0, x0, z0, x0 + w, z0 + w, bottom_sky as i8));
    }
    // North and south faces span x; west and east span z.
    let sides = [
        (Dir::North, z0, x0),
        (Dir::South, z0 + w, x0),
        (Dir::West, x0, z0),
        (Dir::East, x0 + w, z0),
    ];
    for (side, (dir, plane, u)) in sides.into_iter().enumerate() {
        let segments = match adj[side] {
            None => {
                if transparent && !over_void {
                    continue;
                }
                vec![Segment {
                    start: at.min_y,
                    end: at.max_y,
                    sky: 15,
                    covered: false,
                }]
            }
            Some(column) => side_segments(at, plane, transparent, color, material, column, dir),
        };
        for s in segments {
            if s.sky < 0 || (transparent && s.covered) || s.end <= s.start {
                continue;
            }
            let (v0, v1) = (s.start + at.min_world_y, s.end + at.min_world_y);
            quads.push(Quad {
                dir,
                plane,
                transparent,
                id,
                sky: s.sky as u8,
                block,
                u0: u,
                v0,
                u1: u + w,
                v1,
            });
        }
    }
}

/// `ColumnBox.makeAdjVerticalQuad`: splits a side face by the boxes of the
/// adjacent column, each range getting the light it faces. `plane`: where
/// the face is, in blocks from the section's corner.
fn side_segments(
    at: BoxAt,
    plane: i32,
    transparent: bool,
    color: u32,
    material: u8,
    column: &RenderColumn,
    dir: Dir,
) -> Vec<Segment> {
    let (y_min, y_max) = (at.min_y, at.max_y);
    if column.points.is_empty() || render_point::has_zero_height(column.points[0]) {
        return vec![Segment {
            start: y_min,
            end: y_max,
            sky: 15,
            covered: false,
        }];
    }
    let mut segments = vec![Segment {
        start: y_min,
        end: y_max,
        sky: 15,
        covered: false,
    }];
    let count = column.points.len() as isize;
    for adj_index in -1..count {
        let adj = if adj_index >= 0 {
            column.points[adj_index as usize]
        } else {
            column.void
        };
        let adj_min = render_point::y_min(adj);
        let mut adj_max = render_point::y_max(adj);
        if !render_point::exists(adj) || render_point::has_zero_height(adj) || y_max <= adj_min {
            continue;
        }
        let above = if adj_index > 0 {
            column.points[adj_index as usize - 1]
        } else {
            render_point::EMPTY_DATA
        };
        let mut below = if adj_index >= 0 {
            column
                .points
                .get(adj_index as usize + 1)
                .copied()
                .unwrap_or(column.void)
        } else {
            column.void
        };
        if below == render_point::EMPTY_DATA {
            below = column.void;
        }
        let adj_over_void = !render_point::exists(below);
        let adj_transparent = !adj_over_void && render_point::alpha(adj) < 255;
        let adj_sky = render_point::sky(adj) as i8;
        let same_fluid = render_point::is_fluid(adj) && render_point::material(adj) == material;
        let surface_lowered = render_point::is_fluid(adj)
            && !(render_point::exists(above)
                && render_point::y_min(above) == render_point::y_max(adj)
                && render_point::material(above) == render_point::material(adj));
        if !adj_transparent && !same_fluid && surface_lowered {
            adj_max -= 1;
        }
        let (light, covers) = if !adj_transparent {
            // A face on the section's edge, against a neighbour's box, takes
            // that box's light when it is lit and above the cave culling
            // height; the neighbour may draw it at another detail level.
            let on_border = match dir {
                Dir::West | Dir::North => plane == 0,
                _ => plane == at.width * pos::WIDTH,
            };
            let lit = render_point::sky(adj) != 0 || render_point::block(adj) != 0;
            let use_adj = on_border && lit && render_point::y_max(adj) >= at.cave_max_y && !same_fluid;
            (if use_adj { adj_sky } else { SKYLIGHT_COVERED }, true)
        } else if transparent && is_same_transparent_block(adj, color, material) {
            (SKYLIGHT_COVERED, true)
        } else {
            (render_point::sky(below) as i8, false)
        };
        segments = apply_light_to_range(&segments, adj_min, adj_max, light, covers);
        let above_min = render_point::y_min(above);
        if adj_max < above_min {
            segments = apply_light_to_range(&segments, adj_max, above_min, adj_sky, false);
        }
    }
    segments
}

/// `applyLightToRangeAndPopulateNewSgements`: the segments with
/// `start..end` taking the lower of its light and `light`, split as needed.
fn apply_light_to_range(segments: &[Segment], start: i32, end: i32, light: i8, covers: bool) -> Vec<Segment> {
    let mut out = Vec::with_capacity(segments.len() + 2);
    for &s in segments {
        if s.end <= start || s.start >= end {
            out.push(s);
            continue;
        }
        if s.start < start {
            out.push(Segment { end: start, ..s });
        }
        out.push(Segment {
            start: s.start.max(start),
            end: s.end.min(end),
            sky: light.min(s.sky),
            covered: s.covered || covers,
        });
        if s.end > end {
            out.push(Segment { start: end, ..s });
        }
    }
    out
}

/// `LodQuadBuilder`'s merging: faces in one plane with the same block and
/// light that share an edge become one, first along `u`, then along `v`.
pub fn merge_quads(quads: &mut Vec<Quad>) {
    for pass in 0..2 {
        if pass == 0 {
            quads.sort_unstable_by_key(|q| (q.dir, q.plane, q.transparent, q.id, q.sky, q.block, q.v0, q.v1, q.u0));
        } else {
            quads.sort_unstable_by_key(|q| (q.dir, q.plane, q.transparent, q.id, q.sky, q.block, q.u0, q.u1, q.v0));
        }
        let mut merged: Vec<Quad> = Vec::with_capacity(quads.len());
        for &q in quads.iter() {
            if let Some(last) = merged.last_mut() {
                let same = last.dir == q.dir
                    && last.plane == q.plane
                    && last.transparent == q.transparent
                    && last.id == q.id
                    && last.sky == q.sky
                    && last.block == q.block;
                if same && pass == 0 && last.v0 == q.v0 && last.v1 == q.v1 && last.u1 == q.u0 {
                    last.u1 = q.u1;
                    continue;
                }
                if same && pass == 1 && last.u0 == q.u0 && last.u1 == q.u1 && last.v1 == q.v0 {
                    last.v1 = q.v1;
                    continue;
                }
            }
            merged.push(q);
        }
        *quads = merged;
    }
}

/// The section files: Distant Horizons keeps its full data in an SQLite
/// database (`FullDataSourceV2Repo`); this port keeps one file per section,
/// `<detail>/<x>_<z>.dhs`, zlib-compressed, in the same layout (the id map,
/// each column's generation step and data points).
#[allow(dead_code)]
pub mod storage {
    use super::{FullDataSource, IdMap, gen_step, pos};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};

    const MAGIC: &[u8; 4] = b"DHH1";

    pub fn path(dir: &Path, section: i64) -> PathBuf {
        dir.join(pos::detail_level(section).to_string())
            .join(format!("{}_{}.dhs", pos::x(section), pos::z(section)))
    }

    /// `fingerprint` names the game's block and biome registries: a file
    /// written with others is not read back.
    pub fn encode(source: &FullDataSource, fingerprint: u64) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&source.pos.to_le_bytes());
        raw.extend_from_slice(&(source.mapping.len() as u32).to_le_bytes());
        for &(biome, state) in source.mapping.entries() {
            raw.extend_from_slice(&biome.to_le_bytes());
            raw.extend_from_slice(&state.to_le_bytes());
        }
        raw.extend(source.gen_steps.iter().map(|&s| s as u8));
        for column in &source.columns {
            raw.extend_from_slice(&(column.len() as u16).to_le_bytes());
            for p in column {
                raw.extend_from_slice(&p.to_le_bytes());
            }
        }
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&fingerprint.to_le_bytes());
        let mut z = flate2::write::ZlibEncoder::new(out, flate2::Compression::fast());
        let _ = z.write_all(&raw);
        z.finish().unwrap_or_default()
    }

    pub fn decode(bytes: &[u8], fingerprint: u64) -> Option<FullDataSource> {
        if bytes.len() < 12 || &bytes[..4] != MAGIC || bytes[4..12] != fingerprint.to_le_bytes() {
            return None;
        }
        let mut raw = Vec::new();
        flate2::read::ZlibDecoder::new(&bytes[12..])
            .read_to_end(&mut raw)
            .ok()?;
        let mut at = 0usize;
        let mut take = |n: usize| -> Option<&[u8]> {
            let s = raw.get(at..at + n)?;
            at += n;
            Some(s)
        };
        let section = i64::from_le_bytes(take(8)?.try_into().ok()?);
        let mut source = FullDataSource::empty(section);
        let entries = u32::from_le_bytes(take(4)?.try_into().ok()?);
        let mut mapping = IdMap::default();
        for _ in 0..entries {
            let biome = u16::from_le_bytes(take(2)?.try_into().ok()?);
            let state = u32::from_le_bytes(take(4)?.try_into().ok()?);
            mapping.add_if_not_present(biome, state);
        }
        if mapping.len() != entries as usize {
            return None;
        }
        source.mapping = mapping;
        let steps = take(source.gen_steps.len())?.to_vec();
        for (s, b) in source.gen_steps.iter_mut().zip(steps) {
            *s = b as i8;
        }
        for column in &mut source.columns {
            let n = u16::from_le_bytes(take(2)?.try_into().ok()?);
            for _ in 0..n {
                let p = u64::from_le_bytes(take(8)?.try_into().ok()?);
                if super::full_point::id(p) as usize >= entries as usize {
                    return None;
                }
                column.push(p);
            }
        }
        source.is_empty = source.gen_steps.iter().all(|&s| s == gen_step::EMPTY);
        Some(source)
    }

    pub fn load(dir: &Path, section: i64, fingerprint: u64) -> Option<FullDataSource> {
        let bytes = std::fs::read(path(dir, section)).ok()?;
        decode(&bytes, fingerprint).filter(|s| s.pos == section)
    }

    pub fn save(dir: &Path, source: &FullDataSource, fingerprint: u64) -> std::io::Result<()> {
        let path = path(dir, source.pos);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Written beside, then renamed: a crash never leaves half a file.
        let partial = path.with_extension("dhs.partial");
        std::fs::write(&partial, encode(source, fingerprint))?;
        std::fs::rename(partial, path)
    }
}

/// The mod's settings, one file per key in its data folder, as Horizon's
/// native mods keep them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    /// An index into `DISTANCES`.
    pub distance: usize,
    pub quality: HorizontalQuality,
    /// Distant generation: sample the world generator for terrain not
    /// visited yet (single player and LAN hosts only).
    pub generation: bool,
    pub cave_culling: bool,
    /// Tint each section by its detail level (`LodUtil.DEBUG_DETAIL_LEVEL_COLORS`).
    pub debug: bool,
    /// On a server with Distant Horizons: ask it for its LODs
    /// (`net::Client`).
    pub server: bool,
}

/// LOD render distances, in chunks (`lodChunkRenderDistanceRadius`).
pub const DISTANCES: [i32; 5] = [64, 128, 256, 512, 1024];

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            distance: 2,
            quality: HorizontalQuality::Medium,
            generation: true,
            cave_culling: true,
            debug: false,
            server: true,
        }
    }
}

#[allow(dead_code)]
impl Settings {
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        let flag = |key: &str, default: bool| match get(key).as_deref().map(str::trim) {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        };
        let distance = get("distance")
            .and_then(|v| v.trim().parse::<i32>().ok())
            .and_then(|v| DISTANCES.iter().position(|&d| d == v))
            .unwrap_or(d.distance);
        let quality = get("quality")
            .and_then(|v| HorizontalQuality::ALL.into_iter().find(|q| q.name() == v.trim()))
            .unwrap_or(d.quality);
        Self {
            enabled: flag("enabled", d.enabled),
            distance,
            quality,
            generation: flag("generation", d.generation),
            cave_culling: flag("cave_culling", d.cave_culling),
            debug: flag("debug", d.debug),
            server: flag("server", d.server),
        }
    }
    pub fn stored(&self) -> Vec<(&'static str, String)> {
        vec![
            ("enabled", self.enabled.to_string()),
            ("distance", self.radius_chunks().to_string()),
            ("quality", self.quality.name().to_owned()),
            ("generation", self.generation.to_string()),
            ("cave_culling", self.cave_culling.to_string()),
            ("debug", self.debug.to_string()),
            ("server", self.server.to_string()),
        ]
    }
    pub fn radius_chunks(&self) -> i32 {
        DISTANCES[self.distance]
    }
    pub fn labels(&self) -> Vec<String> {
        let on = |b: bool| if b { "ON" } else { "OFF" };
        vec![
            format!("Distant Horizons: {}", on(self.enabled)),
            format!("LOD render distance: {} chunks", self.radius_chunks()),
            format!("Horizontal quality: {}", self.quality.label()),
            format!("Distant generation: {}", on(self.generation)),
            format!("Cave culling: {}", on(self.cave_culling)),
            format!("Debug detail colours: {}", on(self.debug)),
            format!("Server LODs: {}", on(self.server)),
        ]
    }
    pub fn click(&mut self, button: usize) {
        match button {
            0 => self.enabled = !self.enabled,
            1 => self.distance = (self.distance + 1) % DISTANCES.len(),
            2 => {
                let i = HorizontalQuality::ALL
                    .iter()
                    .position(|&q| q == self.quality)
                    .unwrap_or(2);
                self.quality = HorizontalQuality::ALL[(i + 1) % HorizontalQuality::ALL.len()];
            }
            3 => self.generation = !self.generation,
            4 => self.cave_culling = !self.cave_culling,
            5 => self.debug = !self.debug,
            6 => self.server = !self.server,
            _ => {}
        }
    }
    /// `/dh <args>`: changes a setting; the reply line, or `None` for an
    /// unknown command.
    pub fn command(&mut self, args: &[&str]) -> Option<String> {
        match args {
            ["on"] | ["off"] => {
                self.enabled = args[0] == "on";
                Some(format!("Distant Horizons {}", if self.enabled { "on" } else { "off" }))
            }
            ["distance", n] => {
                let n: i32 = n.parse().ok()?;
                let i = DISTANCES.iter().position(|&d| d >= n).unwrap_or(DISTANCES.len() - 1);
                self.distance = i;
                Some(format!("LOD render distance: {} chunks", self.radius_chunks()))
            }
            ["quality", q] => {
                self.quality = HorizontalQuality::ALL.into_iter().find(|h| h.name() == *q)?;
                Some(format!("Horizontal quality: {}", self.quality.label()))
            }
            ["generation", v] | ["caves", v] | ["debug", v] | ["server", v] => {
                let on = match *v {
                    "on" => true,
                    "off" => false,
                    _ => return None,
                };
                match args[0] {
                    "generation" => self.generation = on,
                    "caves" => self.cave_culling = on,
                    "server" => self.server = on,
                    _ => self.debug = on,
                }
                Some(format!("{} {}", args[0], v))
            }
            _ => None,
        }
    }
}

/// `/dh`'s words, for chat completion: each word after `dh` and the values
/// it takes (none: the word alone). `Settings::command` and the bridge's
/// `status`, `save` and `clear` accept every one of them.
pub const COMMANDS: &[(&str, &[&str])] = &[
    ("status", &[]),
    ("save", &[]),
    ("clear", &[]),
    ("on", &[]),
    ("off", &[]),
    ("distance", &["64", "128", "256", "512", "1024"]),
    ("quality", &["lowest", "low", "medium", "high", "extreme"]),
    ("generation", &["on", "off"]),
    ("caves", &["on", "off"]),
    ("debug", &["on", "off"]),
    ("server", &["on", "off"]),
];

/// `LodUtil.DEBUG_DETAIL_LEVEL_COLORS`, by data detail level.
pub const DEBUG_DETAIL_LEVEL_COLORS: [u32; 13] = [
    0xFF0000, 0xFF7F00, 0xFFFF00, 0x7FFF00, 0x00FF00, 0x00FF7F, 0x00FFFF, 0x007FFF, 0x0000FF, 0x7F00FF, 0xFF00FF,
    0xFF7FFF, 0xFFFFFF,
];

/// What the engine needs of the game. Implemented on Horizon in bridge.rs,
/// and on a small fake in the tests.
pub trait Host: Send + Sync + 'static {
    type Mesh: Send + 'static;
    fn blocks(&self) -> &BlockTable;
    /// The world's minimum y and its exclusive maximum.
    fn build_height(&self) -> (i32, i32);
    /// A sampler of the world generator, for distant generation.
    fn sampler(&self) -> Option<Box<dyn ColumnSampler>>;
    /// The faces of a section, made into the game's mesh.
    fn mesh(&self, section: i64, source: &FullDataSource, quads: &[Quad], debug: bool) -> Self::Mesh;
    /// A biome and block state as a Distant Horizons server names them
    /// (`net::parse_block_state`), as the game's ids.
    fn resolve(&self, _biome: &str, _block: &str) -> (u16, u32) {
        (0, 0)
    }
    /// The server's minimum y less the game's, for its data's bottoms.
    fn remote_y_offset(&self) -> i32 {
        0
    }
}

/// The LOD world (`DhClientLevel` with its `FullDataSourceProviderV2`,
/// `LodQuadTree` and `WorldGenerationQueue`): full data kept in memory and
/// on disk, updated from visited chunks and the generator by worker
/// threads, which also mesh the sections the player should see.
pub mod engine {
    use super::*;
    use std::collections::{BinaryHeap, HashMap, HashSet};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    /// A finished section mesh, or the news that a section has nothing to
    /// draw.
    pub struct Meshed<M> {
        pub section: i64,
        pub mesh: Option<M>,
    }

    /// The near area the game draws itself: chunks it has loaded within its
    /// render distance. Their columns are left out of the LODs.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Near {
        pub center: (i32, i32),
        pub radius: i32,
        pub loaded: HashSet<(i32, i32)>,
    }

    impl Near {
        /// Whether the game draws chunk `(x, z)`: vanilla's cylindrical
        /// render-distance test (`ChunkTrackingView.isWithinDistance`).
        pub fn covers(&self, x: i32, z: i32) -> bool {
            let dx = (x - self.center.0).abs().max(1) - 1;
            let dz = (z - self.center.1).abs().max(1) - 1;
            let r = self.radius;
            dx * dx + dz * dz < r * r && self.loaded.contains(&(x, z))
        }
    }

    /// The count of sections each kind of work is waiting on, and done.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Stats {
        pub wanted: usize,
        pub shown: usize,
        pub queued: usize,
        pub loaded: usize,
        pub generated: usize,
        pub chunks: usize,
        /// Faces in the wanted sections' meshes.
        pub quads: usize,
        /// Sections a server sent, merged in.
        pub received: usize,
        /// Sections a server sent that could not be read.
        pub unreadable: usize,
    }

    #[derive(PartialEq, Eq)]
    enum Job {
        /// A visited chunk's columns.
        Chunk(u64),
        /// A full data source a server sent.
        Remote(u64),
        /// Load or generate a section, then mesh it.
        Section(i64),
        /// Mesh a section again (its data or the near area changed).
        Mesh(i64),
        /// Merge a section's changed children into it
        /// (`FullDataUpdatePropagatorV2`, batched as it batches them).
        Propagate(i64),
        Save,
    }

    #[derive(PartialEq, Eq)]
    struct Queued {
        priority: i64,
        sequence: u64,
        job: Job,
    }
    impl Ord for Queued {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            // The lowest priority first, then the oldest.
            other
                .priority
                .cmp(&self.priority)
                .then(other.sequence.cmp(&self.sequence))
        }
    }
    impl PartialOrd for Queued {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    struct Queue {
        heap: BinaryHeap<Queued>,
        sequence: u64,
        /// Sections to load or generate, queued or being worked on: not
        /// queued twice, and meshed when done, so no `Mesh` job is needed.
        sections: HashSet<i64>,
        /// Sections queued to mesh again.
        meshes: HashSet<i64>,
        /// Sections queued to take their children's changes: which children
        /// (bit `child index`) changed.
        propagate: HashMap<i64, u8>,
        chunks: HashMap<u64, Box<dyn ChunkView>>,
        next_chunk: u64,
        remote: HashMap<u64, Vec<u8>>,
    }

    struct Store {
        sections: HashMap<i64, Arc<FullDataSource>>,
        dirty: HashSet<i64>,
    }

    /// The settings the workers read, set from the game thread.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct WorkSettings {
        pub render: RenderSettings,
        pub generation: bool,
        pub debug: bool,
    }

    struct Shared<H: Host> {
        host: H,
        dir: Option<PathBuf>,
        fingerprint: u64,
        queue: Mutex<Queue>,
        ready: Condvar,
        store: Mutex<Store>,
        wanted: Mutex<HashSet<i64>>,
        near: Mutex<Arc<Near>>,
        settings: Mutex<WorkSettings>,
        done: Mutex<Vec<Meshed<H::Mesh>>>,
        stop: AtomicBool,
        generated: AtomicUsize,
        /// Each meshed section's face count.
        quads: Mutex<HashMap<i64, usize>>,
        chunks: AtomicUsize,
        /// Jobs being worked on.
        active: AtomicUsize,
        received: AtomicUsize,
        unreadable: AtomicUsize,
        last_unreadable: Mutex<Option<String>>,
    }

    pub struct Engine<H: Host> {
        shared: Arc<Shared<H>>,
        threads: Vec<JoinHandle<()>>,
        plan: Option<DetailPlan>,
        planned_at: Option<(Instant, [f64; 2])>,
        wanted: HashSet<i64>,
        /// Sections with a mesh in the game, and those it still shows that
        /// are no longer wanted (removed once what replaces them is in).
        shown: HashSet<i64>,
        retiring: HashSet<i64>,
        /// Sections meshed with nothing to draw.
        blank: HashSet<i64>,
        near_sections: HashMap<i64, usize>,
        last_save: Instant,
        /// Where the player was last, for the order of work.
        player: [f64; 2],
    }

    /// Worker threads, as Horizon's far terrain picks them for its own
    /// sampling on 12 or more threads (up to 6), else a third, 2 to 4.
    pub fn worker_threads(cores: usize) -> usize {
        if cores >= 12 {
            (cores / 2).min(6)
        } else {
            (cores / 3).clamp(2, 4)
        }
    }

    impl<H: Host> Engine<H> {
        /// `dir`: where the sections are kept (none: in memory only).
        pub fn new(host: H, dir: Option<PathBuf>, fingerprint: u64, settings: WorkSettings, threads: usize) -> Self {
            let shared = Arc::new(Shared {
                host,
                dir,
                fingerprint,
                queue: Mutex::new(Queue {
                    heap: BinaryHeap::new(),
                    sequence: 0,
                    sections: HashSet::new(),
                    meshes: HashSet::new(),
                    propagate: HashMap::new(),
                    chunks: HashMap::new(),
                    next_chunk: 0,
                    remote: HashMap::new(),
                }),
                ready: Condvar::new(),
                store: Mutex::new(Store {
                    sections: HashMap::new(),
                    dirty: HashSet::new(),
                }),
                wanted: Mutex::new(HashSet::new()),
                near: Mutex::new(Arc::new(Near::default())),
                settings: Mutex::new(settings),
                done: Mutex::new(Vec::new()),
                stop: AtomicBool::new(false),
                generated: AtomicUsize::new(0),
                chunks: AtomicUsize::new(0),
                active: AtomicUsize::new(0),
                received: AtomicUsize::new(0),
                unreadable: AtomicUsize::new(0),
                last_unreadable: Mutex::new(None),
                quads: Mutex::new(HashMap::new()),
            });
            let threads = (0..threads.max(1))
                .map(|i| {
                    let shared = shared.clone();
                    std::thread::Builder::new()
                        .name(format!("Distant Horizons worker {i}"))
                        .spawn(move || work(&shared))
                        .expect("spawn a Distant Horizons worker")
                })
                .collect();
            Self {
                shared,
                threads,
                plan: None,
                planned_at: None,
                wanted: HashSet::new(),
                shown: HashSet::new(),
                retiring: HashSet::new(),
                blank: HashSet::new(),
                near_sections: HashMap::new(),
                last_save: Instant::now(),
                player: [0.0; 2],
            }
        }

        /// New settings: everything is meshed again when they change how
        /// sections look.
        pub fn set_settings(&mut self, settings: WorkSettings) {
            let old = std::mem::replace(&mut *self.shared.settings.lock().unwrap(), settings);
            if old.render != settings.render || old.debug != settings.debug {
                for &s in self.wanted.iter() {
                    push(&self.shared, Job::Mesh(s), 0);
                }
            }
            if !old.generation && settings.generation {
                for &s in self.wanted.iter() {
                    push(&self.shared, Job::Section(s), 0);
                }
            }
        }

        /// A visited chunk, to put in the LODs.
        pub fn add_chunk(&mut self, chunk: Box<dyn ChunkView>) {
            // Nearest first, among the sections' work: neither starves the
            // other.
            let dx = f64::from(chunk.chunk_x() * 16 + 8) - self.player[0];
            let dz = f64::from(chunk.chunk_z() * 16 + 8) - self.player[1];
            let priority = dx.hypot(dz) as i64;
            let mut q = self.shared.queue.lock().unwrap();
            let id = q.next_chunk;
            q.next_chunk += 1;
            q.chunks.insert(id, chunk);
            let sequence = q.sequence;
            q.sequence += 1;
            q.heap.push(Queued {
                priority,
                sequence,
                job: Job::Chunk(id),
            });
            drop(q);
            self.shared.ready.notify_one();
        }

        /// A full data source a server sent (`FullDataSourceV2DTO` bytes):
        /// read and merged in by a worker, nearest first among the work.
        pub fn add_remote(&mut self, dto: Vec<u8>) {
            let priority = net::section_pos(&dto).map_or(0, |s| {
                let dx = f64::from(pos::center_block_x(s)) - self.player[0];
                let dz = f64::from(pos::center_block_z(s)) - self.player[1];
                dx.hypot(dz) as i64
            });
            let mut q = self.shared.queue.lock().unwrap();
            let id = q.next_chunk;
            q.next_chunk += 1;
            q.remote.insert(id, dto);
            let sequence = q.sequence;
            q.sequence += 1;
            q.heap.push(Queued {
                priority,
                sequence,
                job: Job::Remote(id),
            });
            drop(q);
            self.shared.ready.notify_one();
        }

        /// The sections planned to be drawn.
        pub fn wanted_sections(&self) -> Vec<i64> {
            self.wanted.iter().copied().collect()
        }

        /// Why the last unreadable section a server sent was.
        pub fn last_unreadable(&self) -> Option<String> {
            self.shared.last_unreadable.lock().unwrap().clone()
        }

        /// Once a frame: plans the sections to draw around `player` (block
        /// x, z) when it moved or `plan` changed, queues their work, and
        /// returns the meshes finished since and the sections to take away.
        pub fn update(&mut self, player: [f64; 2], plan: DetailPlan, near: Near) -> (Vec<Meshed<H::Mesh>>, Vec<i64>) {
            let now = Instant::now();
            self.player = player;
            let near_changed = **self.shared.near.lock().unwrap() != near;
            let moved = self.planned_at.is_none_or(|(at, p)| {
                let d = (p[0] - player[0]).hypot(p[1] - player[1]);
                d >= 16.0 || (d > 0.0 && now - at > Duration::from_secs(2))
            });
            if self.plan != Some(plan) || moved || near_changed {
                if near_changed {
                    *self.shared.near.lock().unwrap() = Arc::new(near.clone());
                }
                self.replan(player, plan, &near);
                self.planned_at = Some((now, player));
            }
            if now - self.last_save > Duration::from_secs(10) {
                self.last_save = now;
                push(&self.shared, Job::Save, i64::MAX);
            }
            let done = std::mem::take(&mut *self.shared.done.lock().unwrap());
            let mut meshes = Vec::with_capacity(done.len());
            for m in done {
                if !self.wanted.contains(&m.section) {
                    continue;
                }
                if m.mesh.is_some() {
                    self.shown.insert(m.section);
                    self.blank.remove(&m.section);
                } else {
                    self.shown.remove(&m.section);
                    self.blank.insert(m.section);
                }
                meshes.push(m);
            }
            // A retiring section goes once every wanted section over it has
            // its mesh in (or has nothing to draw): no holes meanwhile.
            let ready = |s: &i64| self.shown.contains(s) || self.blank.contains(s);
            let removed: Vec<i64> = self
                .retiring
                .iter()
                .copied()
                .filter(|&old| {
                    self.wanted
                        .iter()
                        .filter(|&&w| pos::contains(old, w) || pos::contains(w, old))
                        .all(ready)
                })
                .collect();
            for s in &removed {
                self.retiring.remove(s);
                self.shown.remove(s);
            }
            (meshes, removed)
        }

        fn replan(&mut self, player: [f64; 2], plan: DetailPlan, near: &Near) {
            let fully_near = |s: i64| {
                let (x0, z0) = (pos::min_corner_block_x(s) >> 4, pos::min_corner_block_z(s) >> 4);
                let n = pos::block_width(s) >> 4;
                n <= 64 && (0..n.max(1)).all(|dx| (0..n.max(1)).all(|dz| near.covers(x0 + dx, z0 + dz)))
            };
            let wanted: HashSet<i64> = plan.sections(player, fully_near).into_iter().collect();
            let distance = |s: i64| {
                let dx = f64::from(pos::center_block_x(s)) - player[0];
                let dz = f64::from(pos::center_block_z(s)) - player[1];
                (dx * dx + dz * dz).sqrt() as i64
            };
            // Sections across the near area's edge are meshed again when the
            // chunks it holds there change.
            let mut near_sections = HashMap::new();
            for &s in &wanted {
                let (x0, z0) = (pos::min_corner_block_x(s) >> 4, pos::min_corner_block_z(s) >> 4);
                let n = (pos::block_width(s) >> 4).max(1);
                if n > 64 {
                    continue;
                }
                let count = (0..n)
                    .flat_map(|dx| (0..n).map(move |dz| (dx, dz)))
                    .filter(|&(dx, dz)| near.covers(x0 + dx, z0 + dz))
                    .count();
                if count > 0 || self.near_sections.contains_key(&s) {
                    near_sections.insert(s, count);
                }
            }
            // Workers skip sections no longer wanted: they must see these
            // as wanted before their jobs are queued.
            *self.shared.wanted.lock().unwrap() = wanted.clone();
            for &s in &wanted {
                let shown = self.shown.contains(&s) || self.blank.contains(&s);
                if !self.wanted.contains(&s) && !shown {
                    push(&self.shared, Job::Section(s), distance(s));
                } else if shown && near_sections.get(&s) != self.near_sections.get(&s) {
                    push(&self.shared, Job::Mesh(s), distance(s));
                }
            }
            for &s in self.shown.iter() {
                if !wanted.contains(&s) {
                    self.retiring.insert(s);
                }
            }
            self.retiring.retain(|s| !wanted.contains(s));
            self.blank.retain(|s| wanted.contains(s));
            self.wanted = wanted;
            self.near_sections = near_sections;
            self.plan = Some(plan);
            // Sections far from everything wanted leave memory.
            let mut store = self.shared.store.lock().unwrap();
            if store.sections.len() > self.wanted.len() * 3 + 256 {
                let keep: Vec<i64> = store
                    .sections
                    .keys()
                    .copied()
                    .filter(|&s| !store.dirty.contains(&s))
                    .filter(|s| !self.wanted.contains(s))
                    .collect();
                for s in keep {
                    store.sections.remove(&s);
                }
            }
        }

        pub fn host(&self) -> &H {
            &self.shared.host
        }

        /// Every section the game shows, to clear them.
        pub fn shown_sections(&self) -> impl Iterator<Item = i64> + '_ {
            self.shown.iter().copied()
        }

        pub fn stats(&self) -> Stats {
            let queued = self.shared.queue.lock().unwrap().heap.len() + self.shared.active.load(Ordering::Relaxed);
            Stats {
                wanted: self.wanted.len(),
                shown: self.shown.len(),
                queued,
                loaded: self.shared.store.lock().unwrap().sections.len(),
                generated: self.shared.generated.load(Ordering::Relaxed),
                chunks: self.shared.chunks.load(Ordering::Relaxed),
                received: self.shared.received.load(Ordering::Relaxed),
                unreadable: self.shared.unreadable.load(Ordering::Relaxed),
                quads: {
                    let quads = self.shared.quads.lock().unwrap();
                    self.wanted.iter().filter_map(|s| quads.get(s)).sum()
                },
            }
        }

        /// Writes every changed section now.
        pub fn save(&self) {
            save_dirty(&self.shared);
        }

        /// Forgets every section, in memory and on disk.
        pub fn wipe(&mut self) {
            {
                let mut store = self.shared.store.lock().unwrap();
                store.sections.clear();
                store.dirty.clear();
            }
            if let Some(dir) = &self.shared.dir {
                let _ = std::fs::remove_dir_all(dir);
            }
            self.retiring.extend(self.shown.iter().copied());
            self.blank.clear();
            self.planned_at = None;
            self.plan = None;
            self.wanted.clear();
        }
    }

    impl<H: Host> Drop for Engine<H> {
        fn drop(&mut self) {
            self.shared.stop.store(true, Ordering::Relaxed);
            self.shared.ready.notify_all();
            for t in self.threads.drain(..) {
                let _ = t.join();
            }
            // Changes still on their way up the levels are carried to the top
            // first (Distant Horizons keeps them in its database for later):
            // finer levels first, as theirs add up in the coarser ones.
            loop {
                let next = {
                    let mut q = self.shared.queue.lock().unwrap();
                    let next = q.propagate.keys().copied().min_by_key(|&s| pos::detail_level(s));
                    next.map(|s| (s, q.propagate.remove(&s).unwrap_or(0)))
                };
                let Some((s, children)) = next else { break };
                propagate(&self.shared, s, children);
            }
            save_dirty(&self.shared);
        }
    }

    fn push<H: Host>(shared: &Shared<H>, job: Job, priority: i64) {
        let mut q = shared.queue.lock().unwrap();
        let duplicate = match job {
            Job::Section(s) => !q.sections.insert(s),
            Job::Mesh(s) => q.sections.contains(&s) || !q.meshes.insert(s),
            _ => false,
        };
        if duplicate {
            return;
        }
        let sequence = q.sequence;
        q.sequence += 1;
        q.heap.push(Queued {
            priority,
            sequence,
            job,
        });
        drop(q);
        shared.ready.notify_one();
    }

    fn work<H: Host>(shared: &Shared<H>) {
        let mut sampler: Option<Box<dyn ColumnSampler>> = None;
        loop {
            let (job, chunk, remote) = {
                let mut q = shared.queue.lock().unwrap();
                loop {
                    if shared.stop.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Some(next) = q.heap.pop() {
                        // Counted before the queue is let go: never neither.
                        shared.active.fetch_add(1, Ordering::Relaxed);
                        let chunk = match next.job {
                            Job::Chunk(id) => q.chunks.remove(&id),
                            _ => None,
                        };
                        let remote = match next.job {
                            Job::Remote(id) => q.remote.remove(&id),
                            _ => None,
                        };
                        break (next.job, chunk, remote);
                    }
                    q = shared.ready.wait(q).unwrap();
                }
            };
            match job {
                Job::Chunk(_) => {
                    if let Some(chunk) = chunk {
                        add_chunk(shared, &*chunk);
                    }
                }
                Job::Remote(_) => {
                    if let Some(dto) = remote {
                        add_remote(shared, &dto);
                    }
                }
                Job::Section(s) => {
                    let settings = *shared.settings.lock().unwrap();
                    // Asked again until there is one: the game's may come
                    // after the session starts (a world still loading).
                    if settings.generation && sampler.is_none() {
                        sampler = shared.host.sampler();
                    }
                    let generator = match &mut sampler {
                        Some(s) if settings.generation => Some(&mut **s as &mut dyn ColumnSampler),
                        _ => None,
                    };
                    if shared.wanted.lock().unwrap().contains(&s) {
                        prepare(shared, s, generator);
                        mesh(shared, s);
                    }
                    shared.queue.lock().unwrap().sections.remove(&s);
                }
                Job::Mesh(s) => {
                    shared.queue.lock().unwrap().meshes.remove(&s);
                    if shared.wanted.lock().unwrap().contains(&s) {
                        mesh(shared, s);
                    }
                }
                Job::Propagate(s) => {
                    let children = shared.queue.lock().unwrap().propagate.remove(&s).unwrap_or(0);
                    propagate(shared, s, children);
                }
                Job::Save => save_dirty(shared),
            }
            shared.active.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// A section from memory, else from its file, else empty.
    fn section<H: Host>(shared: &Shared<H>, store: &mut Store, s: i64) -> Arc<FullDataSource> {
        if let Some(found) = store.sections.get(&s) {
            return found.clone();
        }
        let loaded = shared
            .dir
            .as_ref()
            .and_then(|dir| storage::load(dir, s, shared.fingerprint))
            .unwrap_or_else(|| FullDataSource::empty(s));
        let loaded = Arc::new(loaded);
        store.sections.insert(s, loaded.clone());
        loaded
    }

    /// The chunk into its leaf section; the coarser levels take the change
    /// later, batched (`Propagate` jobs).
    fn add_chunk<H: Host>(shared: &Shared<H>, chunk: &dyn ChunkView) {
        let input = create_from_chunk(chunk, shared.host.blocks());
        shared.chunks.fetch_add(1, Ordering::Relaxed);
        if apply(shared, input.pos, &input) {
            changed(shared, input.pos);
        }
    }

    /// A server's section into its place in the quadtree (a wanted section,
    /// or a smaller one the server split it into: its parents take it as
    /// they take chunks).
    fn add_remote<H: Host>(shared: &Shared<H>, dto: &[u8]) {
        let host = &shared.host;
        let section = match net::decode_section(dto) {
            Ok(s) => s,
            Err(e) => {
                shared.unreadable.fetch_add(1, Ordering::Relaxed);
                *shared.last_unreadable.lock().unwrap() = Some(e);
                return;
            }
        };
        let mut resolved: HashMap<(String, String), (u16, u32)> = HashMap::new();
        let input = section.into_source(
            |biome, block| {
                *resolved
                    .entry((biome.to_owned(), block.to_owned()))
                    .or_insert_with(|| host.resolve(biome, block))
            },
            host.remote_y_offset(),
        );
        shared.received.fetch_add(1, Ordering::Relaxed);
        if apply(shared, input.pos, &input) {
            changed(shared, input.pos);
        }
    }

    /// Merges `input` into section `target` without holding the store while
    /// it works: on a copy, put back unless another worker changed the
    /// section meanwhile (then again, on the newer one). Returns whether it
    /// changed.
    fn apply<H: Host>(shared: &Shared<H>, target: i64, input: &FullDataSource) -> bool {
        let blocks = shared.host.blocks();
        loop {
            let current = section(shared, &mut shared.store.lock().unwrap(), target);
            let mut updated = (*current).clone();
            if !updated.update_from(input, blocks) {
                return false;
            }
            let mut store = shared.store.lock().unwrap();
            if store.sections.get(&target).is_some_and(|s| Arc::ptr_eq(s, &current)) {
                store.sections.insert(target, Arc::new(updated));
                store.dirty.insert(target);
                return true;
            }
        }
    }

    /// Section `s` changed: its mesh and its neighbours' borders again, and
    /// its parent queued to take the change.
    fn changed<H: Host>(shared: &Shared<H>, s: i64) {
        {
            let wanted = shared.wanted.lock().unwrap();
            for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                let n = pos::adjacent(s, dx, dz);
                if wanted.contains(&n) {
                    push(shared, Job::Mesh(n), 0);
                }
            }
        }
        if pos::detail_level(s) >= pos::ROOT_DETAIL_LEVEL {
            return;
        }
        let parent = pos::parent(s);
        let child = ((pos::x(s) & 1) | ((pos::z(s) & 1) << 1)) as u8;
        let queued = {
            let mut q = shared.queue.lock().unwrap();
            let queued = q.propagate.contains_key(&parent);
            *q.propagate.entry(parent).or_insert(0) |= 1 << child;
            queued
        };
        if !queued {
            // After the work in view, finer levels first: their changes add
            // up in the coarser ones.
            push(
                shared,
                Job::Propagate(parent),
                (1 << 20) | i64::from(pos::detail_level(parent)),
            );
        }
    }

    /// Section `s` takes the changes of its children named in `children`.
    fn propagate<H: Host>(shared: &Shared<H>, s: i64, children: u8) {
        let mut any = false;
        for i in 0..4 {
            if children & (1 << i) == 0 {
                continue;
            }
            let child = shared.store.lock().unwrap().sections.get(&pos::child(s, i)).cloned();
            if let Some(child) = child {
                any |= apply(shared, s, &child);
            }
        }
        if any {
            changed(shared, s);
        }
    }

    /// Loads section `s`, then fills what it lacks: from its parent, scaled
    /// up, and from the generator when there is one.
    fn prepare<H: Host>(shared: &Shared<H>, s: i64, generator: Option<&mut dyn ColumnSampler>) {
        let blocks = shared.host.blocks();
        let (current, parent) = {
            let mut store = shared.store.lock().unwrap();
            let current = section(shared, &mut store, s);
            let parent = (current.missing_columns() > 0 && pos::detail_level(s) < pos::ROOT_DETAIL_LEVEL)
                .then(|| store.sections.get(&pos::parent(s)).cloned())
                .flatten();
            (current, parent)
        };
        if current.missing_columns() == 0 {
            return;
        }
        let mut updated = (*current).clone();
        let mut changed = false;
        if let Some(generator) = generator {
            let (min_y, max_y) = shared.host.build_height();
            let mut generated = FullDataSource::empty(s);
            generated.gen_steps = updated.gen_steps.clone();
            if generate_missing(&mut generated, generator, min_y, max_y) > 0 {
                shared.generated.fetch_add(1, Ordering::Relaxed);
                changed |= updated.update_from(&generated, blocks);
            }
        } else if let Some(parent) = parent {
            changed |= updated.update_from(&parent, blocks);
        }
        if changed {
            let mut store = shared.store.lock().unwrap();
            if store.sections.get(&s).is_some_and(|n| Arc::ptr_eq(n, &current)) {
                store.sections.insert(s, Arc::new(updated));
                store.dirty.insert(s);
                drop(store);
            } else {
                // A chunk changed it meanwhile: merged into the newest.
                drop(store);
                apply(shared, s, &updated);
            }
        }
    }

    fn mesh<H: Host>(shared: &Shared<H>, s: i64) {
        let settings = *shared.settings.lock().unwrap();
        let blocks = shared.host.blocks();
        let (source, neighbors) = {
            let mut store = shared.store.lock().unwrap();
            let source = section(shared, &mut store, s);
            let neighbors = [(0, -1), (0, 1), (-1, 0), (1, 0)]
                .map(|(dx, dz)| store.sections.get(&pos::adjacent(s, dx, dz)).cloned());
            (source, neighbors)
        };
        let near = shared.near.lock().unwrap().clone();
        let render = RenderSource::from_full(&source, blocks, &settings.render);
        let neighbors = neighbors.map(|n| n.map(|n| RenderSource::from_full(&n, blocks, &settings.render)));
        let width = pos::data_width(s);
        let (bx, bz) = (pos::min_corner_block_x(s), pos::min_corner_block_z(s));
        let cutout = |x: i32, z: i32| {
            width <= 16 && near.covers((bx + x * width + width / 2) >> 4, (bz + z * width + width / 2) >> 4)
        };
        let quads = build_quads(
            &render,
            neighbors.each_ref().map(Option::as_ref),
            &settings.render,
            cutout,
        );
        let mesh = (!quads.is_empty()).then(|| shared.host.mesh(s, &source, &quads, settings.debug));
        shared.quads.lock().unwrap().insert(s, quads.len());
        shared.done.lock().unwrap().push(Meshed { section: s, mesh });
    }

    fn save_dirty<H: Host>(shared: &Shared<H>) {
        let Some(dir) = &shared.dir else {
            shared.store.lock().unwrap().dirty.clear();
            return;
        };
        let pending: Vec<Arc<FullDataSource>> = {
            let mut store = shared.store.lock().unwrap();
            let dirty: Vec<i64> = store.dirty.drain().collect();
            dirty.iter().filter_map(|s| store.sections.get(s).cloned()).collect()
        };
        for source in pending {
            let _ = storage::save(dir, &source, shared.fingerprint);
        }
    }
}

/// Zstandard frames (RFC 8878), decompressed: Distant Horizons' network
/// data is compressed with `EDhApiDataCompressionMode.Z_STD_BLOCK`, its
/// default. Horizon's native mods can add no crates, so this is a decoder of
/// its own: every block type, Huffman literals and FSE sequences; no
/// dictionaries (Distant Horizons uses none).
#[allow(dead_code)]
pub mod zstd {
    const MAGIC: u32 = 0xFD2F_B528;
    /// Output past this is refused: a section's blob is a few hundred KB.
    const MAX_OUTPUT: usize = 64 << 20;

    pub type Result<T> = std::result::Result<T, String>;

    fn err<T>(what: &str) -> Result<T> {
        Err(format!("zstd: {what}"))
    }

    /// Every frame in `src` (skippable frames skipped), one after another.
    pub fn decompress(src: &[u8]) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < src.len() {
            let magic = u32::from_le_bytes(src.get(at..at + 4).ok_or("zstd: truncated magic")?.try_into().unwrap());
            if magic & 0xFFFF_FFF0 == 0x184D_2A50 {
                let size = u32::from_le_bytes(
                    src.get(at + 4..at + 8)
                        .ok_or("zstd: truncated skippable frame")?
                        .try_into()
                        .unwrap(),
                );
                at += 8 + size as usize;
                continue;
            }
            if magic != MAGIC {
                return err("bad magic number");
            }
            at = frame(src, at + 4, &mut out)?;
        }
        Ok(out)
    }

    /// The tables a frame's blocks may repeat from the block before.
    struct State {
        huffman: Option<Huffman>,
        ll: Option<Fse>,
        of: Option<Fse>,
        ml: Option<Fse>,
        reps: [usize; 3],
    }

    fn frame(src: &[u8], mut at: usize, out: &mut Vec<u8>) -> Result<usize> {
        let descriptor = *src.get(at).ok_or("zstd: truncated frame header")?;
        at += 1;
        let fcs_flag = descriptor >> 6;
        let single_segment = descriptor & 0x20 != 0;
        let checksum = descriptor & 0x04 != 0;
        if descriptor & 0x08 != 0 {
            return err("reserved bit set");
        }
        let dict_size = [0, 1, 2, 4][usize::from(descriptor & 3)];
        if !single_segment {
            at += 1; // window descriptor: the whole output is the window here
        }
        let dict = src.get(at..at + dict_size).ok_or("zstd: truncated dictionary id")?;
        if dict.iter().any(|&b| b != 0) {
            return err("dictionaries are not supported");
        }
        at += dict_size;
        let fcs_size = match fcs_flag {
            0 => usize::from(single_segment),
            1 => 2,
            2 => 4,
            _ => 8,
        };
        let fcs = src.get(at..at + fcs_size).ok_or("zstd: truncated content size")?;
        let mut content_size = fcs.iter().rev().fold(0u64, |v, &b| v << 8 | u64::from(b));
        if fcs_size == 2 {
            content_size += 256;
        }
        at += fcs_size;
        if fcs_size > 0 && content_size as usize > MAX_OUTPUT {
            return err("frame too large");
        }
        let start = out.len();
        if fcs_size > 0 {
            out.reserve(content_size as usize);
        }
        let mut state = State {
            huffman: None,
            ll: None,
            of: None,
            ml: None,
            reps: [1, 4, 8],
        };
        loop {
            let header = src.get(at..at + 3).ok_or("zstd: truncated block header")?;
            let header = u32::from(header[0]) | u32::from(header[1]) << 8 | u32::from(header[2]) << 16;
            at += 3;
            let last = header & 1 != 0;
            let size = (header >> 3) as usize;
            match (header >> 1) & 3 {
                0 => out.extend_from_slice(src.get(at..at + size).ok_or("zstd: truncated raw block")?),
                1 => {
                    let byte = *src.get(at).ok_or("zstd: truncated RLE block")?;
                    out.resize(out.len() + size, byte);
                }
                2 => compressed_block(
                    src.get(at..at + size).ok_or("zstd: truncated block")?,
                    &mut state,
                    out,
                    start,
                )?,
                _ => return err("reserved block type"),
            }
            at += if (header >> 1) & 3 == 1 { 1 } else { size };
            if out.len() - start > MAX_OUTPUT {
                return err("output too large");
            }
            if last {
                break;
            }
        }
        if fcs_size > 0 && (out.len() - start) as u64 != content_size {
            return err("content size mismatch");
        }
        if checksum {
            at += 4; // XXH64's low 32 bits: not checked
        }
        Ok(at)
    }

    // ---- bit readers ----

    /// Little-endian bits from the front (FSE table descriptions).
    struct ForwardBits<'a> {
        data: &'a [u8],
        bit: usize,
    }
    impl ForwardBits<'_> {
        fn read(&mut self, n: u32) -> Result<u32> {
            let mut v = 0u32;
            for i in 0..n {
                let b = self.bit + i as usize;
                let byte = *self.data.get(b / 8).ok_or("zstd: truncated table description")?;
                v |= u32::from((byte >> (b % 8)) & 1) << i;
            }
            self.bit += n as usize;
            Ok(v)
        }
        fn peek(&self, n: u32) -> u32 {
            let mut v = 0u32;
            for i in 0..n {
                let b = self.bit + i as usize;
                let byte = self.data.get(b / 8).copied().unwrap_or(0);
                v |= u32::from((byte >> (b % 8)) & 1) << i;
            }
            v
        }
        fn bytes_used(&self) -> usize {
            self.bit.div_ceil(8)
        }
    }

    /// Bits read from the back, most significant first: the streams of
    /// Huffman literals, FSE weights and sequences. Bits past the start read
    /// as zeros, and are counted (`remaining` goes negative).
    struct BackwardBits<'a> {
        data: &'a [u8],
        /// Bits not read yet: those below this position.
        pos: isize,
    }
    impl<'a> BackwardBits<'a> {
        fn new(data: &'a [u8]) -> Result<Self> {
            let last = *data.last().ok_or("zstd: empty bitstream")?;
            if last == 0 {
                return err("bitstream without its end mark");
            }
            // The highest set bit of the last byte only marks the end.
            let pos = (data.len() * 8) as isize - last.leading_zeros() as isize - 1;
            Ok(Self { data, pos })
        }
        /// Bits `start..start + n` as a number (bits below 0 are zeros).
        fn bits(&self, start: isize, n: u32) -> u64 {
            if n == 0 {
                return 0;
            }
            if start < 0 {
                let shift = (-start) as u32;
                return if shift >= n {
                    0
                } else {
                    self.bits(0, n - shift) << shift
                };
            }
            let start = start as usize;
            let first = start / 8;
            let mut word = 0u64;
            for i in 0..8 {
                if let Some(&b) = self.data.get(first + i) {
                    word |= u64::from(b) << (8 * i);
                }
            }
            (word >> (start % 8)) & ((1u64 << n) - 1)
        }
        fn read(&mut self, n: u32) -> u64 {
            self.pos -= n as isize;
            self.bits(self.pos, n)
        }
        fn peek(&self, n: u32) -> u64 {
            self.bits(self.pos - n as isize, n)
        }
        fn consume(&mut self, n: u32) {
            self.pos -= n as isize;
        }
        fn remaining(&self) -> isize {
            self.pos
        }
    }

    // ---- FSE ----

    #[derive(Clone)]
    struct Fse {
        log: u32,
        symbols: Vec<u8>,
        bits: Vec<u8>,
        base: Vec<u16>,
    }

    impl Fse {
        /// `FSE_buildDTable` from normalized counts (-1: "less than 1").
        fn build(counts: &[i16], log: u32) -> Result<Self> {
            let size = 1usize << log;
            let mut symbols = vec![0u8; size];
            let mut next = vec![0u32; counts.len()];
            let mut high = size - 1;
            for (s, &c) in counts.iter().enumerate() {
                if c == -1 {
                    symbols[high] = s as u8;
                    high = high.wrapping_sub(1);
                    next[s] = 1;
                } else {
                    next[s] = c.max(0) as u32;
                }
            }
            let step = (size >> 1) + (size >> 3) + 3;
            let mask = size - 1;
            let mut position = 0usize;
            for (s, &c) in counts.iter().enumerate() {
                for _ in 0..c.max(0) {
                    symbols[position] = s as u8;
                    loop {
                        position = (position + step) & mask;
                        if position <= high || high == usize::MAX {
                            break;
                        }
                    }
                }
            }
            if position != 0 {
                return err("bad FSE distribution");
            }
            let mut bits = vec![0u8; size];
            let mut base = vec![0u16; size];
            for u in 0..size {
                let s = usize::from(symbols[u]);
                let state = next[s];
                next[s] += 1;
                if state == 0 {
                    return err("bad FSE distribution");
                }
                let nb = log - (31 - state.leading_zeros());
                bits[u] = nb as u8;
                base[u] = ((state << nb) as usize - size) as u16;
            }
            Ok(Self {
                log,
                symbols,
                bits,
                base,
            })
        }

        /// A table description (RFC 8878 4.1.1); returns it and the bytes used.
        fn read(data: &[u8], max_log: u32, max_symbol: usize) -> Result<(Self, usize)> {
            let mut r = ForwardBits { data, bit: 0 };
            let log = r.read(4)? + 5;
            if log > max_log {
                return err("FSE accuracy too high");
            }
            let mut remaining = (1i32 << log) + 1;
            let mut threshold = 1i32 << log;
            let mut nb = log + 1;
            let mut counts: Vec<i16> = Vec::new();
            while remaining > 1 {
                if counts.len() > max_symbol {
                    return err("too many FSE symbols");
                }
                let max = (2 * threshold - 1) - remaining;
                let low = r.peek(nb - 1) as i32;
                let value = if low < max {
                    r.read(nb - 1)? as i32
                } else {
                    let mut v = r.read(nb)? as i32;
                    if v >= threshold {
                        v -= max;
                    }
                    v
                };
                let count = value - 1;
                remaining -= count.abs();
                counts.push(count as i16);
                if count == 0 {
                    loop {
                        let repeat = r.read(2)?;
                        counts.extend(std::iter::repeat_n(0, repeat as usize));
                        if repeat != 3 {
                            break;
                        }
                    }
                }
                while remaining < threshold && threshold > 1 {
                    nb -= 1;
                    threshold >>= 1;
                }
            }
            if remaining != 1 || counts.len() > max_symbol + 1 {
                return err("bad FSE table description");
            }
            Ok((Self::build(&counts, log)?, r.bytes_used()))
        }

        fn rle(symbol: u8) -> Self {
            Self {
                log: 0,
                symbols: vec![symbol],
                bits: vec![0],
                base: vec![0],
            }
        }
    }

    struct FseState<'t> {
        table: &'t Fse,
        state: usize,
    }
    impl<'t> FseState<'t> {
        fn new(table: &'t Fse, bits: &mut BackwardBits) -> Self {
            let state = bits.read(table.log) as usize;
            Self { table, state }
        }
        fn symbol(&self) -> u8 {
            self.table.symbols[self.state]
        }
        fn update(&mut self, bits: &mut BackwardBits) {
            let nb = u32::from(self.table.bits[self.state]);
            self.state = usize::from(self.table.base[self.state]) + bits.read(nb) as usize;
        }
    }

    // ---- Huffman ----

    #[derive(Clone)]
    struct Huffman {
        max_bits: u32,
        /// By the next `max_bits` bits: the symbol and its code length.
        entries: Vec<(u8, u8)>,
    }

    impl Huffman {
        /// A tree description; returns it and the bytes used.
        fn read(data: &[u8]) -> Result<(Self, usize)> {
            let header = usize::from(*data.first().ok_or("zstd: truncated Huffman tree")?);
            let (mut weights, used) = if header < 128 {
                let body = data.get(1..1 + header).ok_or("zstd: truncated Huffman weights")?;
                let (table, table_size) = Fse::read(body, 6, 255)?;
                let mut bits = BackwardBits::new(&body[table_size..])?;
                let mut a = FseState::new(&table, &mut bits);
                let mut b = FseState::new(&table, &mut bits);
                let mut weights = Vec::new();
                loop {
                    weights.push(a.symbol());
                    a.update(&mut bits);
                    if bits.remaining() < 0 {
                        weights.push(b.symbol());
                        break;
                    }
                    weights.push(b.symbol());
                    b.update(&mut bits);
                    if bits.remaining() < 0 {
                        weights.push(a.symbol());
                        break;
                    }
                    if weights.len() > 255 {
                        return err("too many Huffman weights");
                    }
                }
                (weights, 1 + header)
            } else {
                let count = header - 127;
                let body = data
                    .get(1..1 + count.div_ceil(2))
                    .ok_or("zstd: truncated Huffman weights")?;
                let weights = (0..count)
                    .map(|i| if i % 2 == 0 { body[i / 2] >> 4 } else { body[i / 2] & 15 })
                    .collect();
                (weights, 1 + count.div_ceil(2))
            };
            if weights.len() > 255 {
                return err("too many Huffman weights");
            }
            let total: u32 = weights.iter().filter(|&&w| w > 0).map(|&w| 1u32 << (w - 1)).sum();
            if total == 0 {
                return err("empty Huffman tree");
            }
            let max_bits = 32 - total.leading_zeros(); // log2 of the next power of two
            let rest = (1u32 << max_bits) - total;
            if !rest.is_power_of_two() || max_bits > 11 {
                return err("bad Huffman weights");
            }
            weights.push((rest.trailing_zeros() + 1) as u8);
            let mut rank_count = [0u32; 13];
            for &w in &weights {
                if usize::from(w) > max_bits as usize {
                    return err("bad Huffman weight");
                }
                rank_count[usize::from(w)] += 1;
            }
            let mut rank_start = [0u32; 13];
            let mut next = 0;
            for w in 1..=max_bits as usize {
                rank_start[w] = next;
                next += rank_count[w] << (w - 1);
            }
            let mut entries = vec![(0u8, 0u8); 1 << max_bits];
            for (symbol, &w) in weights.iter().enumerate() {
                if w == 0 {
                    continue;
                }
                let w = usize::from(w);
                let length = 1u32 << (w - 1);
                let start = rank_start[w] as usize;
                for e in &mut entries[start..start + length as usize] {
                    *e = (symbol as u8, (max_bits + 1 - w as u32) as u8);
                }
                rank_start[w] += length;
            }
            Ok((Self { max_bits, entries }, used))
        }

        fn decode_stream(&self, data: &[u8], count: usize, out: &mut Vec<u8>) -> Result<()> {
            let mut bits = BackwardBits::new(data)?;
            for _ in 0..count {
                let (symbol, length) = self.entries[bits.peek(self.max_bits) as usize];
                bits.consume(u32::from(length));
                out.push(symbol);
            }
            if bits.remaining() != 0 {
                return err("Huffman stream not fully read");
            }
            Ok(())
        }
    }

    // ---- blocks ----

    fn literals(block: &[u8], state: &mut State) -> Result<(Vec<u8>, usize)> {
        let b = |i: usize| {
            block
                .get(i)
                .copied()
                .map(usize::from)
                .ok_or("zstd: truncated literals header")
        };
        let b0 = b(0)?;
        let kind = b0 & 3;
        let format = (b0 >> 2) & 3;
        match kind {
            0 | 1 => {
                let (size, header) = match format {
                    0 | 2 => (b0 >> 3, 1),
                    1 => ((b0 >> 4) | b(1)? << 4, 2),
                    _ => ((b0 >> 4) | b(1)? << 4 | b(2)? << 12, 3),
                };
                if kind == 0 {
                    let raw = block.get(header..header + size).ok_or("zstd: truncated raw literals")?;
                    Ok((raw.to_vec(), header + size))
                } else {
                    Ok((vec![b(header)? as u8; size], header + 1))
                }
            }
            _ => {
                let (regenerated, compressed, header, streams) = match format {
                    0 | 1 => (
                        (b0 >> 4) | (b(1)? & 0x3F) << 4,
                        (b(1)? >> 6) | b(2)? << 2,
                        3,
                        if format == 0 { 1 } else { 4 },
                    ),
                    2 => (
                        (b0 >> 4) | b(1)? << 4 | (b(2)? & 3) << 12,
                        (b(2)? >> 2) | b(3)? << 6,
                        4,
                        4,
                    ),
                    _ => (
                        (b0 >> 4) | b(1)? << 4 | (b(2)? & 0x3F) << 12,
                        (b(2)? >> 6) | b(3)? << 2 | b(4)? << 10,
                        5,
                        4,
                    ),
                };
                let mut body = block
                    .get(header..header + compressed)
                    .ok_or("zstd: truncated literals")?;
                if kind == 2 {
                    let (table, used) = Huffman::read(body)?;
                    state.huffman = Some(table);
                    body = &body[used..];
                }
                let table = state.huffman.as_ref().ok_or("zstd: treeless literals without a tree")?;
                let mut out = Vec::with_capacity(regenerated);
                if streams == 1 {
                    table.decode_stream(body, regenerated, &mut out)?;
                } else {
                    let jump = body.get(..6).ok_or("zstd: truncated jump table")?;
                    let sizes = [0, 2, 4].map(|i| usize::from(u16::from_le_bytes([jump[i], jump[i + 1]])));
                    let per = regenerated.div_ceil(4);
                    let mut at = 6;
                    for (i, &size) in sizes.iter().enumerate() {
                        let stream = body.get(at..at + size).ok_or("zstd: truncated literal stream")?;
                        table.decode_stream(stream, per, &mut out)?;
                        at += size;
                        let _ = i;
                    }
                    let last = regenerated.checked_sub(3 * per).ok_or("zstd: bad literal sizes")?;
                    table.decode_stream(body.get(at..).ok_or("zstd: truncated literal stream")?, last, &mut out)?;
                }
                Ok((out, header + compressed))
            }
        }
    }

    const LL_BASE: [u32; 36] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 20, 22, 24, 28, 32, 40, 48, 64, 128, 256, 512,
        1024, 2048, 4096, 8192, 16384, 32768, 65536,
    ];
    const LL_BITS: [u32; 36] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 3, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
        16,
    ];
    const ML_BASE: [u32; 53] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
        32, 33, 34, 35, 37, 39, 41, 43, 47, 51, 59, 67, 83, 99, 131, 259, 515, 1027, 2051, 4099, 8195, 16387, 32771,
        65539,
    ];
    const ML_BITS: [u32; 53] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2,
        2, 3, 3, 4, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
    ];
    const LL_DEFAULT: [i16; 36] = [
        4, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 2, 1, 1, 1, 1, 1, -1, -1, -1, -1,
    ];
    const ML_DEFAULT: [i16; 53] = [
        1, 4, 3, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1, -1, -1,
    ];
    const OF_DEFAULT: [i16; 29] = [
        1, 1, 1, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1,
    ];

    /// One of the three sequence tables, by its mode; returns the bytes used.
    fn sequence_table(
        mode: u8,
        data: &[u8],
        slot: &mut Option<Fse>,
        default: &[i16],
        default_log: u32,
        max_log: u32,
    ) -> Result<usize> {
        match mode {
            0 => {
                *slot = Some(Fse::build(default, default_log)?);
                Ok(0)
            }
            1 => {
                let symbol = *data.first().ok_or("zstd: truncated RLE table")?;
                if usize::from(symbol) >= default.len() {
                    return err("bad RLE symbol");
                }
                *slot = Some(Fse::rle(symbol));
                Ok(1)
            }
            2 => {
                let (table, used) = Fse::read(data, max_log, default.len() - 1)?;
                *slot = Some(table);
                Ok(used)
            }
            _ => {
                if slot.is_none() {
                    return err("repeated table without one before");
                }
                Ok(0)
            }
        }
    }

    fn compressed_block(block: &[u8], state: &mut State, out: &mut Vec<u8>, frame_start: usize) -> Result<()> {
        let (lits, mut at) = literals(block, state)?;
        let b = |i: usize| {
            block
                .get(i)
                .copied()
                .map(usize::from)
                .ok_or("zstd: truncated sequences header")
        };
        let b0 = b(at)?;
        let count = if b0 == 0 {
            at += 1;
            0
        } else if b0 < 128 {
            at += 1;
            b0
        } else if b0 < 255 {
            at += 2;
            ((b0 - 128) << 8) + b(at - 1)?
        } else {
            at += 3;
            b(at - 2)? + (b(at - 1)? << 8) + 0x7F00
        };
        if count == 0 {
            out.extend_from_slice(&lits);
            return Ok(());
        }
        let modes = b(at)? as u8;
        at += 1;
        if modes & 3 != 0 {
            return err("reserved sequence mode bits");
        }
        at += sequence_table(modes >> 6, &block[at..], &mut state.ll, &LL_DEFAULT, 6, 9)?;
        at += sequence_table((modes >> 4) & 3, &block[at..], &mut state.of, &OF_DEFAULT, 5, 8)?;
        at += sequence_table((modes >> 2) & 3, &block[at..], &mut state.ml, &ML_DEFAULT, 6, 9)?;
        let mut bits = BackwardBits::new(block.get(at..).ok_or("zstd: truncated sequences")?)?;
        let (ll_table, of_table, ml_table) = (
            state.ll.as_ref().unwrap(),
            state.of.as_ref().unwrap(),
            state.ml.as_ref().unwrap(),
        );
        let mut ll = FseState::new(ll_table, &mut bits);
        let mut of = FseState::new(of_table, &mut bits);
        let mut ml = FseState::new(ml_table, &mut bits);
        let mut lit_at = 0usize;
        for i in 0..count {
            let (ll_code, of_code, ml_code) = (
                usize::from(ll.symbol()),
                u32::from(of.symbol()),
                usize::from(ml.symbol()),
            );
            if ll_code >= LL_BASE.len() || ml_code >= ML_BASE.len() || of_code > 31 {
                return err("bad sequence code");
            }
            let offset_value = (1u64 << of_code) + bits.read(of_code);
            let match_length = ML_BASE[ml_code] as usize + bits.read(ML_BITS[ml_code]) as usize;
            let literal_length = LL_BASE[ll_code] as usize + bits.read(LL_BITS[ll_code]) as usize;
            if i + 1 < count {
                ll.update(&mut bits);
                ml.update(&mut bits);
                of.update(&mut bits);
            }
            // Repeat offsets (RFC 8878 3.1.2.5).
            let reps = &mut state.reps;
            let offset = if offset_value > 3 {
                let o = (offset_value - 3) as usize;
                *reps = [o, reps[0], reps[1]];
                o
            } else {
                let index = offset_value as usize - 1 + usize::from(literal_length == 0);
                match index {
                    0 => reps[0],
                    1 => {
                        let o = reps[1];
                        *reps = [o, reps[0], reps[2]];
                        o
                    }
                    2 => {
                        let o = reps[2];
                        *reps = [o, reps[0], reps[1]];
                        o
                    }
                    _ => {
                        let o = reps[0]
                            .checked_sub(1)
                            .filter(|&o| o > 0)
                            .ok_or("zstd: bad repeat offset")?;
                        *reps = [o, reps[0], reps[1]];
                        o
                    }
                }
            };
            let literal = lits
                .get(lit_at..lit_at + literal_length)
                .ok_or("zstd: literals overrun")?;
            out.extend_from_slice(literal);
            lit_at += literal_length;
            if offset == 0 || offset > out.len() - frame_start {
                return err("match offset out of range");
            }
            if out.len() + match_length > frame_start + MAX_OUTPUT {
                return err("output too large");
            }
            let from = out.len() - offset;
            if offset >= match_length {
                out.extend_from_within(from..from + match_length);
            } else {
                for k in 0..match_length {
                    let byte = out[from + k];
                    out.push(byte);
                }
            }
        }
        if bits.remaining() != 0 {
            return err("sequence stream not fully read");
        }
        out.extend_from_slice(&lits[lit_at..]);
        Ok(())
    }
}

/// LZ4 frames (lz4-java's `LZ4FrameInputStream`), for servers set to
/// `EDhApiDataCompressionMode.LZ4`. Checksums are not checked.
#[allow(dead_code)]
pub mod lz4 {
    const MAGIC: u32 = 0x184D_2204;
    const MAX_OUTPUT: usize = 64 << 20;

    pub fn decompress(src: &[u8]) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut at = 0;
        let u32_at = |at: usize| -> Result<u32, String> {
            Ok(u32::from_le_bytes(
                src.get(at..at + 4).ok_or("lz4: truncated")?.try_into().unwrap(),
            ))
        };
        while at < src.len() {
            let magic = u32_at(at)?;
            if magic & 0xFFFF_FFF0 == 0x184D_2A50 {
                at += 8 + u32_at(at + 4)? as usize;
                continue;
            }
            if magic != MAGIC {
                return Err("lz4: bad magic number".into());
            }
            let flags = *src.get(at + 4).ok_or("lz4: truncated header")?;
            if flags >> 6 != 1 {
                return Err("lz4: unsupported frame version".into());
            }
            let block_checksum = flags & 0x10 != 0;
            let content_size = flags & 0x08 != 0;
            let content_checksum = flags & 0x04 != 0;
            let dictionary = flags & 0x01 != 0;
            // FLG, BD, the optional fields, then the header checksum.
            at += 4 + 2 + if content_size { 8 } else { 0 } + if dictionary { 4 } else { 0 } + 1;
            loop {
                let size = u32_at(at)?;
                at += 4;
                if size == 0 {
                    break;
                }
                let raw = size & 0x8000_0000 != 0;
                let size = (size & 0x7FFF_FFFF) as usize;
                let block = src.get(at..at + size).ok_or("lz4: truncated block")?;
                if raw {
                    out.extend_from_slice(block);
                } else {
                    block_decompress(block, &mut out)?;
                }
                if out.len() > MAX_OUTPUT {
                    return Err("lz4: output too large".into());
                }
                at += size + if block_checksum { 4 } else { 0 };
            }
            if content_checksum {
                at += 4;
            }
        }
        Ok(out)
    }

    /// One LZ4 block, appended to `out` (whose earlier bytes it may copy:
    /// linked blocks).
    pub fn block_decompress(block: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
        let mut at = 0;
        let byte = |at: usize| {
            block
                .get(at)
                .copied()
                .ok_or_else(|| "lz4: truncated sequence".to_string())
        };
        let length = |at: &mut usize, mut n: usize| -> Result<usize, String> {
            if n == 15 {
                loop {
                    let b = byte(*at)?;
                    *at += 1;
                    n += usize::from(b);
                    if b != 255 {
                        break;
                    }
                }
            }
            Ok(n)
        };
        loop {
            let token = byte(at)?;
            at += 1;
            let literals = length(&mut at, usize::from(token >> 4))?;
            out.extend_from_slice(block.get(at..at + literals).ok_or("lz4: truncated literals")?);
            at += literals;
            if at == block.len() {
                return Ok(());
            }
            let offset = usize::from(u16::from_le_bytes([byte(at)?, byte(at + 1)?]));
            at += 2;
            let matched = length(&mut at, usize::from(token & 15))? + 4;
            if offset == 0 || offset > out.len() {
                return Err("lz4: bad offset".into());
            }
            if out.len() + matched > MAX_OUTPUT {
                return Err("lz4: output too large".into());
            }
            let from = out.len() - offset;
            for k in 0..matched {
                let b = out[from + k];
                out.push(b);
            }
        }
    }
}

/// Distant Horizons' networking (`core.network` and `core.multiplayer`),
/// client side: the messages on the `distant_horizons:msg` plugin channel,
/// the full data sources a server sends (`FullDataSourceV2DTO`), and the
/// requests for them (`ClientNetworkState`, `AbstractFullDataNetworkRequestQueue`).
/// A Distant Horizons server (Fabric, NeoForge...) then streams its LODs,
/// pre-generated or generated on request, to this client.
#[allow(dead_code)]
pub mod net {
    use super::{FullDataSource, IdMap, full_point, gen_step, pos};
    use std::collections::{HashMap, HashSet};
    use std::time::{Duration, Instant};

    /// `ModInfo.RESOURCE_NAMESPACE:WRAPPER_PACKET_PATH`.
    pub const CHANNEL: &str = "distant_horizons:msg";
    /// `ModInfo.PROTOCOL_VERSION` of the Distant Horizons 3.3 releases.
    pub const PROTOCOL_VERSION: i16 = 16;
    /// `FullDataPointIdMap.BLOCK_STATE_SEPARATOR_STRING`.
    const BLOCK_STATE_SEPARATOR: &str = "_DH-BSW_";

    pub type Result<T> = std::result::Result<T, String>;

    /// A Netty `ByteBuf`, read: big-endian, as Java's `DataInput`.
    pub struct Reader<'a> {
        data: &'a [u8],
        at: usize,
    }
    impl<'a> Reader<'a> {
        pub fn new(data: &'a [u8]) -> Self {
            Self { data, at: 0 }
        }
        pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
            let s = self.data.get(self.at..self.at + n).ok_or("buffer underflow")?;
            self.at += n;
            Ok(s)
        }
        pub fn u8(&mut self) -> Result<u8> {
            Ok(self.bytes(1)?[0])
        }
        pub fn bool(&mut self) -> Result<bool> {
            Ok(self.u8()? != 0)
        }
        pub fn u16(&mut self) -> Result<u16> {
            Ok(u16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
        }
        pub fn i32(&mut self) -> Result<i32> {
            Ok(i32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
        }
        pub fn i64(&mut self) -> Result<i64> {
            Ok(i64::from_be_bytes(self.bytes(8)?.try_into().unwrap()))
        }
        /// `INetworkObject.readString`: an unsigned short length, UTF-8.
        pub fn string(&mut self) -> Result<String> {
            let n = usize::from(self.u16()?);
            Ok(String::from_utf8_lossy(self.bytes(n)?).into_owned())
        }
        pub fn remaining(&self) -> usize {
            self.data.len() - self.at
        }
    }

    #[derive(Default)]
    pub struct Writer(pub Vec<u8>);
    impl Writer {
        pub fn u8(&mut self, v: u8) {
            self.0.push(v);
        }
        pub fn bool(&mut self, v: bool) {
            self.0.push(u8::from(v));
        }
        pub fn i16(&mut self, v: i16) {
            self.0.extend_from_slice(&v.to_be_bytes());
        }
        pub fn i32(&mut self, v: i32) {
            self.0.extend_from_slice(&v.to_be_bytes());
        }
        pub fn i64(&mut self, v: i64) {
            self.0.extend_from_slice(&v.to_be_bytes());
        }
        pub fn string(&mut self, v: &str) {
            let b = v.as_bytes();
            self.0.extend_from_slice(&(b.len().min(65535) as u16).to_be_bytes());
            self.0.extend_from_slice(&b[..b.len().min(65535)]);
        }
    }

    /// `EDhApiGeneratorPlan`'s ordinals.
    pub mod plan {
        pub const SURFACE_THEN_CHUNKS: u8 = 0;
        pub const SURFACE_ONLY: u8 = 1;
        pub const CHUNKS_ONLY: u8 = 2;
        pub const DISABLED: u8 = 3;
    }

    /// `SessionConfig`: its values in registration order. The client sends
    /// what it would like, the server answers with what it allows.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SessionConfig {
        pub generator_plan: u8,
        pub max_generation_request_distance: i32,
        pub generation_center_chunk_x: i32,
        pub generation_center_chunk_z: i32,
        pub generation_max_chunk_radius: i32,
        pub generation_request_rate_limit: i32,
        pub real_time_updates: bool,
        pub real_time_update_radius: i32,
        pub synchronize_on_load: bool,
        pub max_sync_on_load_distance: i32,
        pub sync_on_load_rate_limit: i32,
        pub player_bandwidth_limit: i32,
    }
    impl SessionConfig {
        /// Everything Distant Horizons' own client would allow at most: the
        /// server's limits decide (each value is the server's and this
        /// one's minimum, or the server's own).
        pub fn client() -> Self {
            Self {
                generator_plan: plan::SURFACE_THEN_CHUNKS,
                max_generation_request_distance: 4096,
                generation_center_chunk_x: 0,
                generation_center_chunk_z: 0,
                generation_max_chunk_radius: 0,
                generation_request_rate_limit: 100,
                real_time_updates: true,
                real_time_update_radius: 4096,
                synchronize_on_load: true,
                max_sync_on_load_distance: 4096,
                sync_on_load_rate_limit: 100,
                player_bandwidth_limit: 0,
            }
        }
        fn encode(&self, w: &mut Writer) {
            w.u8(self.generator_plan);
            w.i32(self.max_generation_request_distance);
            w.i32(self.generation_center_chunk_x);
            w.i32(self.generation_center_chunk_z);
            w.i32(self.generation_max_chunk_radius);
            w.i32(self.generation_request_rate_limit);
            w.bool(self.real_time_updates);
            w.i32(self.real_time_update_radius);
            w.bool(self.synchronize_on_load);
            w.i32(self.max_sync_on_load_distance);
            w.i32(self.sync_on_load_rate_limit);
            w.i32(self.player_bandwidth_limit);
        }
        fn decode(r: &mut Reader) -> Result<Self> {
            Ok(Self {
                generator_plan: r.u8()?,
                max_generation_request_distance: r.i32()?,
                generation_center_chunk_x: r.i32()?,
                generation_center_chunk_z: r.i32()?,
                generation_max_chunk_radius: r.i32()?,
                generation_request_rate_limit: r.i32()?,
                real_time_updates: r.bool()?,
                real_time_update_radius: r.i32()?,
                synchronize_on_load: r.bool()?,
                max_sync_on_load_distance: r.i32()?,
                sync_on_load_rate_limit: r.i32()?,
                player_bandwidth_limit: r.i32()?,
            })
        }
    }

    /// `ExceptionMessage.EXCEPTION_LIST`, by index.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Refusal {
        RateLimited,
        OutOfRange,
        Rejected,
        RequiresSplitting,
        Other(i32),
    }

    /// `FullDataPayload`: names the buffer the data came in
    /// (`FullDataSplitMessage`s before it). Its beacon beams are not kept.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Payload {
        pub buffer: i32,
    }
    impl Payload {
        fn decode(r: &mut Reader) -> Result<Self> {
            let buffer = r.i32()?;
            let beams = r.i32()?;
            // `BeaconBeamDTO`: a `DhBlockPos` (three ints) and a colour.
            r.bytes(usize::try_from(beams).map_err(|_| "bad beacon count")? * 16)?;
            Ok(Self { buffer })
        }
        fn encode(&self, w: &mut Writer) {
            w.i32(self.buffer);
            w.i32(0);
        }
    }

    /// The messages of `MessageRegistry`, by id (1 to 10, in its order).
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Message {
        CloseReason(String),
        LevelInit {
            dimension: String,
            server_key: String,
            level_key: String,
            server_time: i64,
        },
        RequestLevelInit(String),
        SessionConfig(SessionConfig),
        Cancel {
            future: i32,
        },
        Exception {
            future: i32,
            refusal: Refusal,
            message: String,
        },
        Request {
            future: i32,
            level: String,
            section: i64,
            timestamp: Option<i64>,
        },
        Response {
            future: i32,
            payload: Option<Payload>,
        },
        PartialUpdate {
            level: String,
            payload: Payload,
        },
        Split {
            buffer: i32,
            data: Vec<u8>,
            first: bool,
        },
    }

    /// What a payload on the channel holds.
    #[derive(Debug, PartialEq, Eq)]
    pub enum Decoded {
        Message(Message),
        /// A message of another protocol version.
        Incompatible(i16),
    }

    /// `AbstractPluginPacketSender.encodeMessage`.
    pub fn encode(message: &Message) -> Vec<u8> {
        let mut w = Writer::default();
        w.i16(PROTOCOL_VERSION);
        let id: i16 = match message {
            Message::CloseReason(_) => 1,
            Message::LevelInit { .. } => 2,
            Message::RequestLevelInit(_) => 3,
            Message::SessionConfig(_) => 4,
            Message::Cancel { .. } => 5,
            Message::Exception { .. } => 6,
            Message::Request { .. } => 7,
            Message::Response { .. } => 8,
            Message::PartialUpdate { .. } => 9,
            Message::Split { .. } => 10,
        };
        w.i16(id);
        match message {
            Message::CloseReason(reason) => w.string(reason),
            Message::LevelInit {
                dimension,
                server_key,
                level_key,
                server_time,
            } => {
                w.string(dimension);
                w.string(server_key);
                w.string(level_key);
                w.i64(*server_time);
            }
            Message::RequestLevelInit(dimension) => w.string(dimension),
            Message::SessionConfig(config) => config.encode(&mut w),
            Message::Cancel { future } => w.i32(*future),
            Message::Exception {
                future,
                refusal,
                message,
            } => {
                w.i32(*future);
                w.i32(match refusal {
                    Refusal::RateLimited => 0,
                    Refusal::OutOfRange => 1,
                    Refusal::Rejected => 2,
                    Refusal::RequiresSplitting => 3,
                    Refusal::Other(i) => *i,
                });
                w.string(message);
            }
            Message::Request {
                future,
                level,
                section,
                timestamp,
            } => {
                w.i32(*future);
                w.string(level);
                w.i64(*section);
                w.bool(timestamp.is_some());
                if let Some(t) = timestamp {
                    w.i64(*t);
                }
            }
            Message::Response { future, payload } => {
                w.i32(*future);
                w.bool(payload.is_some());
                if let Some(p) = payload {
                    p.encode(&mut w);
                }
            }
            Message::PartialUpdate { level, payload } => {
                w.string(level);
                payload.encode(&mut w);
            }
            Message::Split { buffer, data, first } => {
                w.i32(*buffer);
                w.i32(data.len() as i32);
                w.0.extend_from_slice(data);
                w.bool(*first);
            }
        }
        w.0
    }

    /// `AbstractPluginPacketSender.decodeMessage`.
    pub fn decode(data: &[u8]) -> Result<Decoded> {
        let mut r = Reader::new(data);
        let version = r.u16()? as i16;
        if version != PROTOCOL_VERSION {
            return Ok(Decoded::Incompatible(version));
        }
        let message = match r.u16()? {
            1 => Message::CloseReason(r.string()?),
            2 => Message::LevelInit {
                dimension: r.string()?,
                server_key: r.string()?,
                level_key: r.string()?,
                server_time: r.i64()?,
            },
            3 => Message::RequestLevelInit(r.string()?),
            4 => Message::SessionConfig(SessionConfig::decode(&mut r)?),
            5 => Message::Cancel { future: r.i32()? },
            6 => Message::Exception {
                future: r.i32()?,
                refusal: match r.i32()? {
                    0 => Refusal::RateLimited,
                    1 => Refusal::OutOfRange,
                    2 => Refusal::Rejected,
                    3 => Refusal::RequiresSplitting,
                    i => Refusal::Other(i),
                },
                message: r.string()?,
            },
            7 => {
                let future = r.i32()?;
                let level = r.string()?;
                let section = r.i64()?;
                let timestamp = if r.bool()? { Some(r.i64()?) } else { None };
                Message::Request {
                    future,
                    level,
                    section,
                    timestamp,
                }
            }
            8 => {
                let future = r.i32()?;
                let payload = if r.bool()? {
                    Some(Payload::decode(&mut r)?)
                } else {
                    None
                };
                Message::Response { future, payload }
            }
            9 => Message::PartialUpdate {
                level: r.string()?,
                payload: Payload::decode(&mut r)?,
            },
            10 => {
                let buffer = r.i32()?;
                let size = usize::try_from(r.i32()?).map_err(|_| "bad split size")?;
                let data = r.bytes(size)?.to_vec();
                Message::Split {
                    buffer,
                    data,
                    first: r.bool()?,
                }
            }
            id => return Err(format!("unknown message id {id}")),
        };
        if r.remaining() != 0 {
            return Err("message not fully read".into());
        }
        Ok(Decoded::Message(message))
    }

    // ---- full data sources ----

    /// `EDhApiDataCompressionMode`.
    fn decompress(mode: u8, blob: &[u8]) -> Result<Vec<u8>> {
        match mode {
            0 => Ok(blob.to_vec()),
            1 => super::lz4::decompress(blob),
            // Z_STD_STREAM (deprecated) writes the same frames.
            2 | 4 => super::zstd::decompress(blob),
            3 => {
                Err("the server compresses with LZMA2, which this port can't read: set its compression to Z_STD".into())
            }
            m => Err(format!("unknown compression mode {m}")),
        }
    }

    /// A full data source as a server sent it: Distant Horizons' data
    /// points, which name entries of `mapping` (biome and block state, as
    /// the server serializes them).
    #[derive(Clone, Debug, PartialEq)]
    pub struct RemoteSection {
        pub pos: i64,
        pub columns: Vec<Vec<u64>>,
        pub gen_steps: Vec<i8>,
        pub mapping: Vec<(String, String)>,
    }

    const WIDTH: i32 = pos::WIDTH;
    const COLUMNS: usize = (WIDTH * WIDTH) as usize;

    /// The section's position, the first field of its bytes.
    pub fn section_pos(dto: &[u8]) -> Option<i64> {
        Some(i64::from_be_bytes(dto.get(..8)?.try_into().ok()?))
    }

    /// `FullDataSourceV2DTO.decode` then `populateDataSource`.
    pub fn decode_section(dto: &[u8]) -> Result<RemoteSection> {
        let mut r = Reader::new(dto);
        let section = r.i64()?;
        let _checksum = r.i32()?;
        let mut blob = || -> Result<&[u8]> {
            let n = usize::try_from(r.i32()?).map_err(|_| "bad blob size")?;
            r.bytes(n)
        };
        let data = blob()?;
        let adjacent = [blob()?, blob()?, blob()?, blob()?];
        let steps = blob()?;
        let _world_compression = blob()?;
        let mapping = blob()?;
        let format = r.u8()?;
        let mode = r.u8()?;
        // applyToParent, applyToChildren, regenerate; last modified, created.
        r.bytes(3 + 8 + 8)?;
        let mut columns = vec![Vec::new(); COLUMNS];
        match format {
            1 => read_columns_v1(&decompress(mode, data)?, &mut columns)?,
            2 => {
                read_columns_v2(&decompress(mode, data)?, &mut columns, (1, WIDTH - 1, 1, WIDTH - 1))?;
                // North, south, east and west edges (`FullDataMinMaxPosUtil`).
                let edges = [
                    (0, WIDTH, 0, 1),
                    (0, WIDTH, WIDTH - 1, WIDTH),
                    (WIDTH - 1, WIDTH, 0, WIDTH),
                    (0, 1, 0, WIDTH),
                ];
                for (blob, edge) in adjacent.into_iter().zip(edges) {
                    read_columns_v2(&decompress(mode, blob)?, &mut columns, edge)?;
                }
            }
            f => return Err(format!("unknown data format {f}")),
        }
        let steps = decompress(mode, steps)?;
        if steps.len() < COLUMNS {
            return Err("generation steps cut short".into());
        }
        let gen_steps = steps[..COLUMNS].iter().map(|&b| b as i8).collect();
        let mapping = read_mapping(&decompress(mode, mapping)?)?;
        let count = mapping.len() as u64;
        if columns.iter().flatten().any(|&p| u64::from(full_point::id(p)) >= count) {
            return Err("a data point names no mapping entry".into());
        }
        Ok(RemoteSection {
            pos: section,
            columns,
            gen_steps,
            mapping,
        })
    }

    fn read_columns_v1(raw: &[u8], columns: &mut [Vec<u64>]) -> Result<()> {
        let mut r = Reader::new(raw);
        for column in columns.iter_mut() {
            let n = r.u16()? as i16;
            if n < 0 {
                return Err("negative column length".into());
            }
            *column = (0..n).map(|_| r.i64().map(|v| v as u64)).collect::<Result<_>>()?;
        }
        Ok(())
    }

    /// `VarintUtil.readVarint`.
    fn varint(r: &mut Reader) -> Result<u32> {
        let mut value = 0u32;
        let mut shift = 0;
        loop {
            if shift >= 32 {
                return Err("invalid varint".into());
            }
            let b = r.u8()?;
            value |= u32::from(b & 127) << shift;
            shift += 7;
            if b & 128 == 0 {
                return Ok(value);
            }
        }
    }

    /// `readBlobToDataSourceDataArrayV2`: the columns of `x0..x1, z0..z1`,
    /// each property in its own pass over every point.
    fn read_columns_v2(raw: &[u8], columns: &mut [Vec<u64>], (x0, x1, z0, z1): (i32, i32, i32, i32)) -> Result<()> {
        let mut r = Reader::new(raw);
        let region: Vec<usize> = (x0..x1)
            .flat_map(|x| (z0..z1).map(move |z| super::index(x, z)))
            .collect();
        // 1. Column lengths. 2. Ids, with a light flag and a flag for a
        // bottom other than the one predicted.
        let mut counts = Vec::with_capacity(region.len());
        for _ in &region {
            let n = varint(&mut r)? as usize;
            if n > 4096 {
                return Err("column too long".into());
            }
            counts.push(n);
        }
        let total: usize = counts.iter().sum();
        let mut ids = Vec::with_capacity(total);
        for _ in 0..total {
            ids.push(varint(&mut r)?);
        }
        // 3. Heights.
        let mut heights = Vec::with_capacity(total);
        for _ in 0..total {
            heights.push(varint(&mut r)? as i32);
        }
        // 4. Bottoms: each predicted from the one before (its bottom minus
        // this height), the mis-predicted ones corrected.
        let mut bottoms = Vec::with_capacity(total);
        let mut previous = 0i32;
        for i in 0..total {
            let error = if ids[i] & 1 != 0 {
                let z = varint(&mut r)?;
                ((z >> 1) as i32) ^ -((z & 1) as i32)
            } else {
                0
            };
            let bottom = previous - heights[i] + error;
            bottoms.push(bottom);
            previous = bottom;
        }
        // 5. Light, for the lit points: block light high, sky low.
        let mut at = 0;
        for (column, &n) in region.iter().zip(&counts) {
            let mut points = Vec::with_capacity(n);
            for i in at..at + n {
                let (block, sky) = if ids[i] & 2 != 0 {
                    let packed = r.u8()?;
                    (packed >> 4, packed & 15)
                } else {
                    (0, 0)
                };
                let point =
                    full_point::encode(ids[i] >> 2, heights[i], bottoms[i], block, sky).ok_or("invalid data point")?;
                points.push(point);
            }
            columns[*column] = points;
            at += n;
        }
        Ok(())
    }

    /// `FullDataPointIdMap.deserialize`: an int count, then each entry as a
    /// Java "UTF" string, `<biome>_DH-BSW_<block state>`.
    fn read_mapping(raw: &[u8]) -> Result<Vec<(String, String)>> {
        let mut r = Reader::new(raw);
        let count = r.i32()?;
        if count < 0 {
            return Err("negative mapping size".into());
        }
        let mut out = Vec::with_capacity(count.min(4096) as usize);
        for _ in 0..count {
            let entry = r.string()?;
            let (biome, block) = entry
                .split_once(BLOCK_STATE_SEPARATOR)
                .ok_or("mapping entry without separator")?;
            out.push((biome.to_owned(), block.to_owned()));
        }
        Ok(out)
    }

    /// `BlockStateWrapper.deserialize`'s input: `AIR`, or
    /// `namespace:path_STATE_{name:value}{name:value}`. Returns the name and
    /// properties (`None` for air).
    pub fn parse_block_state(serial: &str) -> Option<(&str, Vec<(&str, &str)>)> {
        if serial == "AIR" || serial.is_empty() {
            return None;
        }
        let (name, properties) = serial.split_once("_STATE_").unwrap_or((serial, ""));
        let properties = properties
            .split('}')
            .filter_map(|p| p.strip_prefix('{'))
            .filter_map(|p| p.split_once(':'))
            .filter(|&(_, v)| v != "NULL")
            .collect();
        Some((name, properties))
    }

    impl RemoteSection {
        /// Into this port's data: each mapping entry resolved to the game's
        /// biome and block state, every bottom moved by `y_offset` (the
        /// server's minimum y less the game's).
        pub fn into_source(self, mut resolve: impl FnMut(&str, &str) -> (u16, u32), y_offset: i32) -> FullDataSource {
            let mut source = FullDataSource::empty(self.pos);
            let mut mapping = IdMap::default();
            let ids: Vec<u32> = self
                .mapping
                .iter()
                .map(|(biome, block)| {
                    let (b, s) = resolve(biome, block);
                    mapping.add_if_not_present(b, s)
                })
                .collect();
            source.mapping = mapping;
            for (i, column) in self.columns.into_iter().enumerate() {
                let step = self.gen_steps[i];
                if step == gen_step::EMPTY && column.is_empty() {
                    continue;
                }
                let points: Vec<u64> = column
                    .into_iter()
                    .filter_map(|p| {
                        let id = ids[full_point::id(p) as usize];
                        let bottom = full_point::bottom_y(p) + y_offset;
                        full_point::encode(
                            id,
                            full_point::height(p),
                            bottom,
                            full_point::block_light(p),
                            full_point::sky_light(p),
                        )
                    })
                    .collect();
                source.columns[i] = points;
                source.gen_steps[i] = step;
                source.is_empty = false;
            }
            source
        }
    }

    // ---- the client ----

    /// How the connection to the server's Distant Horizons stands.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Status {
        /// Asked; no answer yet.
        Waiting,
        /// No answer: the server has no Distant Horizons.
        Absent,
        Ready,
        /// The server shares no LODs (generation and sync both off).
        Disabled,
        Incompatible(i16),
        Closed(String),
    }

    /// A request sent and not answered yet.
    struct InFlight {
        section: i64,
        sent: Instant,
    }

    /// No answer this long: the server has no Distant Horizons.
    const ABSENT_AFTER: Duration = Duration::from_secs(15);
    /// No level key this long after the config: the server sends none
    /// (`levelKeys.send` off); the dimension's name is the level's.
    const LEVEL_KEY_WAIT: Duration = Duration::from_secs(3);
    /// A request unanswered this long is cancelled and asked again later.
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
    /// A section out of the server's range is asked again after this.
    const OUT_OF_RANGE_RETRY: Duration = Duration::from_secs(30);
    /// Split buffers whose message never came are dropped past this many.
    const MAX_BUFFERS: usize = 512;

    /// `ClientNetworkState` with a request queue: the handshake, then full
    /// data requests for the sections the player should see, nearest first,
    /// within the server's limits. Driven from the game thread: `receive`
    /// what arrives on the channel, `tick` once a frame, then send
    /// `take_outgoing` and give `take_sections` to the engine.
    pub struct Client {
        dimension: String,
        started: Instant,
        heard: bool,
        status: Status,
        config: Option<(SessionConfig, Instant)>,
        level: Option<String>,
        /// Ask with a timestamp of 0 (`syncOnLoad`): the server generates
        /// nothing, it sends what it has.
        sync_only: bool,
        next_future: i32,
        in_flight: HashMap<i32, InFlight>,
        /// Sections asked for (and not to be asked again), answered or not.
        asked: HashSet<i64>,
        /// Smaller sections asked for in place of one the server would not
        /// send whole (`SectionRequiresSplittingException`).
        split: HashSet<i64>,
        retry: HashMap<i64, Instant>,
        /// Requests sent in the current second (`SupplierBasedRateLimiter`).
        window: (Instant, i32),
        paused_until: Option<Instant>,
        buffers: HashMap<i32, Vec<u8>>,
        buffer_order: Vec<i32>,
        outgoing: Vec<Vec<u8>>,
        sections: Vec<Vec<u8>>,
        pub received: usize,
        pub empty: usize,
        pub refused: usize,
        pub errors: usize,
        pub last_error: Option<String>,
    }

    impl Client {
        /// Starts the handshake for the level of `dimension` (its id, such
        /// as `minecraft:overworld`): the config to agree on, and the
        /// level's key.
        pub fn new(dimension: &str, now: Instant) -> Self {
            let mut c = Self {
                dimension: dimension.to_owned(),
                started: now,
                heard: false,
                status: Status::Waiting,
                config: None,
                level: None,
                sync_only: false,
                next_future: 0,
                in_flight: HashMap::new(),
                asked: HashSet::new(),
                split: HashSet::new(),
                retry: HashMap::new(),
                window: (now, 0),
                paused_until: None,
                buffers: HashMap::new(),
                buffer_order: Vec::new(),
                outgoing: Vec::new(),
                sections: Vec::new(),
                received: 0,
                empty: 0,
                refused: 0,
                errors: 0,
                last_error: None,
            };
            c.send(Message::RequestLevelInit(c.dimension.clone()));
            c.send(Message::SessionConfig(SessionConfig::client()));
            c
        }

        fn send(&mut self, m: Message) {
            self.outgoing.push(encode(&m));
        }

        pub fn status(&self) -> &Status {
            &self.status
        }
        pub fn config(&self) -> Option<SessionConfig> {
            self.config.map(|(c, _)| c)
        }
        pub fn level(&self) -> Option<&str> {
            self.level.as_deref()
        }
        pub fn in_flight(&self) -> usize {
            self.in_flight.len()
        }
        /// Payloads to send on `CHANNEL`.
        pub fn take_outgoing(&mut self) -> Vec<Vec<u8>> {
            std::mem::take(&mut self.outgoing)
        }
        /// Received full data sources (`FullDataSourceV2DTO` bytes), for
        /// `decode_section`.
        pub fn take_sections(&mut self) -> Vec<Vec<u8>> {
            std::mem::take(&mut self.sections)
        }

        fn error(&mut self, e: String) {
            self.errors += 1;
            self.last_error = Some(e);
        }

        /// A payload that came on `CHANNEL`.
        pub fn receive(&mut self, data: &[u8], now: Instant) {
            self.heard = true;
            let message = match decode(data) {
                Ok(Decoded::Message(m)) => m,
                Ok(Decoded::Incompatible(v)) => {
                    self.status = Status::Incompatible(v);
                    return;
                }
                Err(e) => return self.error(e),
            };
            if matches!(self.status, Status::Closed(_) | Status::Incompatible(_)) {
                return;
            }
            match message {
                Message::CloseReason(reason) => {
                    self.status = Status::Closed(reason);
                    self.in_flight.clear();
                }
                Message::LevelInit {
                    dimension, level_key, ..
                } => {
                    if dimension == self.dimension {
                        self.level = Some(level_key);
                    }
                }
                Message::SessionConfig(config) => {
                    let generation = config.generator_plan != plan::DISABLED;
                    self.sync_only = !generation;
                    self.status = if generation || config.synchronize_on_load {
                        Status::Ready
                    } else {
                        Status::Disabled
                    };
                    self.config = Some((config, now));
                }
                Message::Split { buffer, data, first } => {
                    if first {
                        self.buffers.insert(buffer, data);
                        self.buffer_order.push(buffer);
                        if self.buffer_order.len() > MAX_BUFFERS {
                            let old = self.buffer_order.remove(0);
                            self.buffers.remove(&old);
                        }
                    } else if let Some(b) = self.buffers.get_mut(&buffer) {
                        b.extend_from_slice(&data);
                    }
                }
                Message::Response { future, payload } => {
                    let Some(request) = self.in_flight.remove(&future) else {
                        return;
                    };
                    self.split.remove(&request.section);
                    match payload {
                        Some(p) => match self.take_buffer(p.buffer) {
                            Some(dto) => {
                                self.received += 1;
                                self.sections.push(dto);
                            }
                            None => {
                                self.error("a response's data never came".into());
                                self.asked.remove(&request.section);
                            }
                        },
                        None => self.empty += 1,
                    }
                }
                Message::PartialUpdate { level, payload } => {
                    let dto = self.take_buffer(payload.buffer);
                    if let Some(dto) = dto.filter(|_| self.level.as_deref() == Some(level.as_str())) {
                        self.sections.push(dto);
                    }
                }
                Message::Exception {
                    future,
                    refusal,
                    message,
                } => {
                    let Some(request) = self.in_flight.remove(&future) else {
                        return;
                    };
                    let s = request.section;
                    match refusal {
                        Refusal::RequiresSplitting if pos::detail_level(s) > pos::SECTION_MINIMUM_DETAIL_LEVEL => {
                            self.split.remove(&s);
                            for i in 0..4 {
                                self.split.insert(pos::child(s, i));
                            }
                        }
                        Refusal::RateLimited => {
                            self.asked.remove(&s);
                            self.paused_until = Some(now + Duration::from_secs(1));
                        }
                        Refusal::OutOfRange => {
                            self.asked.remove(&s);
                            self.retry.insert(s, now + OUT_OF_RANGE_RETRY);
                        }
                        Refusal::Rejected if !self.sync_only => {
                            // Generation is off there: ask for what it has.
                            self.sync_only = true;
                            self.asked.remove(&s);
                        }
                        _ => {
                            self.refused += 1;
                            self.last_error = Some(message);
                        }
                    }
                }
                Message::RequestLevelInit(_) | Message::Request { .. } | Message::Cancel { .. } => {}
            }
        }

        fn take_buffer(&mut self, buffer: i32) -> Option<Vec<u8>> {
            let b = self.buffers.remove(&buffer)?;
            self.buffer_order.retain(|&x| x != buffer);
            Some(b)
        }

        /// Once a frame: the status, and requests for `wanted` (the
        /// sections to draw) around `player` (block x, z).
        pub fn tick(&mut self, now: Instant, wanted: &[i64], player: [f64; 2]) {
            if self.status == Status::Waiting && !self.heard && now - self.started > ABSENT_AFTER {
                self.status = Status::Absent;
            }
            if self.status == Status::Absent && self.heard {
                self.status = Status::Waiting;
            }
            let Some((config, configured)) = self.config else {
                return;
            };
            if self.status != Status::Ready {
                return;
            }
            if self.level.is_none() && now - configured >= LEVEL_KEY_WAIT {
                self.level = Some(self.dimension.clone());
            }
            let Some(level) = self.level.clone() else { return };
            // Requests the server forgot.
            let stale: Vec<i32> = self
                .in_flight
                .iter()
                .filter(|(_, r)| now - r.sent > REQUEST_TIMEOUT)
                .map(|(&f, _)| f)
                .collect();
            for future in stale {
                if let Some(r) = self.in_flight.remove(&future) {
                    self.asked.remove(&r.section);
                    self.send(Message::Cancel { future });
                }
            }
            if self.paused_until.is_some_and(|t| now < t) {
                return;
            }
            let limit = if self.sync_only {
                config.sync_on_load_rate_limit
            } else {
                config.generation_request_rate_limit
            }
            .clamp(1, 100);
            if now - self.window.0 >= Duration::from_secs(1) {
                self.window = (now, 0);
            }
            let room = (limit - self.in_flight.len() as i32).min(limit - self.window.1);
            if room <= 0 {
                return;
            }
            let max_distance = if self.sync_only {
                config.max_sync_on_load_distance
            } else {
                config.max_generation_request_distance
            };
            let (px, pz) = (player[0].floor() as i32, player[1].floor() as i32);
            let distance = |s: i64| pos::chebyshev_signed_block_distance(s, px, pz);
            self.retry.retain(|_, t| now < *t);
            let mut candidates: Vec<(i32, i64)> = wanted
                .iter()
                .chain(self.split.iter())
                .copied()
                .filter(|s| !self.asked.contains(s) && !self.retry.contains_key(s))
                .map(|s| (distance(s), s))
                .filter(|&(d, _)| d / 16 <= max_distance)
                .collect();
            candidates.sort_unstable();
            candidates.dedup();
            let timestamp = self.sync_only.then_some(0);
            for (_, section) in candidates.into_iter().take(room as usize) {
                let future = self.next_future;
                // Bit 31 says who asked: 0, the client.
                self.next_future = (self.next_future + 1) & i32::MAX;
                // A split part stays in `split` until answered: refused
                // for now (rate, range), it is asked again from there.
                self.asked.insert(section);
                self.in_flight.insert(future, InFlight { section, sent: now });
                self.window.1 += 1;
                self.send(Message::Request {
                    future,
                    level: level.clone(),
                    section,
                    timestamp,
                });
            }
        }

        /// One line for `/dh`.
        pub fn describe(&self) -> String {
            let state = match &self.status {
                Status::Waiting => "waiting for the server".to_owned(),
                Status::Absent => "the server has no Distant Horizons".to_owned(),
                Status::Disabled => "the server shares no LODs".to_owned(),
                Status::Incompatible(v) => format!(
                    "the server's Distant Horizons speaks protocol {v}, this port {PROTOCOL_VERSION}: {}",
                    if v > &PROTOCOL_VERSION {
                        "update this mod"
                    } else {
                        "the server's is older"
                    }
                ),
                Status::Closed(reason) => format!("closed by the server: {reason}"),
                Status::Ready => format!(
                    "{} ({})",
                    if self.sync_only {
                        "sending what it has"
                    } else {
                        "generating on request"
                    },
                    self.level.as_deref().unwrap_or("level key pending")
                ),
            };
            let mut line = format!(
                "server LODs: {state}; {} received, {} asked, {} in flight",
                self.received,
                self.asked.len(),
                self.in_flight.len()
            );
            if self.errors > 0 || self.refused > 0 {
                line += &format!(", {} errors, {} refused", self.errors, self.refused);
            }
            if let Some(e) = &self.last_error {
                line += &format!(" (last: {e})");
            }
            line
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AIR: u32 = 0;
    const STONE: u32 = 1;
    const GRASS: u32 = 2;
    const WATER: u32 = 3;
    const FLOWER: u32 = 4;
    const SNOW: u32 = 5;

    fn blocks() -> BlockTable {
        let solid = BlockInfo {
            opacity: 16,
            solid: true,
            material: material::STONE,
            alpha: 255,
            ..BlockInfo::default()
        };
        BlockTable {
            states: vec![
                BlockInfo {
                    air: true,
                    material: material::AIR,
                    ..BlockInfo::default()
                },
                solid,
                BlockInfo {
                    material: material::GRASS,
                    ..solid
                },
                BlockInfo {
                    opacity: 1,
                    liquid: true,
                    material: material::WATER,
                    alpha: 0xA0,
                    ..BlockInfo::default()
                },
                BlockInfo {
                    alpha: 255,
                    material: material::LEAVES,
                    ..BlockInfo::default()
                },
                BlockInfo {
                    alpha: 255,
                    material: material::SNOW,
                    snow_layer: true,
                    ..BlockInfo::default()
                },
            ],
        }
    }

    /// A chunk of stone up to `height(x, z)` (exclusive), grass on top.
    struct FlatChunk {
        x: i32,
        z: i32,
        height: fn(i32, i32) -> i32,
    }
    impl ChunkView for FlatChunk {
        fn chunk_x(&self) -> i32 {
            self.x
        }
        fn chunk_z(&self) -> i32 {
            self.z
        }
        fn min_build_height(&self) -> i32 {
            -64
        }
        fn max_build_height(&self) -> i32 {
            320
        }
        fn min_non_empty_height(&self) -> i32 {
            -64
        }
        fn top(&self, x: i32, z: i32) -> i32 {
            (self.height)(self.x * 16 + x, self.z * 16 + z)
        }
        fn state(&self, x: i32, y: i32, z: i32) -> u32 {
            let h = (self.height)(self.x * 16 + x, self.z * 16 + z);
            if y >= h {
                AIR
            } else if y == h - 1 {
                GRASS
            } else {
                STONE
            }
        }
        fn biome(&self, _: i32, _: i32, _: i32) -> u16 {
            1
        }
        fn light(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
            if y >= self.top(x, z) { (15, 0) } else { (0, 0) }
        }
    }

    #[test]
    fn section_positions_pack_as_distant_horizons_does() {
        let p = pos::encode(10, -3, 7);
        assert_eq!((pos::detail_level(p), pos::x(p), pos::z(p)), (10, -3, 7));
        // DhSectionPos.encode(10, -3, 7), computed with the Java code.
        assert_eq!(p, ((7i64) << 36) | ((-3i64 & ((1 << 28) - 1)) << 8) | 10);
        assert_eq!(pos::min_corner_block_x(p), -3 * 1024);
        assert_eq!(pos::center_block_z(p), 7 * 1024 + 512);
        assert_eq!(pos::parent(pos::child(p, 3)), p);
        assert_eq!(pos::containing_block(6, -1, 64), pos::encode(6, -1, 1));
        assert_eq!(pos::containing_block(8, -1, 64), pos::encode(8, -1, 0));
        assert!(pos::contains(p, pos::child(pos::child(p, 2), 1)));
        assert_eq!(pos::to_string(p), "10*-3,7");
    }

    #[test]
    fn full_data_points_keep_their_fields() {
        let p = full_point::encode(1234, 70, 64, 9, 15).unwrap();
        assert_eq!(full_point::id(p), 1234);
        assert_eq!(full_point::height(p), 70);
        assert_eq!(full_point::bottom_y(p), 64);
        assert_eq!(full_point::block_light(p), 9);
        assert_eq!(full_point::sky_light(p), 15);
        assert_eq!(full_point::top_y(full_point::set_height(p, 3)), 67);
        assert!(full_point::encode(1, 0, 0, 0, 0).is_none());
        assert!(full_point::encode(1, 1, 0, 16, 0).is_none());
    }

    #[test]
    fn a_chunk_becomes_runs_from_the_top_down() {
        let chunk = FlatChunk {
            x: 5,
            z: -1,
            height: |_, _| 64,
        };
        let source = create_from_chunk(&chunk, &blocks());
        assert_eq!(source.pos, pos::encode(6, 1, -1));
        // Chunk 5 is the second of its section, chunk -1 the last.
        let column = source.column(16, 48);
        let runs: Vec<_> = column
            .iter()
            .map(|&p| {
                (
                    source.mapping.state(full_point::id(p)),
                    full_point::bottom_y(p),
                    full_point::height(p),
                )
            })
            .collect();
        // Air down to 64, grass at 63, stone to the bottom (relative to -64).
        // The air run starts at the exclusive build limit, one above the
        // world, as `createFromChunk`'s `lastY` does.
        assert_eq!(runs, vec![(AIR, 128, 257), (GRASS, 127, 1), (STONE, 0, 127)]);
        assert_eq!(full_point::sky_light(column[1]), 15);
        assert_eq!(source.gen_steps[index(16, 48)], gen_step::LIGHT);
        assert_eq!(source.gen_steps[index(0, 0)], gen_step::EMPTY);
    }

    #[test]
    fn two_by_two_columns_vote_for_their_block() {
        let b = blocks();
        let mut child = FullDataSource::empty(pos::encode(6, 0, 0));
        let id = |m: &mut IdMap, s| m.add_if_not_present(1, s);
        for (i, height) in [10, 10, 12, 10].into_iter().enumerate() {
            let (x, z) = ((i & 1) as i32, (i >> 1) as i32);
            let air = id(&mut child.mapping, AIR);
            let stone = id(&mut child.mapping, STONE);
            let column = vec![
                full_point::encode(air, 384 - height, height, 0, 15).unwrap(),
                full_point::encode(stone, height, 0, 0, 0).unwrap(),
            ];
            child.set_column(x, z, column, gen_step::LIGHT);
        }
        let mut parent = FullDataSource::empty(pos::encode(7, 0, 0));
        assert!(parent.update_from(&child, &b));
        let merged = parent.column(0, 0);
        let runs: Vec<_> = merged
            .iter()
            .map(|&p| {
                (
                    parent.mapping.state(full_point::id(p)),
                    full_point::bottom_y(p),
                    full_point::top_y(p),
                )
            })
            .collect();
        // Air never wins a slice: the one column of stone to 12 keeps it.
        assert_eq!(runs, vec![(AIR, 12, 384), (STONE, 0, 12)]);
        assert_eq!(parent.gen_steps[index(0, 0)], gen_step::LIGHT);
        // The other columns of the child were empty: left as they were.
        assert_eq!(parent.gen_steps[index(1, 0)], gen_step::EMPTY);
    }

    #[test]
    fn less_complete_data_never_replaces_more_complete() {
        let b = blocks();
        let chunk = FlatChunk {
            x: 0,
            z: 0,
            height: |_, _| 64,
        };
        let mut section = create_from_chunk(&chunk, &b);
        let before = section.clone();
        let mut generated = FullDataSource::empty(section.pos);
        let stone = generated.mapping.add_if_not_present(1, STONE);
        for x in 0..pos::WIDTH {
            for z in 0..pos::WIDTH {
                generated.set_column(
                    x,
                    z,
                    vec![full_point::encode(stone, 10, 0, 0, 15).unwrap()],
                    gen_step::SURFACE,
                );
            }
        }
        assert!(section.update_from(&generated, &b));
        assert_eq!(section.column(3, 3).len(), before.column(3, 3).len());
        assert_eq!(section.gen_steps[index(3, 3)], gen_step::LIGHT);
        assert_eq!(section.gen_steps[index(40, 40)], gen_step::SURFACE);
        assert_eq!(section.missing_columns(), 0);
    }

    #[test]
    fn buried_runs_are_culled_into_the_run_above() {
        let b = blocks();
        let mut source = FullDataSource::empty(pos::encode(6, 0, 0));
        let mut input = FullDataSource::empty(source.pos);
        let air = input.mapping.add_if_not_present(1, AIR);
        let grass = input.mapping.add_if_not_present(1, GRASS);
        let stone = input.mapping.add_if_not_present(1, STONE);
        for x in 0..3 {
            for z in 0..3 {
                let column = vec![
                    full_point::encode(air, 300, 84, 0, 15).unwrap(),
                    full_point::encode(grass, 4, 80, 0, 15).unwrap(),
                    full_point::encode(stone, 80, 0, 0, 0).unwrap(),
                ];
                input.set_column(x, z, column, gen_step::LIGHT);
            }
        }
        source.update_from(&input, &b);
        // The middle column's grass is surrounded and covered above by air:
        // translucent, so it stays; the stone at the bottom is the last run.
        assert_eq!(source.column(1, 1).len(), 3);
        // Three opaque runs: the middle one is hidden on every side.
        let mut input = FullDataSource::empty(source.pos);
        let ids = [AIR, GRASS, STONE, GRASS].map(|s| input.mapping.add_if_not_present(1, s));
        for x in 0..3 {
            for z in 0..3 {
                let column = vec![
                    full_point::encode(ids[0], 300, 84, 0, 15).unwrap(),
                    full_point::encode(ids[1], 4, 80, 0, 15).unwrap(),
                    full_point::encode(ids[2], 40, 40, 0, 0).unwrap(),
                    full_point::encode(ids[3], 40, 0, 0, 0).unwrap(),
                ];
                input.set_column(x, z, column, gen_step::LIGHT);
            }
        }
        let mut source = FullDataSource::empty(input.pos);
        source.update_from(&input, &b);
        let middle = source.column(1, 1);
        assert_eq!(middle.len(), 3);
        assert_eq!(full_point::bottom_y(middle[1]), 40);
        assert_eq!(full_point::height(middle[1]), 44);
        assert_eq!(source.column(0, 0).len(), 4);
    }

    #[test]
    fn the_detail_plan_drops_detail_with_distance() {
        let plan = DetailPlan::new(HorizontalQuality::Medium, 256);
        let player = [8.0, 8.0];
        let near = pos::containing_block(6, 100, 0);
        let far = pos::containing_block(6, 3000, 0);
        // Medium: 192-block units, base 2.
        assert_eq!(plan.expected_detail_level(player, near), 6);
        assert_eq!(plan.expected_detail_level(player, far), 6 + 3);
        let sections = plan.sections(player, |_| false);
        assert!(!sections.is_empty());
        // The sections tile the area without overlapping.
        for (i, &a) in sections.iter().enumerate() {
            for &b in &sections[i + 1..] {
                assert!(
                    !pos::contains(a, b) && !pos::contains(b, a),
                    "{} {}",
                    pos::to_string(a),
                    pos::to_string(b)
                );
            }
        }
        let area: i64 = sections.iter().map(|&s| i64::from(pos::block_width(s)).pow(2)).sum();
        assert!(area >= i64::from(plan.radius * 2).pow(2), "{area}");
        // Nearest sections are the finest.
        let nearest = sections
            .iter()
            .min_by_key(|&&s| pos::chebyshev_signed_block_distance(s, 8, 8))
            .unwrap();
        assert_eq!(pos::detail_level(*nearest), 6);
        assert!(sections.iter().any(|&s| pos::detail_level(s) >= 9));
    }

    #[test]
    fn render_columns_merge_and_carry_snow() {
        let b = blocks();
        let mut mapping = IdMap::default();
        let ids = [AIR, SNOW, GRASS, STONE, FLOWER, WATER].map(|s| mapping.add_if_not_present(1, s));
        let settings = RenderSettings {
            cave_culling: true,
            cave_culling_height: 60,
            min_y: -64,
        };
        let column = vec![
            full_point::encode(ids[0], 100, 129, 0, 15).unwrap(),
            full_point::encode(ids[1], 1, 128, 0, 15).unwrap(),
            full_point::encode(ids[2], 1, 127, 0, 15).unwrap(),
            full_point::encode(ids[3], 127, 0, 0, 0).unwrap(),
        ];
        let r = render_column(&column, &mapping, &b, &settings);
        // Snow is not drawn itself: the grass under it takes its id.
        assert_eq!(r.points.len(), 2);
        assert_eq!(render_point::id(r.points[0]), ids[1]);
        assert_eq!(render_point::y_max(r.points[0]), 128);
        assert_eq!(render_point::id(r.points[1]), ids[3]);
        // A flower is left out; water keeps its alpha.
        let column = vec![
            full_point::encode(ids[0], 100, 130, 0, 15).unwrap(),
            full_point::encode(ids[4], 1, 129, 0, 15).unwrap(),
            full_point::encode(ids[5], 2, 127, 0, 15).unwrap(),
            full_point::encode(ids[3], 127, 0, 0, 0).unwrap(),
        ];
        let r = render_column(&column, &mapping, &b, &settings);
        assert_eq!(r.points.len(), 2);
        assert_eq!(render_point::id(r.points[0]), ids[5]);
        assert!(render_point::alpha(r.points[0]) < 255);
        assert!(render_point::is_fluid(r.points[0]));
    }

    #[test]
    fn a_flat_section_meshes_to_one_top_face() {
        let b = blocks();
        let mut source = FullDataSource::empty(pos::encode(6, 0, 0));
        let mut mapping = IdMap::default();
        let air = mapping.add_if_not_present(1, AIR);
        let grass = mapping.add_if_not_present(1, GRASS);
        source.mapping = mapping;
        for x in 0..pos::WIDTH {
            for z in 0..pos::WIDTH {
                let column = vec![
                    full_point::encode(air, 200, 128, 0, 15).unwrap(),
                    full_point::encode(grass, 128, 0, 0, 15).unwrap(),
                ];
                source.set_column(x, z, column, gen_step::LIGHT);
            }
        }
        let settings = RenderSettings {
            cave_culling: true,
            cave_culling_height: 60,
            min_y: -64,
        };
        let render = RenderSource::from_full(&source, &b, &settings);
        let quads = build_quads(&render, [None; 4], &settings, |_, _| false);
        let up: Vec<_> = quads.iter().filter(|q| q.dir == Dir::Up).collect();
        assert_eq!(up.len(), 1);
        assert_eq!(
            (up[0].plane, up[0].u0, up[0].v0, up[0].u1, up[0].v1),
            (64, 0, 0, 64, 64)
        );
        // The bottom over the void, and the four outer walls.
        assert_eq!(quads.len(), 6, "{quads:?}");
        let walls = quads.iter().filter(|q| q.dir == Dir::North).count();
        assert_eq!(walls, 1);
        // Inner columns hide each other's sides: a hole leaves walls round it.
        // Merged by rows, then rows into bands: the rows above and below the
        // hole, and the two halves of its own row.
        let quads = build_quads(&render, [None; 4], &settings, |x, z| x == 10 && z == 10);
        assert_eq!(quads.iter().filter(|q| q.dir == Dir::Up).count(), 4);
        assert_eq!(quads.iter().filter(|q| q.dir == Dir::Down).count(), 4);
        assert_eq!(quads.len(), 4 + 4 + 4 + 4);
    }

    #[test]
    fn side_faces_are_cut_by_the_adjacent_column() {
        let b = blocks();
        let mut mapping = IdMap::default();
        let air = mapping.add_if_not_present(1, AIR);
        let stone = mapping.add_if_not_present(1, STONE);
        let mut source = FullDataSource::empty(pos::encode(6, 0, 0));
        source.mapping = mapping;
        for x in 0..pos::WIDTH {
            for z in 0..pos::WIDTH {
                let h = if x < 32 { 100 } else { 90 };
                let column = vec![
                    full_point::encode(air, 384 - h, h, 0, 15).unwrap(),
                    full_point::encode(stone, h, 0, 0, 15).unwrap(),
                ];
                source.set_column(x, z, column, gen_step::LIGHT);
            }
        }
        let settings = RenderSettings {
            cave_culling: false,
            cave_culling_height: 60,
            min_y: -64,
        };
        let render = RenderSource::from_full(&source, &b, &settings);
        let quads = build_quads(&render, [None; 4], &settings, |_, _| false);
        // The step between the two heights: one east face, 10 blocks tall.
        let step: Vec<_> = quads.iter().filter(|q| q.dir == Dir::East && q.plane == 32).collect();
        assert_eq!(step.len(), 1);
        assert_eq!(
            (step[0].v0, step[0].v1, step[0].u0, step[0].u1),
            (90 - 64, 100 - 64, 0, 64)
        );
    }

    #[test]
    fn sections_round_trip_through_their_files() {
        let chunk = FlatChunk {
            x: 2,
            z: 3,
            height: |x, z| 60 + (x + z) % 7,
        };
        let source = create_from_chunk(&chunk, &blocks());
        let bytes = storage::encode(&source, 42);
        assert_eq!(storage::decode(&bytes, 42), Some(source.clone()));
        assert_eq!(storage::decode(&bytes, 43), None);
        assert_eq!(storage::decode(&bytes[..bytes.len() - 4], 42), None);
        let dir = std::env::temp_dir().join(format!("dh-test-{}", std::process::id()));
        storage::save(&dir, &source, 7).unwrap();
        assert_eq!(storage::load(&dir, source.pos, 7), Some(source));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_read_back_what_they_store() {
        let mut s = Settings::default();
        s.click(1);
        s.click(2);
        s.click(5);
        let stored: std::collections::HashMap<_, _> = s.stored().into_iter().collect();
        let read = Settings::read(|k| stored.get(k).cloned());
        assert_eq!(read, s);
        assert_eq!(read.radius_chunks(), 512);
        assert_eq!(read.quality, HorizontalQuality::High);
        assert!(read.debug);
        assert_eq!(Settings::read(|_| Some("junk".into())), Settings::default());
        let mut c = Settings::default();
        assert!(c.command(&["distance", "100"]).is_some());
        assert_eq!(c.radius_chunks(), 128);
        assert!(c.command(&["quality", "extreme"]).is_some());
        assert!(c.command(&["nonsense"]).is_none());
    }

    #[test]
    fn every_completed_command_is_one_the_mod_runs() {
        // `status`, `save` and `clear` act on the session (bridge.rs); every
        // other completion must change a setting.
        for &(word, values) in COMMANDS {
            if matches!(word, "status" | "save" | "clear") {
                assert!(values.is_empty());
                continue;
            }
            let runs = |args: &[&str]| Settings::default().command(args).is_some();
            if values.is_empty() {
                assert!(runs(&[word]), "{word}");
            }
            for value in values {
                assert!(runs(&[word, value]), "{word} {value}");
            }
        }
        // The distances offered are the settings' own.
        let distance = COMMANDS.iter().find(|c| c.0 == "distance").unwrap().1;
        assert_eq!(
            distance,
            &DISTANCES
                .map(|d| d.to_string())
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()[..]
        );
    }

    struct FakeHost {
        blocks: BlockTable,
    }
    struct FakeSampler;
    impl ColumnSampler for FakeSampler {
        fn column(&mut self, x: i32, z: i32, out: &mut Vec<SampledRun>) {
            let h = 60 + ((x / 64 + z / 64).rem_euclid(5));
            out.push(SampledRun {
                min: -64,
                max: h,
                state: GRASS,
                biome: 1,
                sky: 15,
                block: 0,
            });
        }
    }
    impl Host for FakeHost {
        type Mesh = usize;
        fn blocks(&self) -> &BlockTable {
            &self.blocks
        }
        fn build_height(&self) -> (i32, i32) {
            (-64, 320)
        }
        fn sampler(&self) -> Option<Box<dyn ColumnSampler>> {
            Some(Box::new(FakeSampler))
        }
        fn mesh(&self, _: i64, _: &FullDataSource, quads: &[Quad], _: bool) -> usize {
            quads.len()
        }
        fn resolve(&self, biome: &str, block: &str) -> (u16, u32) {
            resolve(biome, block)
        }
    }

    #[test]
    fn the_engine_generates_and_meshes_every_wanted_section() {
        let settings = engine::WorkSettings {
            render: RenderSettings {
                cave_culling: true,
                cave_culling_height: 60,
                min_y: -64,
            },
            generation: true,
            debug: false,
        };
        let dir = std::env::temp_dir().join(format!("dh-engine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plan = DetailPlan::new(HorizontalQuality::Low, 64);
        let mut wanted = 0;
        {
            let mut e = engine::Engine::new(FakeHost { blocks: blocks() }, Some(dir.clone()), 1, settings, 2);
            // As a world loading does: hundreds of chunks at once, which
            // must not starve the sections in view.
            for x in -12..=12 {
                for z in -12..=12 {
                    e.add_chunk(Box::new(FlatChunk {
                        x,
                        z,
                        height: |_, _| 70,
                    }));
                }
            }
            let start = std::time::Instant::now();
            let mut meshed = std::collections::HashSet::new();
            while start.elapsed() < std::time::Duration::from_secs(60) {
                let (meshes, _) = e.update([8.0, 8.0], plan, engine::Near::default());
                // A neighbour of the chunk may mesh blank before it is
                // generated; its generation meshes it again.
                for m in meshes {
                    if m.mesh.unwrap_or(0) > 0 {
                        meshed.insert(m.section);
                    } else {
                        meshed.remove(&m.section);
                    }
                }
                wanted = e.stats().wanted;
                if wanted > 0 && meshed.len() == wanted && e.stats().chunks == 625 && e.stats().queued == 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(wanted > 0);
            assert_eq!(meshed.len(), wanted);
            assert_eq!(e.stats().chunks, 625);
            assert_eq!(e.stats().queued, 0);
        }
        // Dropping the engine saved every section it made.
        let files = |d: &str| std::fs::read_dir(dir.join(d)).map(|d| d.count()).unwrap_or(0);
        assert!(files("6") > 0);
        // The chunks reached the coarsest level.
        assert!(files("14") > 0);
        let _ = std::fs::remove_dir_all(dir);
    }
    // ---- server LODs ----

    /// The block states and biomes of the sections below, as the tests'
    /// game knows them.
    fn resolve(biome: &str, block: &str) -> (u16, u32) {
        let biome = match biome {
            "minecraft:plains" => 1,
            "minecraft:forest" => 2,
            _ => 0,
        };
        let state = match net::parse_block_state(block).map(|(name, _)| name) {
            Some("minecraft:stone") => STONE,
            Some("minecraft:grass_block") => GRASS,
            Some("minecraft:water") => WATER,
            Some("minecraft:oak_leaves") => FLOWER,
            _ => AIR,
        };
        (biome, state)
    }

    const MAPPING: [&str; 5] = [
        "minecraft:plains_DH-BSW_AIR",
        "minecraft:plains_DH-BSW_minecraft:grass_block_STATE_{snowy:false}",
        "minecraft:plains_DH-BSW_minecraft:stone_STATE_",
        "minecraft:forest_DH-BSW_minecraft:oak_leaves_STATE_{distance:7}{persistent:false}{waterlogged:false}",
        "minecraft:river_DH-BSW_minecraft:water_STATE_{level:0}",
    ];

    /// A column as a Distant Horizons server keeps it, from the top down:
    /// air, sometimes leaves over a gap, grass or water, stone. Some columns
    /// are empty.
    fn server_column(x: i32, z: i32) -> Vec<u64> {
        if (x * 7 + z * 3) % 23 == 0 {
            return Vec::new();
        }
        let ground = 130 + (x + z) % 9;
        let p = |id, height, bottom, block, sky| full_point::encode(id, height, bottom, block, sky).unwrap();
        let mut column = Vec::new();
        if x % 5 == 0 {
            column.push(p(0, 384 - ground - 12, ground + 12, 0, 15));
            // Leaves over a gap (a discontinuity) lit by a torch.
            column.push(p(3, 4, ground + 6, 14, 12));
        } else {
            column.push(p(0, 384 - ground - 1, ground + 1, 0, 15));
        }
        let top = if z % 4 == 0 { 4 } else { 1 };
        column.push(p(top, 1, ground, 0, if top == 4 { 13 } else { 0 }));
        column.push(p(2, ground, 0, 0, 0));
        column
    }

    fn varint(out: &mut Vec<u8>, mut v: u32) {
        while v >= 128 {
            out.push(v as u8 | 128);
            v >>= 7;
        }
        out.push(v as u8);
    }

    /// `writeDataSourceDataArrayToBlobV2`, uncompressed.
    fn server_blob(columns: &[Vec<u64>], (x0, x1, z0, z1): (i32, i32, i32, i32)) -> Vec<u8> {
        let region: Vec<&Vec<u64>> = (x0..x1)
            .flat_map(|x| (z0..z1).map(move |z| &columns[index(x, z)]))
            .collect();
        let mut out = Vec::new();
        for c in &region {
            varint(&mut out, c.len() as u32);
        }
        let mut previous = 0;
        for &p in region.iter().copied().flatten() {
            let lit = full_point::block_light(p) | full_point::sky_light(p) != 0;
            let discontinuous = full_point::bottom_y(p) != previous - full_point::height(p);
            previous = full_point::bottom_y(p);
            varint(
                &mut out,
                full_point::id(p) << 2 | u32::from(lit) << 1 | u32::from(discontinuous),
            );
        }
        for &p in region.iter().copied().flatten() {
            varint(&mut out, full_point::height(p) as u32);
        }
        previous = 0;
        for &p in region.iter().copied().flatten() {
            let expected = previous - full_point::height(p);
            let error = full_point::bottom_y(p) - expected;
            if error != 0 {
                varint(&mut out, ((error << 1) ^ (error >> 31)) as u32);
            }
            previous = full_point::bottom_y(p);
        }
        for &p in region.iter().copied().flatten() {
            let packed = full_point::block_light(p) << 4 | full_point::sky_light(p);
            if packed != 0 {
                out.push(packed);
            }
        }
        out
    }

    /// `FullDataSourceV2DTO.encode` of a section built from `server_column`,
    /// uncompressed (mode 0).
    fn server_dto(section: i64) -> (Vec<Vec<u64>>, Vec<i8>, Vec<u8>) {
        let mut columns = vec![Vec::new(); 4096];
        let mut steps = vec![gen_step::EMPTY; 4096];
        for x in 0..64 {
            for z in 0..64 {
                columns[index(x, z)] = server_column(x, z);
                if !columns[index(x, z)].is_empty() {
                    steps[index(x, z)] = if x < 32 { gen_step::LIGHT } else { gen_step::SURFACE };
                }
            }
        }
        let mut w = net::Writer::default();
        w.i64(section);
        w.i32(1234);
        let blob = |w: &mut net::Writer, b: Vec<u8>| {
            w.i32(b.len() as i32);
            w.0.extend_from_slice(&b);
        };
        blob(&mut w, server_blob(&columns, (1, 63, 1, 63)));
        for edge in [(0, 64, 0, 1), (0, 64, 63, 64), (63, 64, 0, 64), (0, 1, 0, 64)] {
            blob(&mut w, server_blob(&columns, edge));
        }
        blob(&mut w, steps.iter().map(|&s| s as u8).collect());
        blob(&mut w, vec![0; 4096]);
        let mut mapping = net::Writer::default();
        mapping.i32(MAPPING.len() as i32);
        for m in MAPPING {
            mapping.string(m);
        }
        blob(&mut w, mapping.0);
        w.u8(2);
        w.u8(0);
        w.bool(false);
        w.bool(true);
        w.bool(false);
        w.i64(1_700_000_000_000);
        w.i64(1_600_000_000_000);
        (columns, steps, w.0)
    }

    #[test]
    fn zstd_reads_what_libzstd_wrote() {
        // A skewed alphabet with repeated words: Huffman literals and FSE
        // sequences, compressed by libzstd at level 19.
        let mut x: u64 = 1;
        let mut raw = Vec::new();
        let words: [&[u8]; 4] = [b"stone ", b"grass_block ", b"minecraft:", b"water "];
        for _ in 0..1500 {
            x = (x * 1103515245 + 12345) % (1 << 31);
            if (x >> 8).is_multiple_of(7) {
                raw.extend_from_slice(words[((x >> 12) % 4) as usize]);
            } else {
                raw.push(b"aaaabbbccd efghi"[((x >> 16) % 16) as usize]);
            }
        }
        let hex: String = [
            "28b52ffd60b80ba51d007a42600715b0a59a0300348c5bb32c9696369109270fcdb5641e76006d006c005f1d62f1e9bc",
            "9e77122ca7b491597dc4f47ca865c93ba54324f5f89cb07aa4f37bff31fdaf9d593dacfae6ca67d74ca5920538d085ce",
            "e008843d92ee642f9d71aca5133509e7cdcfb31232322caabaf1ee448c313566403818acd7f9cc106a487bd342a08401",
            "06180c17eaee4447ce5198c305331b035f4ecda44dfcc27e945bb6b3338d0df54a3fa3e31d37c999f82c8a21c22ac55c",
            "b5521b1585e4d125298c252c8fc616cdd6fae9b951571e1ff5933f5af58b54efb7743d8da6457d502b5ab2cb7883d374",
            "d790bc7ac64e44ac62378d5395c1674da7696651cce99847bde2d7f60321fcb183a688da59549277243b9f2d5f889e0b",
            "8d144366a8cb3771830fadb774cd8f114292630d65b682ec8bc74cad23766d3dfee790f2e31dce54ae908d749d58bc2b",
            "8b4face0fd337af663d6a7c4f8da3c6af1b2843294d2c798ae72e437e53d29e1fcf8b3668b38517f12fdb2b346d668c6",
            "63db2955d3ff43b7d3b8884a6d8c9a8f2be2267b45eb947a1693c8f09b8c67499d8e26557e57382d9e7e27e556d5f7e9",
            "cc22d94b57627536fbc3be56992d13d35aa906cb868874444ed590accbcd8ec20ab269748a2d27a4d85d9292e5eaa80c",
            "7eb39790f16a80d7a8b12ce60c42ca8cc8d024493a2044400c73761e11584c449065d68224293406568fdf38e11c1281",
            "12468dcac6f72889f199cf6cf39aa10d00cc865fadfa98f4860ad9a467b8c847074070116d8e477661d4c091535a232b",
            "2bfcd043d954743071ba86a75450d67ad9a8cf3f7388dbfbb51ed74edd4be5b76eef40b4059dec30641554317892165c",
            "f5eed6239245918a0e67a8998258fe4104b33679d80a7b17d7c4a9ff1e18399c0049d8bff0077692ba1cb785360558cc",
            "acf95b423ccca7ebac3a03787c3ceee8be9646e0b9c0bb6ba03886040b3f8c421a966405511ec8b5894026c31bc8017b",
            "b2853991e35f13bf3586936ade84ff469943da14dee6ca6bb0434b82b71aeffa6b20b3fc510a22e179a97cc3c8e47013",
            "d03493d6c0fbb4d997c38196154cc533d28f6604e6d251947cca40203f1c7b5194c8dc83ec8cc88c4982949dea6f9e6b",
            "c1a697df9a3f2e63d96b573b89d028c060094e2e28011bb26748c215dfde024179c3daa225809f6e156cfcda83a8c3d6",
            "caf1240d10adfb62a4ece5bb94a957c23c03c553db7c06204d53115e4fa021b4e99f665c96dd8cd103ef4722db941854",
            "dcc25d70dbfc252396b2c3376d067260cfa44ceda7724f02ddb6163b86a1f4205eeeba7a358e34ce86e2bf964303",
        ]
        .concat();
        let compressed: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(zstd::decompress(&compressed).unwrap(), raw);
        // A raw block and an RLE block, in one frame each, one after another.
        let frames = [
            &[0x28, 0xB5, 0x2F, 0xFD, 0x20, 3, 0x19, 0, 0, b'a', b'b', b'c'][..],
            &[0x28, 0xB5, 0x2F, 0xFD, 0x20, 5, 0x2B, 0, 0, b'z'][..],
        ]
        .concat();
        assert_eq!(zstd::decompress(&frames).unwrap(), b"abczzzzz");
        assert!(zstd::decompress(&compressed[..compressed.len() - 3]).is_err());
    }

    #[test]
    fn lz4_reads_frames() {
        // "abcabcabcabc!" as literals "abc" and a match of 9 at offset 3.
        let block = [0x35, b'a', b'b', b'c', 3, 0, 0x10, b'!'];
        let mut frame = vec![0x04, 0x22, 0x4D, 0x18, 0x60, 0x40, 0x82];
        frame.extend_from_slice(&(block.len() as u32).to_le_bytes());
        frame.extend_from_slice(&block);
        // An uncompressed block, then the end mark.
        frame.extend_from_slice(&(0x8000_0002u32).to_le_bytes());
        frame.extend_from_slice(b"..");
        frame.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(lz4::decompress(&frame).unwrap(), b"abcabcabcabc!..");
    }

    #[test]
    fn messages_round_trip_as_distant_horizons_encodes_them() {
        let messages = [
            net::Message::CloseReason("bye".into()),
            net::Message::LevelInit {
                dimension: "minecraft:overworld".into(),
                server_key: "".into(),
                level_key: "abc@minecraft:overworld".into(),
                server_time: 42,
            },
            net::Message::RequestLevelInit("minecraft:the_nether".into()),
            net::Message::SessionConfig(net::SessionConfig::client()),
            net::Message::Cancel { future: 9 },
            net::Message::Exception {
                future: 3,
                refusal: net::Refusal::RequiresSplitting,
                message: "split".into(),
            },
            net::Message::Request {
                future: 5,
                level: "abc@minecraft:overworld".into(),
                section: pos::encode(8, -3, 4),
                timestamp: Some(0),
            },
            net::Message::Response {
                future: 5,
                payload: Some(net::Payload { buffer: 77 }),
            },
            net::Message::PartialUpdate {
                level: "x".into(),
                payload: net::Payload { buffer: 1 },
            },
            net::Message::Split {
                buffer: 77,
                data: vec![1, 2, 3],
                first: true,
            },
        ];
        for m in messages {
            assert_eq!(net::decode(&net::encode(&m)).unwrap(), net::Decoded::Message(m));
        }
        // A request as the Java client writes it, byte for byte: protocol
        // 16, message 7, future id, level name, position, no timestamp.
        let request = net::encode(&net::Message::Request {
            future: 1,
            level: "a".into(),
            section: 0x0102,
            timestamp: None,
        });
        assert_eq!(
            request,
            [0, 16, 0, 7, 0, 0, 0, 1, 0, 1, b'a', 0, 0, 0, 0, 0, 0, 1, 2, 0]
        );
        // The session config: a byte, nine ints and two booleans.
        assert_eq!(
            net::encode(&net::Message::SessionConfig(net::SessionConfig::client())).len(),
            4 + 39
        );
        assert_eq!(net::decode(&[0, 17, 0, 1]).unwrap(), net::Decoded::Incompatible(17));
    }

    #[test]
    fn a_servers_section_reads_back_point_for_point() {
        let section = pos::encode(6, 3, -2);
        let (columns, steps, dto) = server_dto(section);
        assert_eq!(net::section_pos(&dto), Some(section));
        let remote = net::decode_section(&dto).unwrap();
        assert_eq!(remote.pos, section);
        assert_eq!(remote.columns, columns);
        assert_eq!(remote.gen_steps, steps);
        assert_eq!(remote.mapping[3].0, "minecraft:forest");
        assert_eq!(
            remote.mapping[3].1,
            "minecraft:oak_leaves_STATE_{distance:7}{persistent:false}{waterlogged:false}"
        );
        assert_eq!(
            net::parse_block_state(&remote.mapping[3].1),
            Some((
                "minecraft:oak_leaves",
                vec![("distance", "7"), ("persistent", "false"), ("waterlogged", "false")]
            ))
        );
        assert_eq!(
            net::parse_block_state("minecraft:stone_STATE_"),
            Some(("minecraft:stone", vec![]))
        );
        assert_eq!(net::parse_block_state("AIR"), None);
        // Into the game's ids, 64 blocks up (a Nether server's 0 is the
        // game's -64).
        let source = remote.into_source(resolve, 64);
        let column = source.column(5, 4);
        assert_eq!(source.mapping.get(full_point::id(column[1])), (2, FLOWER));
        assert_eq!(full_point::block_light(column[1]), 14);
        assert_eq!(
            full_point::bottom_y(column[1]),
            full_point::bottom_y(columns[index(5, 4)][1]) + 64
        );
        assert_eq!(source.gen_steps[index(40, 1)], gen_step::SURFACE);
        assert!(source.column(0, 0).is_empty());
        // Cut short anywhere, it is refused, not misread.
        assert!(net::decode_section(&dto[..dto.len() - 1]).is_err());
    }

    fn requests(c: &mut net::Client) -> Vec<(i32, i64, Option<i64>)> {
        c.take_outgoing()
            .into_iter()
            .filter_map(|b| match net::decode(&b).unwrap() {
                net::Decoded::Message(net::Message::Request {
                    future,
                    section,
                    timestamp,
                    level,
                }) => {
                    assert_eq!(level, "seed@minecraft:overworld");
                    Some((future, section, timestamp))
                }
                _ => None,
            })
            .collect()
    }

    fn refuse(c: &mut net::Client, future: i32, refusal: net::Refusal, at: std::time::Instant) {
        let m = net::Message::Exception {
            future,
            refusal,
            message: String::new(),
        };
        c.receive(&net::encode(&m), at);
    }

    #[test]
    fn the_client_shakes_hands_then_asks_nearest_first_within_the_limits() {
        use std::time::Duration;
        let t0 = std::time::Instant::now();
        let mut c = net::Client::new("minecraft:overworld", t0);
        let hello: Vec<_> = c.take_outgoing().iter().map(|b| net::decode(b).unwrap()).collect();
        assert_eq!(
            hello,
            [
                net::Decoded::Message(net::Message::RequestLevelInit("minecraft:overworld".into())),
                net::Decoded::Message(net::Message::SessionConfig(net::SessionConfig::client())),
            ]
        );
        // Far to near along x, all at detail 8 (256 blocks).
        let wanted: Vec<i64> = (0..6).rev().map(|x| pos::encode(8, x, 0)).collect();
        c.tick(t0, &wanted, [0.0, 0.0]);
        assert!(requests(&mut c).is_empty());
        assert_eq!(c.status(), &net::Status::Waiting);
        let config = net::SessionConfig {
            generation_request_rate_limit: 3,
            sync_on_load_rate_limit: 3,
            ..net::SessionConfig::client()
        };
        c.receive(&net::encode(&net::Message::SessionConfig(config)), t0);
        let init = net::Message::LevelInit {
            dimension: "minecraft:overworld".into(),
            server_key: String::new(),
            level_key: "seed@minecraft:overworld".into(),
            server_time: 0,
        };
        c.receive(&net::encode(&init), t0);
        c.tick(t0, &wanted, [0.0, 0.0]);
        let sent = requests(&mut c);
        assert_eq!(
            sent.iter().map(|r| r.1).collect::<Vec<_>>(),
            [0, 1, 2].map(|x| pos::encode(8, x, 0))
        );
        assert!(sent.iter().all(|r| r.2.is_none()));
        // The limit holds until answers come.
        c.tick(t0 + Duration::from_secs(2), &wanted, [0.0, 0.0]);
        assert!(requests(&mut c).is_empty());

        // The first comes in two pieces, then its response.
        let (_, _, dto) = server_dto(sent[0].1);
        let (a, b) = dto.split_at(1000);
        for (data, first) in [(a, true), (b, false)] {
            let m = net::Message::Split {
                buffer: 4,
                data: data.to_vec(),
                first,
            };
            c.receive(&net::encode(&m), t0);
        }
        let response = net::Message::Response {
            future: sent[0].0,
            payload: Some(net::Payload { buffer: 4 }),
        };
        c.receive(&net::encode(&response), t0);
        assert_eq!(c.take_sections(), [dto]);
        // The second must be split: its four children are asked for.
        refuse(&mut c, sent[1].0, net::Refusal::RequiresSplitting, t0);
        // The third: generation is off there; it is asked again for what
        // the server has.
        refuse(&mut c, sent[2].0, net::Refusal::Rejected, t0);
        let t1 = t0 + Duration::from_secs(3);
        c.tick(t1, &wanted, [0.0, 0.0]);
        let next = requests(&mut c);
        assert_eq!(next.len(), 3);
        assert!(next.iter().all(|r| r.2 == Some(0)));
        // The children are nearer than the third section, asked again.
        let children: Vec<i64> = (0..4).map(|i| pos::child(pos::encode(8, 1, 0), i)).collect();
        assert!(next.iter().all(|r| children.contains(&r.1)));
        // Two answered with nothing (the server has none there): room for
        // two more.
        for r in &next[1..] {
            let m = net::Message::Response {
                future: r.0,
                payload: None,
            };
            c.receive(&net::encode(&m), t1);
        }
        c.tick(t1 + Duration::from_secs(1), &wanted, [0.0, 0.0]);
        let more: Vec<i64> = requests(&mut c).iter().map(|r| r.1).collect();
        assert!(more.contains(&pos::encode(8, 2, 0)));
        // Rate limited: paused, the section asked again later.
        refuse(&mut c, next[0].0, net::Refusal::RateLimited, t1);
        c.tick(t1, &wanted, [0.0, 0.0]);
        assert!(requests(&mut c).is_empty());
        c.tick(t1 + Duration::from_secs(2), &wanted, [0.0, 0.0]);
        assert!(requests(&mut c).iter().any(|r| r.1 == next[0].1));
        // Closed by the server: nothing more.
        c.receive(&net::encode(&net::Message::CloseReason("bye".into())), t1);
        c.tick(t1 + Duration::from_secs(9), &wanted, [0.0, 0.0]);
        assert!(requests(&mut c).is_empty());
        assert!(c.describe().contains("bye"));
    }

    #[test]
    fn without_an_answer_the_server_has_no_distant_horizons() {
        let t0 = std::time::Instant::now();
        let mut c = net::Client::new("minecraft:overworld", t0);
        c.tick(t0 + std::time::Duration::from_secs(20), &[], [0.0, 0.0]);
        assert_eq!(c.status(), &net::Status::Absent);
    }

    #[test]
    fn the_engine_draws_what_a_server_sent() {
        let settings = engine::WorkSettings {
            render: RenderSettings {
                cave_culling: true,
                cave_culling_height: 60,
                min_y: -64,
            },
            generation: false,
            debug: false,
        };
        let plan = DetailPlan::new(HorizontalQuality::Low, 64);
        let mut e = engine::Engine::new(FakeHost { blocks: blocks() }, None, 1, settings, 2);
        let section = pos::encode(6, 0, 0);
        let start = std::time::Instant::now();
        let mut drawn = false;
        let mut sent = false;
        while start.elapsed() < std::time::Duration::from_secs(30) && !drawn {
            let (meshes, _) = e.update([32.0, 32.0], plan, engine::Near::default());
            if !sent && e.wanted_sections().contains(&section) {
                e.add_remote(server_dto(section).2);
                e.add_remote(vec![1, 2, 3]);
                sent = true;
            }
            drawn = meshes.iter().any(|m| m.section == section && m.mesh.unwrap_or(0) > 0);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(drawn);
        assert_eq!(e.stats().received, 1);
        assert_eq!(e.stats().unreadable, 1);
        assert!(e.last_unreadable().is_some());
    }
}
