// Part of Distant Horizons for Horizon Client, LGPL-3.0 (see runtime.rs).

use super::distant_horizons::{FullDataSource, IdMap, full_point, gen_step, index, pos};
use super::distant_horizons_codec as codec;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

pub type Result<T> = std::result::Result<T, String>;

pub const CHANNEL: &str = "distant_horizons:msg";
pub const PROTOCOL_VERSION: i16 = 16;
pub const OLDER_PROTOCOL_VERSION: i16 = 15;

pub struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }
    pub fn remaining(&self) -> usize {
        self.data.len() - self.at
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(format!("message ends early: {n} bytes wanted, {} left", self.remaining()));
        }
        let out = &self.data[self.at..self.at + n];
        self.at += n;
        Ok(out)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }
    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }
    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.array()?))
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.array()?))
    }
    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.array()?))
    }
    pub fn string(&mut self) -> Result<String> {
        let n = self.u16()? as usize;
        Ok(String::from_utf8_lossy(self.bytes(n)?).into_owned())
    }
    pub fn blob(&mut self) -> Result<&'a [u8]> {
        let n = self.i32()?;
        if n < 0 {
            return Err(format!("negative length {n}"));
        }
        self.bytes(n as usize)
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
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    pub fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    pub fn string(&mut self, v: &str) {
        let bytes = &v.as_bytes()[..v.len().min(u16::MAX as usize)];
        self.u16(bytes.len() as u16);
        self.0.extend_from_slice(bytes);
    }
    pub fn blob(&mut self, v: &[u8]) {
        self.i32(v.len() as i32);
        self.0.extend_from_slice(v);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorPlan {
    SurfaceThenChunks,
    SurfaceOnly,
    ChunksOnly,
    Disabled,
}

impl GeneratorPlan {
    const ALL: [Self; 4] = [Self::SurfaceThenChunks, Self::SurfaceOnly, Self::ChunksOnly, Self::Disabled];
    pub fn enabled(self) -> bool {
        self != Self::Disabled
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionConfig {
    pub generator_plan: GeneratorPlan,
    pub max_generation_request_distance: i32,
    pub generation_center_chunk_x: i32,
    pub generation_center_chunk_z: i32,
    pub generation_max_chunk_radius: i32,
    pub generation_request_rate_limit: i32,
    pub enable_real_time_updates: bool,
    pub real_time_update_distance: i32,
    pub synchronize_on_load: bool,
    pub max_sync_on_load_distance: i32,
    pub sync_on_load_rate_limit: i32,
    pub player_bandwidth_limit: i32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            generator_plan: GeneratorPlan::SurfaceThenChunks,
            max_generation_request_distance: 4096,
            generation_center_chunk_x: 0,
            generation_center_chunk_z: 0,
            generation_max_chunk_radius: 0,
            generation_request_rate_limit: 20,
            enable_real_time_updates: true,
            real_time_update_distance: 256,
            synchronize_on_load: true,
            max_sync_on_load_distance: 4096,
            sync_on_load_rate_limit: 50,
            player_bandwidth_limit: 500,
        }
    }
}

impl SessionConfig {
    fn encode(&self, w: &mut Writer, version: i16) {
        if version == OLDER_PROTOCOL_VERSION {
            w.bool(self.generator_plan.enabled());
        } else {
            w.u8(self.generator_plan as u8);
        }
        w.i32(self.max_generation_request_distance);
        w.i32(self.generation_center_chunk_x);
        w.i32(self.generation_center_chunk_z);
        w.i32(self.generation_max_chunk_radius);
        w.i32(self.generation_request_rate_limit);
        w.bool(self.enable_real_time_updates);
        w.i32(self.real_time_update_distance);
        w.bool(self.synchronize_on_load);
        w.i32(self.max_sync_on_load_distance);
        w.i32(self.sync_on_load_rate_limit);
        w.i32(self.player_bandwidth_limit);
    }
    fn decode(r: &mut Reader, version: i16) -> Result<Self> {
        let plan = r.u8()?;
        let generator_plan = if version == OLDER_PROTOCOL_VERSION {
            if plan != 0 { GeneratorPlan::SurfaceThenChunks } else { GeneratorPlan::Disabled }
        } else {
            *GeneratorPlan::ALL
                .get(plan as usize)
                .ok_or_else(|| format!("unknown generator plan {plan}"))?
        };
        Ok(Self {
            generator_plan,
            max_generation_request_distance: r.i32()?,
            generation_center_chunk_x: r.i32()?,
            generation_center_chunk_z: r.i32()?,
            generation_max_chunk_radius: r.i32()?,
            generation_request_rate_limit: r.i32()?,
            enable_real_time_updates: r.bool()?,
            real_time_update_distance: r.i32()?,
            synchronize_on_load: r.bool()?,
            max_sync_on_load_distance: r.i32()?,
            sync_on_load_rate_limit: r.i32()?,
            player_bandwidth_limit: r.i32()?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    RateLimited,
    OutOfRange,
    Rejected,
    RequiresSplitting,
    Other(i32),
}

impl Failure {
    fn from_index(i: i32) -> Self {
        match i {
            0 => Self::RateLimited,
            1 => Self::OutOfRange,
            2 => Self::Rejected,
            3 => Self::RequiresSplitting,
            i => Self::Other(i),
        }
    }
    fn index(self) -> i32 {
        match self {
            Self::RateLimited => 0,
            Self::OutOfRange => 1,
            Self::Rejected => 2,
            Self::RequiresSplitting => 3,
            Self::Other(i) => i,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payload {
    pub buffer_id: i32,
    pub beacons: Vec<([i32; 3], i32)>,
}

impl Payload {
    fn encode(&self, w: &mut Writer) {
        w.i32(self.buffer_id);
        w.i32(self.beacons.len() as i32);
        for &([x, y, z], rgb) in &self.beacons {
            w.i32(x);
            w.i32(y);
            w.i32(z);
            w.i32(rgb);
        }
    }
    fn decode(r: &mut Reader) -> Result<Self> {
        let buffer_id = r.i32()?;
        let n = r.i32()?;
        if n < 0 || n as usize > r.remaining() / 16 {
            return Err(format!("bad beacon count {n}"));
        }
        let beacons = (0..n)
            .map(|_| Ok(([r.i32()?, r.i32()?, r.i32()?], r.i32()?)))
            .collect::<Result<_>>()?;
        Ok(Self { buffer_id, beacons })
    }
}

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
    Cancel(i32),
    Exception {
        future: i32,
        failure: Failure,
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
        buffer_id: i32,
        bytes: Vec<u8>,
        first: bool,
    },
}

impl Message {
    fn id(&self) -> u16 {
        match self {
            Self::CloseReason(_) => 1,
            Self::LevelInit { .. } => 2,
            Self::RequestLevelInit(_) => 3,
            Self::SessionConfig(_) => 4,
            Self::Cancel(_) => 5,
            Self::Exception { .. } => 6,
            Self::Request { .. } => 7,
            Self::Response { .. } => 8,
            Self::PartialUpdate { .. } => 9,
            Self::Split { .. } => 10,
        }
    }

    #[cfg(test)]
    pub fn encode(&self) -> Vec<u8> {
        self.encode_as(PROTOCOL_VERSION)
    }

    pub fn encode_as(&self, version: i16) -> Vec<u8> {
        let mut w = Writer::default();
        w.i16(version);
        w.u16(self.id());
        match self {
            Self::CloseReason(reason) => w.string(reason),
            Self::LevelInit {
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
            Self::RequestLevelInit(dimension) => w.string(dimension),
            Self::SessionConfig(config) => config.encode(&mut w, version),
            Self::Cancel(future) => w.i32(*future),
            Self::Exception {
                future,
                failure,
                message,
            } => {
                w.i32(*future);
                w.i32(failure.index());
                w.string(message);
            }
            Self::Request {
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
            Self::Response { future, payload } => {
                w.i32(*future);
                w.bool(payload.is_some());
                if let Some(p) = payload {
                    p.encode(&mut w);
                }
            }
            Self::PartialUpdate { level, payload } => {
                w.string(level);
                payload.encode(&mut w);
            }
            Self::Split { buffer_id, bytes, first } => {
                w.i32(*buffer_id);
                w.blob(bytes);
                w.bool(*first);
            }
        }
        w.0
    }

    #[cfg(test)]
    pub fn decode(data: &[u8]) -> Result<Self> {
        Self::decode_versioned(data).map(|(_, m)| m)
    }

    pub fn decode_versioned(data: &[u8]) -> Result<(i16, Self)> {
        let mut r = Reader::new(data);
        let version = r.i16()?;
        if version != PROTOCOL_VERSION && version != OLDER_PROTOCOL_VERSION {
            return Err(unsupported(version));
        }
        let id = r.u16()?;
        let message = match id {
            1 => Self::CloseReason(close_reason(r.bytes(r.remaining())?)?),
            2 => Self::LevelInit {
                dimension: r.string()?,
                server_key: r.string()?,
                level_key: r.string()?,
                server_time: r.i64()?,
            },
            3 => Self::RequestLevelInit(r.string()?),
            4 => Self::SessionConfig(SessionConfig::decode(&mut r, version)?),
            5 => Self::Cancel(r.i32()?),
            6 => Self::Exception {
                future: r.i32()?,
                failure: Failure::from_index(r.i32()?),
                message: r.string()?,
            },
            7 => Self::Request {
                future: r.i32()?,
                level: r.string()?,
                section: r.i64()?,
                timestamp: if r.bool()? { Some(r.i64()?) } else { None },
            },
            8 => Self::Response {
                future: r.i32()?,
                payload: if r.bool()? { Some(Payload::decode(&mut r)?) } else { None },
            },
            9 => Self::PartialUpdate {
                level: r.string()?,
                payload: Payload::decode(&mut r)?,
            },
            10 => Self::Split {
                buffer_id: r.i32()?,
                bytes: r.blob()?.to_vec(),
                first: r.bool()?,
            },
            id => return Err(format!("unknown message id {id}")),
        };
        if r.remaining() > 0 {
            return Err(format!("{} bytes left after message {id}", r.remaining()));
        }
        Ok((version, message))
    }
}

fn close_reason(bytes: &[u8]) -> Result<String> {
    let text = match *bytes {
        [a, b, ref rest @ ..] if usize::from(u16::from_be_bytes([a, b])) == rest.len() => rest,
        [a, b, c, d, ref rest @ ..] if u32::from_be_bytes([a, b, c, d]) as usize == rest.len() => rest,
        _ => return Err("unreadable close reason".into()),
    };
    Ok(String::from_utf8_lossy(text).into_owned())
}

fn unsupported(version: i16) -> String {
    format!(
        "the server's Distant Horizons speaks protocol {version}, this client {OLDER_PROTOCOL_VERSION} and {PROTOCOL_VERSION}"
    )
}

const SPLIT_EXPIRY: Duration = Duration::from_secs(30);
const MAX_BUFFER: usize = 64 << 20;

#[derive(Default)]
pub struct Splits {
    buffers: HashMap<i32, (Vec<u8>, Instant)>,
}

impl Splits {
    pub fn add(&mut self, buffer_id: i32, bytes: &[u8], first: bool, now: Instant) {
        self.buffers.retain(|_, (_, at)| now.duration_since(*at) < SPLIT_EXPIRY);
        if first {
            self.buffers.insert(buffer_id, (bytes.to_vec(), now));
        } else if let Some((buffer, at)) = self.buffers.get_mut(&buffer_id) {
            if buffer.len() + bytes.len() > MAX_BUFFER {
                self.buffers.remove(&buffer_id);
                return;
            }
            buffer.extend_from_slice(bytes);
            *at = now;
        }
    }
    pub fn take(&mut self, buffer_id: i32) -> Option<Vec<u8>> {
        self.buffers.remove(&buffer_id).map(|(b, _)| b)
    }
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.buffers.len()
    }
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Dto {
    pub source: FullDataSource,
    pub last_modified: i64,
    pub created: i64,
    pub apply_to_parent: bool,
    pub apply_to_children: bool,
    pub compression: u8,
}

fn varint(r: &mut Reader) -> Result<i32> {
    let mut value: u32 = 0;
    for i in 0..5 {
        let b = r.u8()?;
        value |= u32::from(b & 0x7F) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(value as i32);
        }
    }
    Err("varint longer than 5 bytes".to_owned())
}

fn zigzag(n: i32) -> i32 {
    ((n as u32) >> 1) as i32 ^ -(n & 1)
}

const BLOCK_STATE_SEPARATOR: &str = "_DH-BSW_";

pub fn parse_block(text: &str) -> Option<(&str, Vec<(&str, &str)>)> {
    if text == "AIR" {
        return None;
    }
    let Some((name, state)) = text.split_once("_STATE_") else {
        return Some((text, Vec::new()));
    };
    let properties = state
        .split('}')
        .filter_map(|p| p.strip_prefix('{')?.split_once(':'))
        .filter(|(_, value)| *value != "NULL")
        .collect();
    Some((name, properties))
}

#[derive(Clone, Copy, Default)]
struct RawPoint {
    id: u32,
    height: i32,
    bottom: i32,
    block: u8,
    sky: u8,
}

fn read_v2(data: &[u8], (min_x, max_x, min_z, max_z): (i32, i32, i32, i32), columns: &mut [Vec<RawPoint>]) -> Result<()> {
    let mut r = Reader::new(data);
    let cells = || (min_x..max_x).flat_map(move |x| (min_z..max_z).map(move |z| index(x, z)));
    for i in cells() {
        let count = varint(&mut r)?;
        if count < 0 || count as usize > r.remaining() {
            return Err(format!("bad column length {count}"));
        }
        columns[i] = vec![RawPoint::default(); count as usize];
    }
    let mut discontinuity = Vec::new();
    let mut lit = Vec::new();
    for i in cells() {
        for p in &mut columns[i] {
            let encoded = varint(&mut r)? as u32;
            p.id = encoded >> 2;
            discontinuity.push(encoded & 1 != 0);
            lit.push(encoded & 2 != 0);
        }
    }
    for i in cells() {
        for p in &mut columns[i] {
            p.height = varint(&mut r)?;
        }
    }
    let mut previous_bottom = 0;
    let mut k = 0;
    for i in cells() {
        for p in &mut columns[i] {
            let error = if discontinuity[k] { zigzag(varint(&mut r)?) } else { 0 };
            p.bottom = previous_bottom - p.height + error;
            previous_bottom = p.bottom;
            k += 1;
        }
    }
    let mut k = 0;
    for i in cells() {
        for p in &mut columns[i] {
            if lit[k] {
                let packed = r.u8()?;
                p.sky = packed & 15;
                p.block = packed >> 4;
            }
            k += 1;
        }
    }
    Ok(())
}

fn read_v1(data: &[u8], columns: &mut [Vec<RawPoint>]) -> Result<()> {
    let mut r = Reader::new(data);
    for column in columns.iter_mut() {
        let n = r.i16()?;
        if n < 0 {
            return Err(format!("bad column length {n}"));
        }
        *column = (0..n)
            .map(|_| {
                let p = r.i64()? as u64;
                Ok(RawPoint {
                    id: full_point::id(p),
                    height: full_point::height(p),
                    bottom: full_point::bottom_y(p),
                    block: full_point::block_light(p),
                    sky: full_point::sky_light(p),
                })
            })
            .collect::<Result<_>>()?;
    }
    Ok(())
}

fn read_mapping(data: &[u8]) -> Result<Vec<(String, String)>> {
    let mut r = Reader::new(data);
    let n = r.i32()?;
    if n < 0 || n as usize > r.remaining() / 2 {
        return Err(format!("bad mapping count {n}"));
    }
    (0..n)
        .map(|_| {
            let len = r.u16()? as usize;
            let entry: String = r.bytes(len)?.iter().map(|&b| char::from(b)).collect();
            let (biome, block) = entry
                .split_once(BLOCK_STATE_SEPARATOR)
                .ok_or_else(|| format!("mapping entry without a separator: {entry}"))?;
            Ok((biome.to_owned(), block.to_owned()))
        })
        .collect()
}

pub fn decode_dto(
    bytes: &[u8],
    resolve: &mut dyn FnMut(&str, &str) -> (u16, u32),
    y_shift: i32,
    height: i32,
) -> Result<Dto> {
    const CELLS: usize = (pos::WIDTH * pos::WIDTH) as usize;
    let mut r = Reader::new(bytes);
    let section = r.i64()?;
    let _checksum = r.i32()?;
    let data = r.blob()?;
    let adjacent = [r.blob()?, r.blob()?, r.blob()?, r.blob()?];
    let steps = r.blob()?;
    let _world_compression = r.blob()?;
    let mapping = r.blob()?;
    let format = r.u8()?;
    let compression = r.u8()?;
    let apply_to_parent = r.bool()?;
    let apply_to_children = r.bool()?;
    if r.remaining() > 16 {
        let _regenerate = r.bool()?;
    }
    let last_modified = r.i64()?;
    let created = r.i64()?;
    let detail = pos::detail_level(section);
    if !(pos::SECTION_MINIMUM_DETAIL_LEVEL..=pos::ROOT_DETAIL_LEVEL + 8).contains(&detail) {
        return Err(format!("bad section {}", pos::to_string(section)));
    }

    let unpack = |data: &[u8]| codec::decompress(compression, data);
    let mut columns: Vec<Vec<RawPoint>> = vec![Vec::new(); CELLS];
    match format {
        1 => read_v1(&unpack(data)?, &mut columns)?,
        2 => {
            let w = pos::WIDTH;
            read_v2(&unpack(data)?, (1, w - 1, 1, w - 1), &mut columns)?;
            let edges = [(0, w, 0, 1), (0, w, w - 1, w), (w - 1, w, 0, w), (0, 1, 0, w)];
            for (blob, edge) in adjacent.into_iter().zip(edges) {
                read_v2(&unpack(blob)?, edge, &mut columns)?;
            }
        }
        v => return Err(format!("unknown data format {v}")),
    }
    let steps = unpack(steps)?;
    if steps.len() < CELLS {
        return Err(format!("{} generation steps, {CELLS} wanted", steps.len()));
    }
    let entries = read_mapping(&unpack(mapping)?)?;

    let mut source = FullDataSource::empty(section);
    let mut ids = IdMap::default();
    let remap: Vec<u32> = entries
        .iter()
        .map(|(biome, block)| {
            let (b, s) = resolve(biome, block);
            ids.add_if_not_present(b, s)
        })
        .collect();
    source.mapping = ids;
    let mut any = false;
    for (i, raw) in columns.into_iter().enumerate() {
        let column: Vec<u64> = raw
            .into_iter()
            .filter_map(|p| {
                let id = *remap.get(p.id as usize)?;
                let mut bottom = p.bottom + y_shift;
                let mut top = bottom + p.height;
                bottom = bottom.max(0);
                top = top.min(height);
                full_point::encode(id, top - bottom, bottom, p.block, p.sky)
            })
            .collect();
        let step = steps[i] as i8;
        any |= !column.is_empty() || step != gen_step::EMPTY;
        source.columns[i] = column;
        source.gen_steps[i] = step;
    }
    source.is_empty = !any;
    Ok(Dto {
        source,
        last_modified,
        created,
        apply_to_parent,
        apply_to_children,
        compression,
    })
}

static NEXT_FUTURE: AtomicI32 = AtomicI32::new(1);

fn next_future() -> i32 {
    NEXT_FUTURE.fetch_add(1, Ordering::Relaxed) & i32::MAX
}

const LEVEL_INIT_WAIT: Duration = Duration::from_secs(3);
const HELLO_AGAIN: Duration = Duration::from_secs(5);
const HELLO_AGAIN_MAX: Duration = Duration::from_secs(60);
const SYNC_AGAIN_AFTER: Duration = Duration::from_secs(20 * 60);
const RATE_LIMITED_PAUSE: Duration = Duration::from_secs(1);
const ATTEMPTS: u8 = 3;

#[derive(Debug, PartialEq)]
pub enum Event {
    Data {
        bytes: Vec<u8>,
        requested: Option<i64>,
        sync: bool,
    },
    UpToDate(i64),
    Gone(i64),
    LevelInit,
    Closed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Generation,
    Sync,
}

#[derive(Clone, Debug)]
struct Task {
    section: i64,
    timestamp: Option<i64>,
    attempts: u8,
}

impl Task {
    fn kind(&self) -> Kind {
        if self.timestamp.is_some() { Kind::Sync } else { Kind::Generation }
    }
}

#[derive(Default)]
struct Queue {
    waiting: HashMap<i64, Task>,
    in_flight: usize,
    tokens: f64,
    refilled: Option<Instant>,
    paused_until: Option<Instant>,
}

impl Queue {
    fn take_token(&mut self, rate: i32, now: Instant) -> bool {
        let rate = f64::from(rate.max(1));
        if let Some(at) = self.refilled {
            self.tokens = (self.tokens + now.duration_since(at).as_secs_f64() * rate).min(rate);
        } else {
            self.tokens = rate;
        }
        self.refilled = Some(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Level {
    pub dimension: String,
    pub server_key: String,
    pub level_key: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ClientStats {
    pub waiting: usize,
    pub in_flight: usize,
    pub received: u64,
    pub up_to_date: u64,
    pub failed: u64,
    pub bytes: u64,
}

pub struct Client {
    pub config: SessionConfig,
    configured: bool,
    dimension: Option<(String, Instant)>,
    level: Option<Level>,
    time_offset: i64,
    outgoing: Vec<Vec<u8>>,
    events: Vec<Event>,
    splits: Splits,
    queues: [Queue; 2],
    in_flight: HashMap<i32, Task>,
    synced: HashMap<i64, Instant>,
    closed: Option<String>,
    stats: ClientStats,
    pub last_failure: Option<String>,
    protocol: i16,
    hello: Option<(Instant, Duration)>,
}

fn unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl Client {
    pub fn new(config: SessionConfig) -> Self {
        let mut client = Self {
            config: config.clone(),
            configured: false,
            dimension: None,
            level: None,
            time_offset: 0,
            outgoing: Vec::new(),
            events: Vec::new(),
            splits: Splits::default(),
            queues: Default::default(),
            in_flight: HashMap::new(),
            synced: HashMap::new(),
            closed: None,
            stats: ClientStats::default(),
            last_failure: None,
            protocol: PROTOCOL_VERSION,
            hello: None,
        };
        client.send(&Message::SessionConfig(config));
        client
    }

    pub fn reconfigure(&mut self, config: SessionConfig) {
        if self.closed.is_none() {
            self.send(&Message::SessionConfig(config));
        }
    }

    fn send(&mut self, message: &Message) {
        self.outgoing.push(message.encode_as(self.protocol));
    }

    #[cfg(test)]
    pub fn protocol(&self) -> i16 {
        self.protocol
    }

    fn close(&mut self, reason: String) {
        self.retain(|_| false);
        self.events.push(Event::Closed(reason.clone()));
        self.closed = Some(reason);
    }

    pub fn take_outgoing(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outgoing)
    }
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn set_dimension(&mut self, dimension: &str, now: Instant) {
        if self.dimension.as_ref().is_some_and(|(d, _)| d == dimension) {
            return;
        }
        self.retain(|_| false);
        self.synced.clear();
        self.level = None;
        self.dimension = Some((dimension.to_owned(), now));
        self.send(&Message::RequestLevelInit(dimension.to_owned()));
    }

    pub fn level(&self) -> Option<&Level> {
        self.level.as_ref()
    }

    pub fn level_name(&self, now: Instant) -> Option<&str> {
        match (&self.level, &self.dimension) {
            (Some(level), _) => Some(&level.level_key),
            (None, Some((d, at))) if now.duration_since(*at) >= LEVEL_INIT_WAIT => Some(d),
            _ => None,
        }
    }

    pub fn configured(&self) -> bool {
        self.configured
    }

    pub fn closed(&self) -> Option<&str> {
        self.closed.as_deref()
    }

    pub fn stats(&self) -> ClientStats {
        ClientStats {
            waiting: self.queues.iter().map(|q| q.waiting.len()).sum(),
            in_flight: self.in_flight.len(),
            ..self.stats
        }
    }

    pub fn generates(&self) -> bool {
        self.configured && self.config.generator_plan.enabled()
    }

    pub fn want(&mut self, section: i64, timestamp: Option<i64>, now: Instant) {
        if self.closed.is_some() || self.in_flight.values().any(|t| t.section == section) {
            return;
        }
        if self.queues.iter().any(|q| q.waiting.contains_key(&section)) {
            return;
        }
        let task = Task {
            section,
            timestamp,
            attempts: 0,
        };
        match task.kind() {
            Kind::Sync => {
                if self.synced.get(&section).is_some_and(|&at| now.duration_since(at) < SYNC_AGAIN_AFTER) {
                    return;
                }
                self.queues[Kind::Sync as usize].waiting.insert(section, task);
            }
            Kind::Generation => {
                self.queues[Kind::Generation as usize].waiting.insert(section, task);
            }
        }
    }

    pub fn retain(&mut self, keep: impl Fn(i64) -> bool) {
        for q in &mut self.queues {
            q.waiting.retain(|&s, _| keep(s));
        }
        let cancelled: Vec<i32> = self.in_flight.iter().filter(|(_, t)| !keep(t.section)).map(|(&f, _)| f).collect();
        for future in cancelled {
            let task = self.in_flight.remove(&future).unwrap();
            self.queues[task.kind() as usize].in_flight -= 1;
            self.send(&Message::Cancel(future));
        }
    }

    fn in_range(&self, task: &Task, player: [f64; 2]) -> bool {
        let c = &self.config;
        let distance =
            pos::chebyshev_signed_block_distance(task.section, player[0].floor() as i32, player[1].floor() as i32);
        match task.kind() {
            Kind::Sync => distance <= c.max_sync_on_load_distance.saturating_mul(16),
            Kind::Generation => {
                if distance > c.max_generation_request_distance.saturating_mul(16) {
                    return false;
                }
                if c.generation_max_chunk_radius <= 0 {
                    return true;
                }
                let bound = pos::chebyshev_signed_block_distance(
                    task.section,
                    c.generation_center_chunk_x.saturating_mul(16),
                    c.generation_center_chunk_z.saturating_mul(16),
                );
                bound <= c.generation_max_chunk_radius.saturating_mul(16)
            }
        }
    }

    pub fn tick(&mut self, now: Instant, player: [f64; 2]) {
        if self.closed.is_some() {
            return;
        }
        if !self.configured {
            match self.hello {
                None => self.hello = Some((now, HELLO_AGAIN)),
                Some((at, wait)) if now.duration_since(at) >= wait => {
                    self.hello = Some((now, (wait * 2).min(HELLO_AGAIN_MAX)));
                    self.send(&Message::SessionConfig(self.config.clone()));
                    if let Some((dimension, _)) = self.dimension.clone() {
                        self.dimension = Some((dimension.clone(), now));
                        self.send(&Message::RequestLevelInit(dimension));
                    }
                }
                Some(_) => {}
            }
            return;
        }
        let Some(level) = self.level_name(now).map(str::to_owned) else { return };
        for kind in [Kind::Generation, Kind::Sync] {
            let rate = match kind {
                Kind::Generation if !self.config.generator_plan.enabled() => continue,
                Kind::Sync if !self.config.synchronize_on_load => continue,
                Kind::Generation => self.config.generation_request_rate_limit,
                Kind::Sync => self.config.sync_on_load_rate_limit,
            };
            let q = &self.queues[kind as usize];
            if q.paused_until.is_some_and(|t| now < t) || q.waiting.is_empty() {
                continue;
            }
            let center = [player[0].floor() as i32, player[1].floor() as i32];
            let mut order: Vec<(i32, i64)> = q
                .waiting
                .values()
                .filter(|t| self.in_range(t, player))
                .map(|t| (pos::chebyshev_signed_block_distance(t.section, center[0], center[1]), t.section))
                .collect();
            order.sort_unstable();
            for (_, section) in order {
                let q = &mut self.queues[kind as usize];
                if q.in_flight as i32 >= rate.max(1) || !q.take_token(rate, now) {
                    break;
                }
                let task = q.waiting.remove(&section).unwrap();
                q.in_flight += 1;
                let future = next_future();
                if kind == Kind::Sync {
                    self.synced.insert(section, now);
                }
                let timestamp = task.timestamp.map(|t| t + self.time_offset);
                self.in_flight.insert(future, task);
                self.send(&Message::Request {
                    future,
                    level: level.clone(),
                    section,
                    timestamp,
                });
            }
        }
    }

    fn finish(&mut self, future: i32) -> Option<Task> {
        let task = self.in_flight.remove(&future)?;
        self.queues[task.kind() as usize].in_flight -= 1;
        Some(task)
    }

    fn retry(&mut self, mut task: Task) {
        task.attempts += 1;
        if task.attempts >= ATTEMPTS {
            self.stats.failed += 1;
            self.events.push(Event::Gone(task.section));
        } else {
            self.queues[task.kind() as usize].waiting.insert(task.section, task);
        }
    }

    pub fn receive(&mut self, data: &[u8], now: Instant) -> Result<()> {
        if self.closed.is_some() {
            return Ok(());
        }
        if let [a, b, ..] = *data {
            let version = i16::from_be_bytes([a, b]);
            if version != PROTOCOL_VERSION && version != OLDER_PROTOCOL_VERSION {
                self.close(unsupported(version));
                return Ok(());
            }
        }
        let (version, message) = Message::decode_versioned(data)?;
        if version != self.protocol {
            if version == OLDER_PROTOCOL_VERSION && !self.configured {
                self.protocol = version;
                self.send(&Message::SessionConfig(self.config.clone()));
                if let Some((dimension, _)) = self.dimension.clone() {
                    self.dimension = Some((dimension.clone(), now));
                    self.send(&Message::RequestLevelInit(dimension));
                }
            }
            return Ok(());
        }
        match message {
            Message::CloseReason(reason)
                if self.protocol == OLDER_PROTOCOL_VERSION && !self.configured && reason.contains("outdated") => {}
            Message::CloseReason(reason) => self.close(reason),
            Message::SessionConfig(config) => {
                self.config = config;
                self.configured = true;
            }
            Message::LevelInit {
                dimension,
                server_key,
                level_key,
                server_time,
            } => {
                if self.dimension.as_ref().is_none_or(|(d, _)| *d == dimension) {
                    self.time_offset = server_time - unix_ms();
                    self.level = Some(Level {
                        dimension,
                        server_key,
                        level_key,
                    });
                    self.events.push(Event::LevelInit);
                }
            }
            Message::Split { buffer_id, bytes, first } => {
                self.stats.bytes += bytes.len() as u64;
                self.splits.add(buffer_id, &bytes, first, now);
            }
            Message::Response { future, payload } => {
                let Some(task) = self.finish(future) else {
                    if let Some(p) = payload {
                        self.splits.take(p.buffer_id);
                    }
                    return Ok(());
                };
                match payload {
                    None if task.kind() == Kind::Sync => {
                        self.stats.up_to_date += 1;
                        self.events.push(Event::UpToDate(task.section));
                    }
                    None => self.events.push(Event::Gone(task.section)),
                    Some(p) => match self.splits.take(p.buffer_id) {
                        Some(bytes) => {
                            self.stats.received += 1;
                            self.events.push(Event::Data {
                                bytes,
                                requested: Some(task.section),
                                sync: task.kind() == Kind::Sync,
                            });
                        }
                        None => self.retry(task),
                    },
                }
            }
            Message::Exception { future, failure, message } => {
                let Some(task) = self.finish(future) else { return Ok(()) };
                match failure {
                    Failure::RateLimited => {
                        let q = &mut self.queues[task.kind() as usize];
                        q.paused_until = Some(now + RATE_LIMITED_PAUSE);
                        q.waiting.insert(task.section, task);
                    }
                    Failure::RequiresSplitting if pos::detail_level(task.section) > pos::SECTION_MINIMUM_DETAIL_LEVEL => {
                        for i in 0..4 {
                            self.want(pos::child(task.section, i), task.timestamp, now);
                        }
                    }
                    Failure::Rejected => {
                        self.stats.failed += 1;
                        self.events.push(Event::Gone(task.section));
                    }
                    _ => {
                        self.last_failure = Some(format!("{}: {failure:?} {message}", pos::to_string(task.section)));
                        self.retry(task);
                    }
                }
            }
            Message::PartialUpdate { level, payload } => {
                let Some(bytes) = self.splits.take(payload.buffer_id) else { return Ok(()) };
                if self.config.enable_real_time_updates && self.level_name(now) == Some(level.as_str()) {
                    self.stats.received += 1;
                    self.events.push(Event::Data {
                        bytes,
                        requested: None,
                        sync: false,
                    });
                }
            }
            Message::RequestLevelInit(_) | Message::Cancel(_) | Message::Request { .. } => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(m: Message) {
        let bytes = m.encode();
        assert_eq!(Message::decode(&bytes).unwrap(), m);
    }

    #[test]
    fn messages_roundtrip() {
        roundtrip(Message::CloseReason("bye".into()));
        roundtrip(Message::LevelInit {
            dimension: "minecraft:overworld".into(),
            server_key: "srv".into(),
            level_key: "minecraft:overworld".into(),
            server_time: 1_700_000_000_000,
        });
        roundtrip(Message::RequestLevelInit("minecraft:the_nether".into()));
        roundtrip(Message::SessionConfig(SessionConfig::default()));
        roundtrip(Message::Cancel(7));
        roundtrip(Message::Exception {
            future: 3,
            failure: Failure::RequiresSplitting,
            message: "split".into(),
        });
        roundtrip(Message::Request {
            future: 9,
            level: "l".into(),
            section: pos::encode(8, -3, 4),
            timestamp: Some(42),
        });
        roundtrip(Message::Request {
            future: 9,
            level: "l".into(),
            section: 5,
            timestamp: None,
        });
        roundtrip(Message::Response {
            future: 2,
            payload: Some(Payload {
                buffer_id: 11,
                beacons: vec![([1, 2, 3], 0xFF00FF)],
            }),
        });
        roundtrip(Message::Response { future: 2, payload: None });
        roundtrip(Message::PartialUpdate {
            level: "l".into(),
            payload: Payload {
                buffer_id: 1,
                beacons: vec![],
            },
        });
        roundtrip(Message::Split {
            buffer_id: 4,
            bytes: vec![1, 2, 3],
            first: true,
        });
    }

    #[test]
    fn wire_layout() {
        assert_eq!(Message::RequestLevelInit("ab".into()).encode(), [0, 16, 0, 3, 0, 2, b'a', b'b']);
        let c = Message::SessionConfig(SessionConfig::default()).encode();
        assert_eq!(c.len(), 4 + 1 + 4 * 5 + 1 + 4 + 1 + 4 * 3);
        assert_eq!(&c[4..9], [0, 0, 0, 16, 0]);
        assert!(Message::decode(&[0, 14, 0, 1, 0, 0]).is_err());
        assert!(Message::decode(&[0, 15, 0, 1, 0, 0]).is_ok());
        assert!(Message::decode(&[0, 16, 0, 5, 0, 0, 0, 1, 9]).is_err());
    }

    #[test]
    fn splits() {
        let now = Instant::now();
        let mut s = Splits::default();
        s.add(1, b"ab", false, now);
        assert!(s.take(1).is_none());
        s.add(1, b"ab", true, now);
        s.add(1, b"cd", false, now);
        assert_eq!(s.take(1).unwrap(), b"abcd");
        s.add(2, b"x", true, now);
        s.add(3, b"y", true, now + Duration::from_secs(31));
        assert!(s.take(2).is_none());
        assert!(s.take(3).is_some());
    }

    fn put_varint(out: &mut Vec<u8>, mut v: u32) {
        loop {
            let b = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                break;
            }
            out.push(b | 0x80);
        }
    }

    fn write_v2(columns: &[Vec<RawPoint>], (min_x, max_x, min_z, max_z): (i32, i32, i32, i32)) -> Vec<u8> {
        let cells: Vec<usize> = (min_x..max_x).flat_map(|x| (min_z..max_z).map(move |z| index(x, z))).collect();
        let mut out = Vec::new();
        for &i in &cells {
            put_varint(&mut out, columns[i].len() as u32);
        }
        let mut previous = 0;
        let mut flags = Vec::new();
        for &i in &cells {
            for p in &columns[i] {
                let expected = previous - p.height;
                let discontinuity = p.bottom != expected;
                let lit = p.sky != 0 || p.block != 0;
                flags.push((discontinuity, lit, p.bottom - expected));
                put_varint(&mut out, p.id << 2 | u32::from(lit) << 1 | u32::from(discontinuity));
                previous = p.bottom;
            }
        }
        for &i in &cells {
            for p in &columns[i] {
                put_varint(&mut out, p.height as u32);
            }
        }
        let mut k = 0;
        for &i in &cells {
            for _ in &columns[i] {
                if flags[k].0 {
                    let e = flags[k].2;
                    put_varint(&mut out, ((e << 1) ^ (e >> 31)) as u32);
                }
                k += 1;
            }
        }
        let mut k = 0;
        for &i in &cells {
            for p in &columns[i] {
                if flags[k].1 {
                    out.push(p.block << 4 | p.sky);
                }
                k += 1;
            }
        }
        out
    }

    fn dto(columns: &[Vec<RawPoint>], mapping: &[&str], steps: i8) -> Vec<u8> {
        let w = pos::WIDTH;
        let mut map = Writer::default();
        map.i32(mapping.len() as i32);
        for m in mapping {
            map.u16(m.len() as u16);
            map.0.extend_from_slice(m.as_bytes());
        }
        let mut out = Writer::default();
        out.i64(pos::encode(6, 2, -1));
        out.i32(0);
        out.blob(&write_v2(columns, (1, w - 1, 1, w - 1)));
        for edge in [(0, w, 0, 1), (0, w, w - 1, w), (w - 1, w, 0, w), (0, 1, 0, w)] {
            out.blob(&write_v2(columns, edge));
        }
        out.blob(&vec![steps as u8; 4096]);
        out.blob(&[0; 4096]);
        out.blob(&map.0);
        out.u8(2);
        out.u8(codec::mode::UNCOMPRESSED);
        out.bool(true);
        out.bool(false);
        out.bool(false);
        out.i64(1234);
        out.i64(1000);
        out.0
    }

    #[test]
    fn decodes_v2() {
        let mut columns = vec![Vec::new(); 4096];
        for x in 0..64 {
            for z in 0..64 {
                let i = index(x, z);
                let gap = (x + z) % 7 == 0;
                columns[i] = vec![
                    RawPoint { id: 0, height: 320, bottom: 80, block: 0, sky: 15 },
                    RawPoint { id: 1, height: 1, bottom: 79, block: 2, sky: 15 },
                    RawPoint { id: 2, height: if gap { 40 } else { 78 }, bottom: 1, block: 0, sky: 0 },
                    RawPoint { id: 3, height: 1, bottom: 0, block: 0, sky: 0 },
                ];
            }
        }
        let bytes = dto(
            &columns,
            &[
                "minecraft:plains_DH-BSW_AIR",
                "minecraft:plains_DH-BSW_minecraft:grass_block_STATE_{snowy:false}",
                "minecraft:plains_DH-BSW_minecraft:stone",
                "minecraft:plains_DH-BSW_minecraft:bedrock",
            ],
            gen_step::FEATURES,
        );
        let mut seen = Vec::new();
        let d = decode_dto(
            &bytes,
            &mut |biome, block| {
                seen.push(format!("{biome}|{block}"));
                (7, seen.len() as u32 * 10)
            },
            0,
            384,
        )
        .unwrap();
        assert_eq!(seen[1], "minecraft:plains|minecraft:grass_block_STATE_{snowy:false}");
        assert_eq!(d.last_modified, 1234);
        assert!(d.apply_to_parent && !d.apply_to_children);
        let s = &d.source;
        assert_eq!(s.pos, pos::encode(6, 2, -1));
        assert!(!s.is_empty);
        assert!(s.gen_steps.iter().all(|&g| g == gen_step::FEATURES));
        for x in 0..64 {
            for z in 0..64 {
                let c = s.column(x, z);
                assert_eq!(c.len(), 4, "{x},{z}");
                let gap = (x + z) % 7 == 0;
                assert_eq!(full_point::height(c[2]), if gap { 40 } else { 78 });
                assert_eq!(full_point::bottom_y(c[1]), 79);
                assert_eq!(full_point::block_light(c[1]), 2);
                assert_eq!(full_point::sky_light(c[1]), 15);
                assert_eq!(s.mapping.get(full_point::id(c[1])), (7, 20));
                assert_eq!(full_point::top_y(c[0]), 384);
            }
        }
        let d = decode_dto(&bytes, &mut |_, _| (0, 1), 64, 384).unwrap();
        assert_eq!(full_point::bottom_y(d.source.column(5, 5)[3]), 64);
    }

    #[test]
    fn parses_block_states() {
        assert_eq!(parse_block("AIR"), None);
        assert_eq!(parse_block("minecraft:stone"), Some(("minecraft:stone", vec![])));
        assert_eq!(
            parse_block("minecraft:oak_stairs_STATE_{facing:east}{half:top}{waterlogged:false}"),
            Some(("minecraft:oak_stairs", vec![("facing", "east"), ("half", "top"), ("waterlogged", "false")]))
        );
    }

    #[test]
    fn decodes_v1() {
        for regenerate in [true, false] {
            decodes_v1_with(regenerate);
        }
    }

    fn decodes_v1_with(regenerate: bool) {
        let mut out = Writer::default();
        out.i64(pos::encode(6, 0, 0));
        out.i32(0);
        let mut data = Writer::default();
        for i in 0..4096 {
            if i == 5 {
                data.i16(1);
                data.i64(full_point::encode(0, 10, 3, 1, 14).unwrap() as i64);
            } else {
                data.i16(0);
            }
        }
        out.blob(&data.0);
        for _ in 0..4 {
            out.blob(&[]);
        }
        let mut steps = vec![0u8; 4096];
        steps[5] = gen_step::LIGHT as u8;
        out.blob(&steps);
        out.blob(&[]);
        let mut map = Writer::default();
        map.i32(1);
        map.string("EMPTY_DH-BSW_AIR");
        out.blob(&map.0);
        let flags: &[u8] = if regenerate { &[1, 0, 0, 0, 0] } else { &[1, 0, 0, 0] };
        for &b in flags {
            out.u8(b);
        }
        out.i64(0);
        out.i64(0);
        let d = decode_dto(&out.0, &mut |_, _| (0, 0), 0, 384).unwrap();
        assert_eq!(d.source.columns[5].len(), 1);
        assert_eq!(full_point::sky_light(d.source.columns[5][0]), 14);
        assert!(d.source.columns[6].is_empty());
        assert_eq!(d.source.missing_columns(), 4095);
    }

    fn server_says(client: &mut Client, m: Message) {
        client.receive(&m.encode(), Instant::now()).unwrap();
    }

    fn sent(client: &mut Client) -> Vec<Message> {
        client.take_outgoing().iter().map(|b| Message::decode(b).unwrap()).collect()
    }

    #[test]
    fn says_hello_again_until_answered() {
        let t = Instant::now();
        let mut c = Client::new(SessionConfig::default());
        c.set_dimension("minecraft:overworld", t);
        c.take_outgoing();
        c.tick(t, [0.0, 0.0]);
        c.tick(t + Duration::from_secs(4), [0.0, 0.0]);
        assert!(c.take_outgoing().is_empty());
        c.tick(t + Duration::from_secs(5), [0.0, 0.0]);
        let again: Vec<_> = c.take_outgoing().iter().map(|b| Message::decode(b).unwrap()).collect();
        assert!(matches!(&again[..], [Message::SessionConfig(_), Message::RequestLevelInit(d)] if d == "minecraft:overworld"));
        c.tick(t + Duration::from_secs(14), [0.0, 0.0]);
        assert!(c.take_outgoing().is_empty());
        c.tick(t + Duration::from_secs(15), [0.0, 0.0]);
        assert_eq!(c.take_outgoing().len(), 2);
        c.receive(&Message::SessionConfig(SessionConfig::default()).encode(), t).unwrap();
        c.tick(t + Duration::from_secs(100), [0.0, 0.0]);
        assert!(c.take_outgoing().iter().all(|b| !matches!(Message::decode(b).unwrap(), Message::SessionConfig(_))));
    }

    #[test]
    fn falls_back_to_the_older_protocol() {
        let mut c = Client::new(SessionConfig::default());
        let now = Instant::now();
        c.set_dimension("minecraft:overworld", now);
        assert_eq!(c.take_outgoing().len(), 2);
        let outdated = Message::CloseReason("Client is outdated.".into()).encode_as(OLDER_PROTOCOL_VERSION);
        c.receive(&outdated, now).unwrap();
        c.receive(&outdated, now).unwrap();
        assert_eq!(c.protocol(), OLDER_PROTOCOL_VERSION);
        assert!(c.closed().is_none());
        let again = c.take_outgoing();
        assert_eq!(again.len(), 2);
        assert!(again.iter().all(|b| b[..2] == OLDER_PROTOCOL_VERSION.to_be_bytes()));
        let hello = Message::decode_versioned(&again[0]).unwrap();
        assert_eq!(again[0][4], 1);
        assert_eq!(hello.1, Message::SessionConfig(SessionConfig::default()));
        let reply = Message::SessionConfig(SessionConfig::default()).encode_as(OLDER_PROTOCOL_VERSION);
        c.receive(&reply, now).unwrap();
        assert!(c.configured() && c.generates());
        let bye = Message::CloseReason("bye".into()).encode_as(OLDER_PROTOCOL_VERSION);
        c.receive(&bye, now).unwrap();
        assert_eq!(c.closed(), Some("bye"));

        let mut dhs = vec![0, 16, 0, 1, 0, 0, 0, 3];
        dhs.extend_from_slice(b"bye");
        assert_eq!(Message::decode(&dhs).unwrap(), Message::CloseReason("bye".into()));

        let mut c = Client::new(SessionConfig::default());
        c.receive(&Message::CloseReason("x".into()).encode_as(17), now).unwrap();
        assert!(c.closed().is_some_and(|r| r.contains("protocol 17")));
    }

    fn ready_client() -> Client {
        let mut c = Client::new(SessionConfig::default());
        let now = Instant::now();
        c.set_dimension("minecraft:overworld", now);
        assert!(matches!(&sent(&mut c)[..], [Message::SessionConfig(_), Message::RequestLevelInit(_)]));
        server_says(&mut c, Message::SessionConfig(SessionConfig {
            generation_request_rate_limit: 2,
            ..SessionConfig::default()
        }));
        server_says(&mut c, Message::LevelInit {
            dimension: "minecraft:overworld".into(),
            server_key: "key".into(),
            level_key: "level".into(),
            server_time: unix_ms() + 1000,
        });
        assert_eq!(c.take_events(), [Event::LevelInit]);
        c
    }

    #[test]
    fn requests_nearest_first_within_limits() {
        let mut c = ready_client();
        let now = Instant::now();
        for x in [5, 1, 3, 2] {
            c.want(pos::encode(7, x, 0), None, now);
        }
        c.tick(now, [0.0, 0.0]);
        let s = sent(&mut c);
        let sections: Vec<i64> = s
            .iter()
            .map(|m| match m {
                Message::Request { section, timestamp: None, level, .. } => {
                    assert_eq!(level, "level");
                    *section
                }
                m => panic!("{m:?}"),
            })
            .collect();
        assert_eq!(sections, [pos::encode(7, 1, 0), pos::encode(7, 2, 0)]);
        c.tick(now, [0.0, 0.0]);
        assert!(sent(&mut c).is_empty());
        let Message::Request { future, .. } = s[0] else { unreachable!() };
        server_says(&mut c, Message::Exception {
            future,
            failure: Failure::RequiresSplitting,
            message: String::new(),
        });
        assert_eq!(c.stats().waiting, 6);
        let Message::Request { future, section, .. } = s[1] else { unreachable!() };
        server_says(&mut c, Message::Split { buffer_id: 77, bytes: vec![1, 2], first: true });
        server_says(&mut c, Message::Split { buffer_id: 77, bytes: vec![3], first: false });
        server_says(&mut c, Message::Response {
            future,
            payload: Some(Payload { buffer_id: 77, beacons: vec![] }),
        });
        assert_eq!(c.take_events(), [Event::Data {
            bytes: vec![1, 2, 3],
            requested: Some(section),
            sync: false
        }]);
        c.tick(now + Duration::from_secs(2), [0.0, 0.0]);
        let s = sent(&mut c);
        assert_eq!(s.len(), 2);
        c.retain(|_| false);
        assert!(matches!(&sent(&mut c)[..], [Message::Cancel(_), Message::Cancel(_)]));
        assert_eq!(c.stats().in_flight, 0);
    }

    #[test]
    fn sync_on_load() {
        let mut c = ready_client();
        let now = Instant::now();
        let s = pos::encode(7, 0, 0);
        c.want(s, Some(5000), now);
        c.tick(now, [0.0, 0.0]);
        let [Message::Request { future, timestamp: Some(t), .. }] = sent(&mut c)[..] else { panic!() };
        assert!((t - 6000).abs() < 100, "{t}");
        server_says(&mut c, Message::Response { future, payload: None });
        assert_eq!(c.take_events(), [Event::UpToDate(s)]);
        c.want(s, Some(5000), now);
        assert_eq!(c.stats().waiting, 0);
    }

    #[test]
    fn retries_and_rate_limits() {
        let mut c = ready_client();
        let now = Instant::now();
        let s = pos::encode(6, 0, 0);
        c.want(s, None, now);
        c.tick(now, [0.0, 0.0]);
        let [Message::Request { future, .. }] = sent(&mut c)[..] else { panic!() };
        server_says(&mut c, Message::Exception { future, failure: Failure::RateLimited, message: String::new() });
        c.tick(now, [0.0, 0.0]);
        assert!(sent(&mut c).is_empty());
        let mut t = now + Duration::from_secs(2);
        for _ in 0..ATTEMPTS {
            c.tick(t, [0.0, 0.0]);
            let [Message::Request { future, .. }] = sent(&mut c)[..] else { panic!() };
            server_says(&mut c, Message::Exception { future, failure: Failure::Other(9), message: String::new() });
            t += Duration::from_secs(1);
        }
        assert_eq!(c.take_events(), [Event::Gone(s)]);
        c.want(pos::encode(6, 100_000, 0), None, now);
        c.tick(t, [0.0, 0.0]);
        assert!(sent(&mut c).is_empty());
        assert_eq!(c.stats().waiting, 1);
    }

    #[test]
    fn partial_updates_for_this_level() {
        let mut c = ready_client();
        server_says(&mut c, Message::Split { buffer_id: 1, bytes: vec![9], first: true });
        server_says(&mut c, Message::PartialUpdate { level: "other".into(), payload: Payload { buffer_id: 1, beacons: vec![] } });
        server_says(&mut c, Message::Split { buffer_id: 2, bytes: vec![8], first: true });
        server_says(&mut c, Message::PartialUpdate { level: "level".into(), payload: Payload { buffer_id: 2, beacons: vec![] } });
        assert_eq!(c.take_events(), [Event::Data { bytes: vec![8], requested: None, sync: false }]);
    }
}
