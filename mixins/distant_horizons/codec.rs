// Part of Distant Horizons for Horizon Client, LGPL-3.0 (see runtime.rs).

pub const MAX_OUTPUT: usize = 64 << 20;

pub mod mode {
    pub const UNCOMPRESSED: u8 = 0;
    pub const LZ4: u8 = 1;
    pub const Z_STD_STREAM: u8 = 2;
    pub const LZMA2: u8 = 3;
    pub const Z_STD_BLOCK: u8 = 4;
}

pub type Result<T> = std::result::Result<T, String>;

fn err<T>(what: &str) -> Result<T> {
    Err(what.to_owned())
}

pub fn decompress(mode: u8, data: &[u8]) -> Result<Vec<u8>> {
    match mode {
        mode::UNCOMPRESSED => Ok(data.to_vec()),
        mode::LZ4 => lz4::decode_frames(data),
        mode::Z_STD_STREAM | mode::Z_STD_BLOCK => zstd::decode(data),
        mode::LZMA2 => xz::decode(data),
        other => Err(format!("unknown compression mode {other}")),
    }
}

pub mod hash {
    const P32_1: u32 = 0x9E37_79B1;
    const P32_2: u32 = 0x85EB_CA77;
    const P32_3: u32 = 0xC2B2_AE3D;
    const P32_4: u32 = 0x27D4_EB2F;
    const P32_5: u32 = 0x1656_67B1;

    fn u32_at(b: &[u8], i: usize) -> u32 {
        u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
    }
    fn u64_at(b: &[u8], i: usize) -> u64 {
        u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
    }

    pub fn xxh32(data: &[u8], seed: u32) -> u32 {
        let round = |acc: u32, lane: u32| acc.wrapping_add(lane.wrapping_mul(P32_2)).rotate_left(13).wrapping_mul(P32_1);
        let mut i = 0;
        let mut h = if data.len() >= 16 {
            let mut v = [
                seed.wrapping_add(P32_1).wrapping_add(P32_2),
                seed.wrapping_add(P32_2),
                seed,
                seed.wrapping_sub(P32_1),
            ];
            while i + 16 <= data.len() {
                for (k, lane) in v.iter_mut().enumerate() {
                    *lane = round(*lane, u32_at(data, i + 4 * k));
                }
                i += 16;
            }
            v[0].rotate_left(1)
                .wrapping_add(v[1].rotate_left(7))
                .wrapping_add(v[2].rotate_left(12))
                .wrapping_add(v[3].rotate_left(18))
        } else {
            seed.wrapping_add(P32_5)
        };
        h = h.wrapping_add(data.len() as u32);
        while i + 4 <= data.len() {
            h = h.wrapping_add(u32_at(data, i).wrapping_mul(P32_3)).rotate_left(17).wrapping_mul(P32_4);
            i += 4;
        }
        for &b in &data[i..] {
            h = h.wrapping_add(u32::from(b).wrapping_mul(P32_5)).rotate_left(11).wrapping_mul(P32_1);
        }
        h ^= h >> 15;
        h = h.wrapping_mul(P32_2);
        h ^= h >> 13;
        h = h.wrapping_mul(P32_3);
        h ^ (h >> 16)
    }

    const P64_1: u64 = 0x9E37_79B1_85EB_CA87;
    const P64_2: u64 = 0xC2B2_AE3D_27D4_EB4F;
    const P64_3: u64 = 0x1656_67B1_9E37_79F9;
    const P64_4: u64 = 0x85EB_CA77_C2B2_AE63;
    const P64_5: u64 = 0x27D4_EB2F_1656_67C5;

    fn round64(acc: u64, lane: u64) -> u64 {
        acc.wrapping_add(lane.wrapping_mul(P64_2)).rotate_left(31).wrapping_mul(P64_1)
    }
    fn merge64(acc: u64, v: u64) -> u64 {
        (acc ^ round64(0, v)).wrapping_mul(P64_1).wrapping_add(P64_4)
    }

    pub fn xxh64(data: &[u8], seed: u64) -> u64 {
        let mut i = 0;
        let mut h = if data.len() >= 32 {
            let mut v = [
                seed.wrapping_add(P64_1).wrapping_add(P64_2),
                seed.wrapping_add(P64_2),
                seed,
                seed.wrapping_sub(P64_1),
            ];
            while i + 32 <= data.len() {
                for (k, lane) in v.iter_mut().enumerate() {
                    *lane = round64(*lane, u64_at(data, i + 8 * k));
                }
                i += 32;
            }
            let mut h = v[0]
                .rotate_left(1)
                .wrapping_add(v[1].rotate_left(7))
                .wrapping_add(v[2].rotate_left(12))
                .wrapping_add(v[3].rotate_left(18));
            for lane in v {
                h = merge64(h, lane);
            }
            h
        } else {
            seed.wrapping_add(P64_5)
        };
        h = h.wrapping_add(data.len() as u64);
        while i + 8 <= data.len() {
            h ^= round64(0, u64_at(data, i));
            h = h.rotate_left(27).wrapping_mul(P64_1).wrapping_add(P64_4);
            i += 8;
        }
        if i + 4 <= data.len() {
            h ^= u64::from(u32_at(data, i)).wrapping_mul(P64_1);
            h = h.rotate_left(23).wrapping_mul(P64_2).wrapping_add(P64_3);
            i += 4;
        }
        for &b in &data[i..] {
            h ^= u64::from(b).wrapping_mul(P64_5);
            h = h.rotate_left(11).wrapping_mul(P64_1);
        }
        h ^= h >> 33;
        h = h.wrapping_mul(P64_2);
        h ^= h >> 29;
        h = h.wrapping_mul(P64_3);
        h ^ (h >> 32)
    }

    fn crc_table32() -> [u32; 256] {
        std::array::from_fn(|n| (0..8).fold(n as u32, |c, _| if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 }))
    }
    fn crc_table64() -> [u64; 256] {
        std::array::from_fn(|n| {
            (0..8).fold(n as u64, |c, _| if c & 1 != 0 { 0xC96C_5795_D787_0F42 ^ (c >> 1) } else { c >> 1 })
        })
    }

    pub fn crc32(data: &[u8]) -> u32 {
        thread_local!(static TABLE: [u32; 256] = crc_table32());
        TABLE.with(|t| !data.iter().fold(!0u32, |c, &b| t[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8)))
    }

    pub fn crc64(data: &[u8]) -> u64 {
        thread_local!(static TABLE: [u64; 256] = crc_table64());
        TABLE.with(|t| !data.iter().fold(!0u64, |c, &b| t[((c ^ u64::from(b)) & 0xFF) as usize] ^ (c >> 8)))
    }
}

pub mod lz4 {
    use super::{MAX_OUTPUT, Result, err, hash};

    const MAGIC: u32 = 0x184D_2204;

    pub fn decode_block(input: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<()> {
        let mut i = 0;
        let length = |i: &mut usize, mut n: usize| -> Result<usize> {
            if n == 15 {
                loop {
                    let b = *input.get(*i).ok_or("truncated LZ4 length")?;
                    *i += 1;
                    n += usize::from(b);
                    if b != 255 {
                        break;
                    }
                }
            }
            Ok(n)
        };
        loop {
            let token = *input.get(i).ok_or("truncated LZ4 block")?;
            i += 1;
            let literals = length(&mut i, usize::from(token >> 4))?;
            let run = input.get(i..i + literals).ok_or("truncated LZ4 literals")?;
            if out.len() + literals > limit {
                return err("LZ4 block too large");
            }
            out.extend_from_slice(run);
            i += literals;
            if i == input.len() {
                return Ok(());
            }
            let offset = usize::from(u16::from_le_bytes(
                input.get(i..i + 2).ok_or("truncated LZ4 offset")?.try_into().unwrap(),
            ));
            i += 2;
            let len = length(&mut i, usize::from(token & 15))? + 4;
            if offset == 0 || offset > out.len() {
                return err("invalid LZ4 offset");
            }
            if out.len() + len > limit {
                return err("LZ4 block too large");
            }
            let start = out.len() - offset;
            for k in 0..len {
                let b = out[start + k];
                out.push(b);
            }
        }
    }

    pub fn decode_frames(data: &[u8]) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < data.len() {
            let magic = u32::from_le_bytes(data.get(i..i + 4).ok_or("truncated LZ4 frame")?.try_into().unwrap());
            if magic & 0xFFFF_FFF0 == 0x184D_2A50 {
                let size = u32::from_le_bytes(data.get(i + 4..i + 8).ok_or("truncated skippable frame")?.try_into().unwrap());
                i += 8 + size as usize;
                continue;
            }
            if magic != MAGIC {
                return err("not an LZ4 frame");
            }
            i = decode_frame(data, i + 4, &mut out)?;
        }
        Ok(out)
    }

    fn decode_frame(data: &[u8], mut i: usize, out: &mut Vec<u8>) -> Result<usize> {
        let start_out = out.len();
        let descriptor_start = i;
        let flg = *data.get(i).ok_or("truncated LZ4 header")?;
        let bd = *data.get(i + 1).ok_or("truncated LZ4 header")?;
        i += 2;
        if flg >> 6 != 1 {
            return err("unsupported LZ4 frame version");
        }
        let block_checksum = flg & 0x10 != 0;
        let content_size = flg & 0x08 != 0;
        let content_checksum = flg & 0x04 != 0;
        let dictionary = flg & 0x01 != 0;
        let max_block = match (bd >> 4) & 7 {
            4 => 64 << 10,
            5 => 256 << 10,
            6 => 1 << 20,
            7 => 4 << 20,
            _ => return err("invalid LZ4 block size"),
        };
        if content_size {
            i += 8;
        }
        if dictionary {
            return err("LZ4 dictionaries are not supported");
        }
        let header = data.get(descriptor_start..i).ok_or("truncated LZ4 header")?;
        let hc = *data.get(i).ok_or("truncated LZ4 header")?;
        if (hash::xxh32(header, 0) >> 8) as u8 != hc {
            return err("LZ4 header checksum mismatch");
        }
        i += 1;
        loop {
            let word = u32::from_le_bytes(data.get(i..i + 4).ok_or("truncated LZ4 block")?.try_into().unwrap());
            i += 4;
            if word == 0 {
                break;
            }
            let size = (word & 0x7FFF_FFFF) as usize;
            if size > max_block {
                return err("LZ4 block over its size");
            }
            let block = data.get(i..i + size).ok_or("truncated LZ4 block")?;
            i += size;
            if block_checksum {
                let sum = u32::from_le_bytes(data.get(i..i + 4).ok_or("truncated LZ4 checksum")?.try_into().unwrap());
                if hash::xxh32(block, 0) != sum {
                    return err("LZ4 block checksum mismatch");
                }
                i += 4;
            }
            let before = out.len();
            let limit = (before + max_block).min(MAX_OUTPUT);
            if word & 0x8000_0000 != 0 {
                if before + size > limit {
                    return err("LZ4 output too large");
                }
                out.extend_from_slice(block);
            } else {
                decode_block(block, out, limit)?;
            }
        }
        if content_checksum {
            let sum = u32::from_le_bytes(data.get(i..i + 4).ok_or("truncated LZ4 checksum")?.try_into().unwrap());
            if hash::xxh32(&out[start_out..], 0) != sum {
                return err("LZ4 content checksum mismatch");
            }
            i += 4;
        }
        Ok(i)
    }
}

pub mod zstd {
    use super::{MAX_OUTPUT, Result, err, hash};

    const MAGIC: u32 = 0xFD2F_B528;
    const MAX_BLOCK: usize = 128 << 10;

    struct Backward<'a> {
        data: &'a [u8],
        left: i64,
    }

    impl<'a> Backward<'a> {
        fn new(data: &'a [u8]) -> Result<Self> {
            let last = *data.last().ok_or("empty bit stream")?;
            if last == 0 {
                return err("bit stream has no end marker");
            }
            let marker = 7 - i64::from(last.leading_zeros());
            Ok(Self {
                data,
                left: (data.len() as i64 - 1) * 8 + marker,
            })
        }
        fn peek(&self, n: u32) -> u64 {
            if n == 0 {
                return 0;
            }
            let n = i64::from(n);
            let start = self.left - n;
            let value = if start >= 0 {
                self.bits_at(start as usize, n as u32)
            } else if self.left > 0 {
                self.bits_at(0, self.left as u32) << (-start)
            } else {
                0
            };
            value & ((1u64 << n) - 1)
        }
        fn bits_at(&self, start: usize, n: u32) -> u64 {
            let byte = start / 8;
            let mut word = [0u8; 8];
            let end = (byte + 8).min(self.data.len());
            word[..end - byte].copy_from_slice(&self.data[byte..end]);
            (u64::from_le_bytes(word) >> (start % 8)) & ((1u64 << n) - 1)
        }
        fn consume(&mut self, n: u32) {
            self.left -= i64::from(n);
        }
        fn read(&mut self, n: u32) -> u64 {
            let v = self.peek(n);
            self.consume(n);
            v
        }
        fn overflowed(&self) -> bool {
            self.left < 0
        }
    }

    struct Forward<'a> {
        data: &'a [u8],
        bit: usize,
    }

    impl Forward<'_> {
        fn peek(&self, n: u32) -> u64 {
            let byte = self.bit / 8;
            let mut word = [0u8; 8];
            let end = (byte + 8).min(self.data.len());
            if byte < end {
                word[..end - byte].copy_from_slice(&self.data[byte..end]);
            }
            (u64::from_le_bytes(word) >> (self.bit % 8)) & ((1u64 << n) - 1)
        }
        fn read(&mut self, n: u32) -> u64 {
            let v = self.peek(n);
            self.bit += n as usize;
            v
        }
    }

    #[derive(Clone, Copy, Default)]
    struct FseEntry {
        symbol: u8,
        bits: u8,
        base: u16,
    }

    #[derive(Clone, Default)]
    struct Fse {
        log: u32,
        table: Vec<FseEntry>,
    }

    impl Fse {
        fn build(counts: &[i16], log: u32) -> Result<Self> {
            let size = 1usize << log;
            let mut table = vec![FseEntry::default(); size];
            let mut next = vec![0u32; counts.len()];
            let mut high = size - 1;
            for (s, &c) in counts.iter().enumerate() {
                if c == -1 {
                    table[high].symbol = s as u8;
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
                    table[position].symbol = s as u8;
                    loop {
                        position = (position + step) & mask;
                        if high == usize::MAX || position <= high {
                            break;
                        }
                    }
                }
            }
            if position != 0 {
                return err("invalid FSE distribution");
            }
            for entry in table.iter_mut() {
                let s = usize::from(entry.symbol);
                let state = next[s];
                next[s] += 1;
                if state == 0 {
                    return err("invalid FSE distribution");
                }
                let bits = log - (31 - state.leading_zeros());
                entry.bits = bits as u8;
                entry.base = ((state << bits) as usize - size) as u16;
            }
            Ok(Self { log, table })
        }

        fn rle(symbol: u8) -> Self {
            Self {
                log: 0,
                table: vec![FseEntry { symbol, bits: 0, base: 0 }],
            }
        }

        fn read(data: &[u8], max_symbol: usize, max_log: u32) -> Result<(Self, usize)> {
            let mut r = Forward { data, bit: 0 };
            let log = r.read(4) as u32 + 5;
            if log > max_log {
                return err("FSE accuracy too high");
            }
            let mut remaining = (1i32 << log) + 1;
            let mut threshold = 1i32 << log;
            let mut bits = log + 1;
            let mut counts: Vec<i16> = Vec::new();
            while remaining > 1 {
                if counts.len() > max_symbol {
                    return err("too many FSE symbols");
                }
                let max = 2 * threshold - 1 - remaining;
                let value = r.peek(bits) as i32;
                let count = if (value & (threshold - 1)) < max {
                    r.bit += (bits - 1) as usize;
                    value & (threshold - 1)
                } else {
                    r.bit += bits as usize;
                    let v = value & (2 * threshold - 1);
                    if v >= threshold { v - max } else { v }
                };
                let probability = count - 1;
                remaining -= probability.abs();
                counts.push(probability as i16);
                if probability == 0 {
                    loop {
                        let repeat = r.read(2) as usize;
                        counts.extend(std::iter::repeat_n(0, repeat));
                        if repeat != 3 {
                            break;
                        }
                    }
                }
                while remaining < threshold && threshold > 1 {
                    bits -= 1;
                    threshold >>= 1;
                }
            }
            if remaining != 1 || counts.len() > max_symbol + 1 {
                return err("invalid FSE table description");
            }
            let used = r.bit.div_ceil(8);
            if used > data.len() {
                return err("truncated FSE table");
            }
            Ok((Self::build(&counts, log)?, used))
        }
    }

    struct State<'t> {
        fse: &'t Fse,
        state: usize,
    }

    impl<'t> State<'t> {
        fn new(fse: &'t Fse, bits: &mut Backward) -> Self {
            Self {
                fse,
                state: bits.read(fse.log) as usize,
            }
        }
        fn symbol(&self) -> u8 {
            self.fse.table[self.state].symbol
        }
        fn update(&mut self, bits: &mut Backward) {
            let e = self.fse.table[self.state];
            self.state = usize::from(e.base) + bits.read(u32::from(e.bits)) as usize;
        }
    }

    #[derive(Clone, Default)]
    struct Huffman {
        max_bits: u32,
        table: Vec<(u8, u8)>,
    }

    impl Huffman {
        fn read(data: &[u8]) -> Result<(Self, usize)> {
            let header = *data.first().ok_or("missing Huffman tree")? as usize;
            let mut weights: Vec<u8>;
            let used;
            if header >= 128 {
                let count = header - 127;
                let bytes = data.get(1..1 + count.div_ceil(2)).ok_or("truncated Huffman weights")?;
                weights = (0..count).map(|i| if i % 2 == 0 { bytes[i / 2] >> 4 } else { bytes[i / 2] & 15 }).collect();
                used = 1 + count.div_ceil(2);
            } else {
                let body = data.get(1..1 + header).ok_or("truncated Huffman weights")?;
                let (fse, n) = Fse::read(body, 255, 6)?;
                let mut bits = Backward::new(body.get(n..).ok_or("truncated Huffman weights")?)?;
                let mut s1 = State::new(&fse, &mut bits);
                let mut s2 = State::new(&fse, &mut bits);
                weights = Vec::new();
                loop {
                    weights.push(s1.symbol());
                    s1.update(&mut bits);
                    if bits.overflowed() {
                        weights.push(s2.symbol());
                        break;
                    }
                    weights.push(s2.symbol());
                    s2.update(&mut bits);
                    if bits.overflowed() {
                        weights.push(s1.symbol());
                        break;
                    }
                    if weights.len() > 255 {
                        return err("too many Huffman weights");
                    }
                }
                used = 1 + header;
            }
            if weights.len() > 255 || weights.iter().any(|&w| w > 11) {
                return err("invalid Huffman weights");
            }
            let total: u32 = weights.iter().filter(|&&w| w > 0).map(|&w| 1u32 << (w - 1)).sum();
            if total == 0 {
                return err("invalid Huffman weights");
            }
            let max_bits = 32 - total.leading_zeros();
            let rest = (1u32 << max_bits) - total;
            if !rest.is_power_of_two() {
                return err("invalid Huffman weights");
            }
            weights.push((rest.trailing_zeros() + 1) as u8);
            if max_bits > 11 {
                return err("Huffman codes too long");
            }
            let mut table = vec![(0u8, 0u8); 1 << max_bits];
            let mut position = 0usize;
            for w in 1..=max_bits as u8 {
                for (symbol, _) in weights.iter().enumerate().filter(|&(_, &x)| x == w) {
                    let n = 1usize << (w - 1);
                    for slot in &mut table[position..position + n] {
                        *slot = (symbol as u8, (max_bits + 1) as u8 - w);
                    }
                    position += n;
                }
            }
            if position != table.len() {
                return err("invalid Huffman weights");
            }
            Ok((Self { max_bits, table }, used))
        }

        fn decode_stream(&self, data: &[u8], count: usize, out: &mut Vec<u8>) -> Result<()> {
            let mut bits = Backward::new(data)?;
            for _ in 0..count {
                let (symbol, n) = self.table[bits.peek(self.max_bits) as usize];
                bits.consume(u32::from(n));
                out.push(symbol);
            }
            if bits.left != 0 {
                return err("Huffman stream not fully read");
            }
            Ok(())
        }
    }

    const LL_BASE: [u32; 36] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 20, 22, 24, 28, 32, 40, 48, 64, 128, 256, 512,
        1024, 2048, 4096, 8192, 16384, 32768, 65536,
    ];
    const LL_BITS: [u8; 36] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 3, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
    ];
    const ML_BASE: [u32; 53] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
        33, 34, 35, 37, 39, 41, 43, 47, 51, 59, 67, 83, 99, 131, 259, 515, 1027, 2051, 4099, 8195, 16387, 32771, 65539,
    ];
    const ML_BITS: [u8; 53] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2,
        3, 3, 4, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
    ];
    const LL_DEFAULT: [i16; 36] = [
        4, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 2, 1, 1, 1, 1, 1, -1, -1, -1, -1,
    ];
    const ML_DEFAULT: [i16; 53] = [
        1, 4, 3, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1, -1, -1,
    ];
    const OF_DEFAULT: [i16; 29] = [
        1, 1, 1, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1,
    ];

    struct Context {
        huffman: Option<Huffman>,
        ll: Option<Fse>,
        of: Option<Fse>,
        ml: Option<Fse>,
        reps: [usize; 3],
    }

    pub fn decode(data: &[u8]) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < data.len() {
            let magic = u32::from_le_bytes(data.get(i..i + 4).ok_or("truncated Zstandard frame")?.try_into().unwrap());
            if magic & 0xFFFF_FFF0 == 0x184D_2A50 {
                let size = u32::from_le_bytes(data.get(i + 4..i + 8).ok_or("truncated skippable frame")?.try_into().unwrap());
                i += 8 + size as usize;
                continue;
            }
            if magic != MAGIC {
                return err("not a Zstandard frame");
            }
            i = frame(data, i + 4, &mut out)?;
        }
        Ok(out)
    }

    fn frame(data: &[u8], mut i: usize, out: &mut Vec<u8>) -> Result<usize> {
        let start = out.len();
        let descriptor = *data.get(i).ok_or("truncated frame header")?;
        i += 1;
        let fcs_flag = descriptor >> 6;
        let single_segment = descriptor & 0x20 != 0;
        if descriptor & 0x08 != 0 {
            return err("reserved frame header bit set");
        }
        let checksum = descriptor & 0x04 != 0;
        let dictionary_bytes = [0, 1, 2, 4][usize::from(descriptor & 3)];
        if !single_segment {
            i += 1;
        }
        let dictionary = data.get(i..i + dictionary_bytes).ok_or("truncated frame header")?;
        if dictionary.iter().any(|&b| b != 0) {
            return err("Zstandard dictionaries are not supported");
        }
        i += dictionary_bytes;
        let fcs_bytes = match fcs_flag {
            0 if single_segment => 1,
            0 => 0,
            1 => 2,
            2 => 4,
            _ => 8,
        };
        let mut content_size: Option<u64> = None;
        if fcs_bytes > 0 {
            let raw = data.get(i..i + fcs_bytes).ok_or("truncated frame header")?;
            let mut v = raw.iter().rev().fold(0u64, |a, &b| a << 8 | u64::from(b));
            if fcs_bytes == 2 {
                v += 256;
            }
            content_size = Some(v);
            i += fcs_bytes;
        }
        if content_size.is_some_and(|s| s > MAX_OUTPUT as u64) {
            return err("Zstandard frame too large");
        }
        let mut ctx = Context {
            huffman: None,
            ll: None,
            of: None,
            ml: None,
            reps: [1, 4, 8],
        };
        loop {
            let header = data.get(i..i + 3).ok_or("truncated block header")?;
            let header = u32::from(header[0]) | u32::from(header[1]) << 8 | u32::from(header[2]) << 16;
            i += 3;
            let last = header & 1 != 0;
            let size = (header >> 3) as usize;
            match (header >> 1) & 3 {
                0 => {
                    let raw = data.get(i..i + size).ok_or("truncated raw block")?;
                    check_room(out, size)?;
                    out.extend_from_slice(raw);
                    i += size;
                }
                1 => {
                    let b = *data.get(i).ok_or("truncated RLE block")?;
                    check_room(out, size)?;
                    out.resize(out.len() + size, b);
                    i += 1;
                }
                2 => {
                    if size > MAX_BLOCK {
                        return err("Zstandard block too large");
                    }
                    let block = data.get(i..i + size).ok_or("truncated compressed block")?;
                    compressed_block(block, &mut ctx, out)?;
                    i += size;
                }
                _ => return err("reserved block type"),
            }
            if last {
                break;
            }
        }
        if content_size.is_some_and(|s| s != (out.len() - start) as u64) {
            return err("Zstandard frame size mismatch");
        }
        if checksum {
            let sum = u32::from_le_bytes(data.get(i..i + 4).ok_or("truncated frame checksum")?.try_into().unwrap());
            if hash::xxh64(&out[start..], 0) as u32 != sum {
                return err("Zstandard checksum mismatch");
            }
            i += 4;
        }
        Ok(i)
    }

    fn check_room(out: &[u8], more: usize) -> Result<()> {
        if out.len() + more > MAX_OUTPUT { err("Zstandard output too large") } else { Ok(()) }
    }

    fn compressed_block(block: &[u8], ctx: &mut Context, out: &mut Vec<u8>) -> Result<()> {
        let (literals, used) = literals(block, ctx)?;
        let rest = &block[used..];
        let count_byte = *rest.first().ok_or("missing sequences header")?;
        let (count, mut i) = match count_byte {
            0 => (0usize, 1usize),
            1..=127 => (usize::from(count_byte), 1),
            128..=254 => {
                let b1 = *rest.get(1).ok_or("truncated sequences header")?;
                ((usize::from(count_byte) - 128) << 8 | usize::from(b1), 2)
            }
            255 => {
                let b = rest.get(1..3).ok_or("truncated sequences header")?;
                (usize::from(b[0]) | usize::from(b[1]) << 8 | 0x7F00, 3)
            }
        };
        if count == 0 {
            check_room(out, literals.len())?;
            out.extend_from_slice(&literals);
            return Ok(());
        }
        let modes = *rest.get(i).ok_or("truncated sequences header")?;
        i += 1;
        if modes & 3 != 0 {
            return err("reserved sequence mode bits set");
        }
        let mut table = |mode: u8, slot: &mut Option<Fse>, default: &[i16], default_log: u32, max_symbol: usize, max_log: u32| -> Result<()> {
            match mode {
                0 => *slot = Some(Fse::build(default, default_log)?),
                1 => {
                    let s = *rest.get(i).ok_or("truncated RLE table")?;
                    if usize::from(s) > max_symbol {
                        return err("invalid RLE symbol");
                    }
                    *slot = Some(Fse::rle(s));
                    i += 1;
                }
                2 => {
                    let (fse, n) = Fse::read(rest.get(i..).ok_or("truncated FSE table")?, max_symbol, max_log)?;
                    *slot = Some(fse);
                    i += n;
                }
                _ => {
                    if slot.is_none() {
                        return err("repeat table with none before");
                    }
                }
            }
            Ok(())
        };
        table(modes >> 6, &mut ctx.ll, &LL_DEFAULT, 6, 35, 9)?;
        table((modes >> 4) & 3, &mut ctx.of, &OF_DEFAULT, 5, 31, 8)?;
        table((modes >> 2) & 3, &mut ctx.ml, &ML_DEFAULT, 6, 52, 9)?;
        let (Some(ll), Some(of), Some(ml)) = (&ctx.ll, &ctx.of, &ctx.ml) else {
            return err("missing sequence tables");
        };
        let mut bits = Backward::new(rest.get(i..).ok_or("missing sequence stream")?)?;
        let mut ll_state = State::new(ll, &mut bits);
        let mut of_state = State::new(of, &mut bits);
        let mut ml_state = State::new(ml, &mut bits);
        let mut lit = 0usize;
        for n in 0..count {
            let of_code = u32::from(of_state.symbol());
            let ll_code = usize::from(ll_state.symbol());
            let ml_code = usize::from(ml_state.symbol());
            if of_code > 31 || ll_code >= LL_BASE.len() || ml_code >= ML_BASE.len() {
                return err("invalid sequence code");
            }
            let offset_value = (1u64 << of_code) + bits.read(of_code);
            let match_len = (ML_BASE[ml_code] as u64 + bits.read(u32::from(ML_BITS[ml_code]))) as usize;
            let lit_len = (LL_BASE[ll_code] as u64 + bits.read(u32::from(LL_BITS[ll_code]))) as usize;
            if n + 1 < count {
                ll_state.update(&mut bits);
                ml_state.update(&mut bits);
                of_state.update(&mut bits);
            }
            let reps = &mut ctx.reps;
            let offset = if offset_value > 3 {
                let o = (offset_value - 3) as usize;
                *reps = [o, reps[0], reps[1]];
                o
            } else {
                let index = offset_value as usize + usize::from(lit_len == 0);
                match index {
                    1 => reps[0],
                    2 => {
                        let o = reps[1];
                        *reps = [o, reps[0], reps[2]];
                        o
                    }
                    3 => {
                        let o = reps[2];
                        *reps = [o, reps[0], reps[1]];
                        o
                    }
                    _ => {
                        let o = reps[0].wrapping_sub(1);
                        if o == 0 {
                            return err("invalid repeat offset");
                        }
                        *reps = [o, reps[0], reps[1]];
                        o
                    }
                }
            };
            let run = literals.get(lit..lit + lit_len).ok_or("sequence past the literals")?;
            check_room(out, lit_len + match_len)?;
            out.extend_from_slice(run);
            lit += lit_len;
            if offset == 0 || offset > out.len() {
                return err("invalid match offset");
            }
            let from = out.len() - offset;
            if offset >= match_len {
                out.extend_from_within(from..from + match_len);
            } else {
                for k in 0..match_len {
                    let b = out[from + k];
                    out.push(b);
                }
            }
        }
        if bits.left != 0 {
            return err("sequence stream not fully read");
        }
        let tail = &literals[lit..];
        check_room(out, tail.len())?;
        out.extend_from_slice(tail);
        Ok(())
    }

    fn literals(block: &[u8], ctx: &mut Context) -> Result<(Vec<u8>, usize)> {
        let b0 = *block.first().ok_or("missing literals header")?;
        let kind = b0 & 3;
        let format = (b0 >> 2) & 3;
        let byte = |k: usize| -> Result<usize> { block.get(k).map(|&b| usize::from(b)).ok_or_else(|| "truncated literals header".to_owned()) };
        if kind < 2 {
            let (size, header) = match format {
                0 | 2 => (usize::from(b0 >> 3), 1),
                1 => (usize::from(b0 >> 4) | byte(1)? << 4, 2),
                _ => (usize::from(b0 >> 4) | byte(1)? << 4 | byte(2)? << 12, 3),
            };
            if size > MAX_BLOCK {
                return err("literals too large");
            }
            return if kind == 0 {
                let raw = block.get(header..header + size).ok_or("truncated raw literals")?;
                Ok((raw.to_vec(), header + size))
            } else {
                let b = *block.get(header).ok_or("truncated RLE literals")?;
                Ok((vec![b; size], header + 1))
            };
        }
        let (regen, compressed, header, streams) = match format {
            0 | 1 => {
                let c = usize::from(b0) | byte(1)? << 8 | byte(2)? << 16;
                ((c >> 4) & 0x3FF, (c >> 14) & 0x3FF, 3, if format == 0 { 1 } else { 4 })
            }
            2 => {
                let c = usize::from(b0) | byte(1)? << 8 | byte(2)? << 16 | byte(3)? << 24;
                ((c >> 4) & 0x3FFF, (c >> 18) & 0x3FFF, 4, 4)
            }
            _ => {
                let c = usize::from(b0) | byte(1)? << 8 | byte(2)? << 16 | byte(3)? << 24 | byte(4)? << 32;
                ((c >> 4) & 0x3FFFF, (c >> 22) & 0x3FFFF, 5, 4)
            }
        };
        if regen > MAX_BLOCK {
            return err("literals too large");
        }
        let body = block.get(header..header + compressed).ok_or("truncated compressed literals")?;
        let mut at = 0;
        if kind == 2 {
            let (tree, n) = Huffman::read(body)?;
            ctx.huffman = Some(tree);
            at = n;
        }
        let huffman = ctx.huffman.as_ref().ok_or("treeless literals with no tree before")?;
        let mut literals = Vec::with_capacity(regen);
        if streams == 1 {
            huffman.decode_stream(body.get(at..).ok_or("truncated literals")?, regen, &mut literals)?;
        } else {
            let jump = body.get(at..at + 6).ok_or("truncated jump table")?;
            let sizes = [0, 2, 4].map(|k| usize::from(u16::from_le_bytes([jump[k], jump[k + 1]])));
            let mut start = at + 6;
            let each = regen.div_ceil(4);
            for s in 0..4 {
                let end = sizes.get(s).map_or(body.len(), |n| start + n);
                let stream = body.get(start..end).ok_or("truncated literal stream")?;
                let count = if s < 3 { each } else { regen.checked_sub(3 * each).ok_or("invalid literal streams")? };
                huffman.decode_stream(stream, count, &mut literals)?;
                start = end;
            }
        }
        Ok((literals, header + compressed))
    }
}

pub mod xz {
    use super::{MAX_OUTPUT, Result, err, hash};

    const MAGIC: [u8; 6] = [0xFD, b'7', b'z', b'X', b'Z', 0];

    fn varint(data: &[u8], i: &mut usize) -> Result<u64> {
        let mut value = 0u64;
        for shift in (0..63).step_by(7) {
            let b = *data.get(*i).ok_or("truncated xz number")?;
            *i += 1;
            value |= u64::from(b & 0x7F) << shift;
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        err("xz number too long")
    }

    pub fn decode(data: &[u8]) -> Result<Vec<u8>> {
        let header = data.get(..12).ok_or("truncated xz header")?;
        if header[..6] != MAGIC {
            return err("not an xz stream");
        }
        if hash::crc32(&header[6..8]) != u32::from_le_bytes(header[8..12].try_into().unwrap()) {
            return err("xz header checksum mismatch");
        }
        if header[6] != 0 {
            return err("invalid xz stream flags");
        }
        let check = header[7] & 0x0F;
        let check_size = match check {
            0 => 0,
            1..=3 => 4,
            4..=6 => 8,
            7..=9 => 16,
            10..=12 => 32,
            _ => 64,
        };
        let mut out = Vec::new();
        let mut i = 12;
        loop {
            let size_byte = *data.get(i).ok_or("truncated xz block")?;
            if size_byte == 0 {
                return Ok(out);
            }
            let block_start = i;
            let header_size = (usize::from(size_byte) + 1) * 4;
            let block_header = data.get(i..i + header_size).ok_or("truncated xz block header")?;
            let stored = u32::from_le_bytes(block_header[header_size - 4..].try_into().unwrap());
            if hash::crc32(&block_header[..header_size - 4]) != stored {
                return err("xz block header checksum mismatch");
            }
            let flags = block_header[1];
            let filters = usize::from(flags & 3) + 1;
            let mut k = 2;
            if flags & 0x40 != 0 {
                varint(block_header, &mut k)?;
            }
            if flags & 0x80 != 0 {
                varint(block_header, &mut k)?;
            }
            let mut dict_props = None;
            for f in 0..filters {
                let id = varint(block_header, &mut k)?;
                let props = varint(block_header, &mut k)? as usize;
                if f + 1 != filters || id != 0x21 || props != 1 {
                    return err("only the LZMA2 filter is supported in xz");
                }
                dict_props = Some(*block_header.get(k).ok_or("truncated xz filter")?);
                k += 1;
            }
            if dict_props.is_none_or(|d| d > 40) {
                return err("invalid LZMA2 dictionary size");
            }
            i += header_size;
            let start = out.len();
            i = lzma2(data, i, &mut out)?;
            if i == data.len() {
                return Ok(out);
            }
            while (i - block_start) % 4 != 0 {
                if *data.get(i).ok_or("truncated xz block padding")? != 0 {
                    return err("invalid xz block padding");
                }
                i += 1;
            }
            let sum = data.get(i..i + check_size).ok_or("truncated xz check")?;
            let ok = match check {
                1 => hash::crc32(&out[start..]).to_le_bytes() == sum,
                4 => hash::crc64(&out[start..]).to_le_bytes() == sum,
                _ => true,
            };
            if !ok {
                return err("xz check mismatch");
            }
            i += check_size;
        }
    }

    pub fn lzma2(data: &[u8], mut i: usize, out: &mut Vec<u8>) -> Result<usize> {
        let mut lzma: Option<Lzma> = None;
        let mut need_dictionary = true;
        let mut need_props = true;
        loop {
            if i == data.len() && !need_dictionary {
                return Ok(i);
            }
            let control = *data.get(i).ok_or("truncated LZMA2 chunk")?;
            i += 1;
            if control == 0 {
                return Ok(i);
            }
            let size_bytes = data.get(i..i + 2).ok_or("truncated LZMA2 chunk")?;
            let mut unpacked = usize::from(u16::from_be_bytes([size_bytes[0], size_bytes[1]])) + 1;
            i += 2;
            if control < 0x80 {
                if control > 2 {
                    return err("invalid LZMA2 control byte");
                }
                if control == 2 && need_dictionary {
                    return err("LZMA2 chunk needs a dictionary reset first");
                }
                need_dictionary = false;
                let raw = data.get(i..i + unpacked).ok_or("truncated LZMA2 chunk")?;
                if out.len() + unpacked > MAX_OUTPUT {
                    return err("LZMA2 output too large");
                }
                out.extend_from_slice(raw);
                i += unpacked;
                continue;
            }
            unpacked += usize::from(control & 0x1F) << 16;
            let packed_bytes = data.get(i..i + 2).ok_or("truncated LZMA2 chunk")?;
            let packed = usize::from(u16::from_be_bytes([packed_bytes[0], packed_bytes[1]])) + 1;
            i += 2;
            let reset = (control >> 5) & 3;
            if reset == 3 {
                need_dictionary = false;
            } else if need_dictionary {
                return err("LZMA2 chunk needs a dictionary reset first");
            }
            if reset >= 2 {
                let props = *data.get(i).ok_or("truncated LZMA2 properties")?;
                i += 1;
                lzma = Some(Lzma::new(props)?);
                need_props = false;
            } else if need_props {
                return err("LZMA2 chunk needs properties first");
            } else if reset == 1
                && let Some(l) = &mut lzma
            {
                l.reset();
            }
            let chunk = data.get(i..i + packed).ok_or("truncated LZMA2 chunk")?;
            if out.len() + unpacked > MAX_OUTPUT {
                return err("LZMA2 output too large");
            }
            lzma.as_mut().ok_or("LZMA2 chunk with no properties")?.decode(chunk, out, unpacked)?;
            i += packed;
        }
    }

    const STATES: usize = 12;
    const POS_STATES_MAX: usize = 16;
    const END_POS_MODEL_INDEX: usize = 14;
    const FULL_DISTANCES: usize = 128;
    const BIT_MODEL_TOTAL: u32 = 1 << 11;
    const INIT: u16 = (BIT_MODEL_TOTAL / 2) as u16;

    struct RangeDecoder<'a> {
        data: &'a [u8],
        at: usize,
        range: u32,
        code: u32,
    }

    impl<'a> RangeDecoder<'a> {
        fn new(data: &'a [u8]) -> Result<Self> {
            if data.len() < 5 || data[0] != 0 {
                return err("invalid LZMA range coder start");
            }
            Ok(Self {
                data,
                at: 5,
                range: u32::MAX,
                code: u32::from_be_bytes(data[1..5].try_into().unwrap()),
            })
        }
        fn normalize(&mut self) {
            if self.range < 1 << 24 {
                self.range <<= 8;
                let b = self.data.get(self.at).copied().unwrap_or(0);
                self.at += 1;
                self.code = (self.code << 8) | u32::from(b);
            }
        }
        fn bit(&mut self, prob: &mut u16) -> u32 {
            let bound = (self.range >> 11) * u32::from(*prob);
            let bit = if self.code < bound {
                self.range = bound;
                *prob += ((BIT_MODEL_TOTAL - u32::from(*prob)) >> 5) as u16;
                0
            } else {
                self.range -= bound;
                self.code -= bound;
                *prob -= *prob >> 5;
                1
            };
            self.normalize();
            bit
        }
        fn tree(&mut self, probs: &mut [u16], bits: u32) -> u32 {
            let mut m = 1usize;
            for _ in 0..bits {
                m = (m << 1) | self.bit(&mut probs[m]) as usize;
            }
            (m - (1 << bits)) as u32
        }
        fn reverse_tree(&mut self, probs: &mut [u16], offset: usize, bits: u32) -> u32 {
            let mut m = 1usize;
            let mut symbol = 0;
            for k in 0..bits {
                let bit = self.bit(&mut probs[offset + m - 1]);
                m = (m << 1) | bit as usize;
                symbol |= bit << k;
            }
            symbol
        }
        fn direct(&mut self, bits: u32) -> u32 {
            let mut result = 0u32;
            for _ in 0..bits {
                self.range >>= 1;
                self.code = self.code.wrapping_sub(self.range);
                let t = 0u32.wrapping_sub(self.code >> 31);
                self.code = self.code.wrapping_add(self.range & t);
                result = (result << 1).wrapping_add(t.wrapping_add(1));
                self.normalize();
            }
            result
        }
    }

    #[derive(Clone)]
    struct LenDecoder {
        choice: u16,
        choice2: u16,
        low: [[u16; 8]; POS_STATES_MAX],
        mid: [[u16; 8]; POS_STATES_MAX],
        high: [u16; 256],
    }

    impl LenDecoder {
        fn new() -> Self {
            Self {
                choice: INIT,
                choice2: INIT,
                low: [[INIT; 8]; POS_STATES_MAX],
                mid: [[INIT; 8]; POS_STATES_MAX],
                high: [INIT; 256],
            }
        }
        fn decode(&mut self, rc: &mut RangeDecoder, pos_state: usize) -> usize {
            if rc.bit(&mut self.choice) == 0 {
                rc.tree(&mut self.low[pos_state], 3) as usize + 2
            } else if rc.bit(&mut self.choice2) == 0 {
                rc.tree(&mut self.mid[pos_state], 3) as usize + 10
            } else {
                rc.tree(&mut self.high, 8) as usize + 18
            }
        }
    }

    struct Lzma {
        lc: u32,
        lp: u32,
        pb: u32,
        state: usize,
        reps: [usize; 4],
        is_match: [u16; STATES * POS_STATES_MAX],
        is_rep: [u16; STATES],
        is_rep0: [u16; STATES],
        is_rep1: [u16; STATES],
        is_rep2: [u16; STATES],
        is_rep0_long: [u16; STATES * POS_STATES_MAX],
        dist_slot: [[u16; 64]; 4],
        dist_special: [u16; FULL_DISTANCES - END_POS_MODEL_INDEX],
        dist_align: [u16; 16],
        match_len: LenDecoder,
        rep_len: LenDecoder,
        literal: Vec<u16>,
    }

    impl Lzma {
        fn new(props: u8) -> Result<Self> {
            if props >= 9 * 5 * 5 {
                return err("invalid LZMA properties");
            }
            let (lc, lp, pb) = (u32::from(props % 9), u32::from(props / 9 % 5), u32::from(props / 45));
            if lc + lp > 4 {
                return err("invalid LZMA2 properties");
            }
            Ok(Self {
                lc,
                lp,
                pb,
                state: 0,
                reps: [0; 4],
                is_match: [INIT; STATES * POS_STATES_MAX],
                is_rep: [INIT; STATES],
                is_rep0: [INIT; STATES],
                is_rep1: [INIT; STATES],
                is_rep2: [INIT; STATES],
                is_rep0_long: [INIT; STATES * POS_STATES_MAX],
                dist_slot: [[INIT; 64]; 4],
                dist_special: [INIT; FULL_DISTANCES - END_POS_MODEL_INDEX],
                dist_align: [INIT; 16],
                match_len: LenDecoder::new(),
                rep_len: LenDecoder::new(),
                literal: vec![INIT; 0x300 << (lc + lp)],
            })
        }

        fn reset(&mut self) {
            if let Ok(fresh) = Self::new((self.pb * 5 + self.lp) as u8 * 9 + self.lc as u8) {
                *self = fresh;
            }
        }

        fn decode(&mut self, chunk: &[u8], out: &mut Vec<u8>, unpacked: usize) -> Result<()> {
            let mut rc = RangeDecoder::new(chunk)?;
            let end = out.len() + unpacked;
            let pb_mask = (1usize << self.pb) - 1;
            let lp_mask = (1usize << self.lp) - 1;
            while out.len() < end {
                let pos = out.len();
                let pos_state = pos & pb_mask;
                let s = self.state;
                if rc.bit(&mut self.is_match[s * POS_STATES_MAX + pos_state]) == 0 {
                    let prev = out.last().copied().unwrap_or(0);
                    let base = 0x300 * (((pos & lp_mask) << self.lc) + (usize::from(prev) >> (8 - self.lc)));
                    let probs = &mut self.literal[base..base + 0x300];
                    let mut symbol = 1usize;
                    if s >= 7 {
                        let mut match_byte =
                            usize::from(*out.get(pos.wrapping_sub(self.reps[0] + 1)).ok_or("LZMA match before the start")?);
                        loop {
                            let match_bit = (match_byte >> 7) & 1;
                            match_byte <<= 1;
                            let bit = rc.bit(&mut probs[0x100 + (match_bit << 8) + symbol]) as usize;
                            symbol = (symbol << 1) | bit;
                            if match_bit != bit || symbol >= 0x100 {
                                break;
                            }
                        }
                    }
                    while symbol < 0x100 {
                        symbol = (symbol << 1) | rc.bit(&mut probs[symbol]) as usize;
                    }
                    out.push(symbol as u8);
                    self.state = if s < 4 {
                        0
                    } else if s < 10 {
                        s - 3
                    } else {
                        s - 6
                    };
                    continue;
                }
                let len;
                if rc.bit(&mut self.is_rep[s]) == 0 {
                    self.reps = [0, self.reps[0], self.reps[1], self.reps[2]];
                    len = self.match_len.decode(&mut rc, pos_state);
                    self.state = if s < 7 { 7 } else { 10 };
                    let len_state = (len - 2).min(3);
                    let slot = rc.tree(&mut self.dist_slot[len_state], 6) as usize;
                    let distance = if slot < 4 {
                        slot
                    } else {
                        let footer = ((slot >> 1) - 1) as u32;
                        let base = (2 | (slot & 1)) << footer;
                        if slot < END_POS_MODEL_INDEX {
                            base + rc.reverse_tree(&mut self.dist_special, base - slot, footer) as usize
                        } else {
                            base + ((rc.direct(footer - 4) as usize) << 4) + rc.reverse_tree(&mut self.dist_align, 1, 4) as usize
                        }
                    };
                    if distance == u32::MAX as usize {
                        return err("unexpected LZMA end marker");
                    }
                    self.reps[0] = distance;
                } else {
                    if rc.bit(&mut self.is_rep0[s]) == 0 {
                        if rc.bit(&mut self.is_rep0_long[s * POS_STATES_MAX + pos_state]) == 0 {
                            self.state = if s < 7 { 9 } else { 11 };
                            let b = *out.get(pos.wrapping_sub(self.reps[0] + 1)).ok_or("LZMA match before the start")?;
                            out.push(b);
                            continue;
                        }
                    } else {
                        let distance;
                        if rc.bit(&mut self.is_rep1[s]) == 0 {
                            distance = self.reps[1];
                        } else {
                            if rc.bit(&mut self.is_rep2[s]) == 0 {
                                distance = self.reps[2];
                            } else {
                                distance = self.reps[3];
                                self.reps[3] = self.reps[2];
                            }
                            self.reps[2] = self.reps[1];
                        }
                        self.reps[1] = self.reps[0];
                        self.reps[0] = distance;
                    }
                    len = self.rep_len.decode(&mut rc, pos_state);
                    self.state = if s < 7 { 8 } else { 11 };
                }
                let distance = self.reps[0] + 1;
                if distance > out.len() {
                    return err("LZMA match before the start");
                }
                let len = len.min(end - out.len());
                let from = out.len() - distance;
                for k in 0..len {
                    let b = out[from + k];
                    out.push(b);
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn sample(len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        let mut x: u32 = 12345;
        while out.len() < len {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            match (x >> 16) % 5 {
                0 => out.extend(std::iter::repeat_n((x >> 8) as u8, (x % 40) as usize)),
                1 => out.extend_from_slice(b"minecraft:grass_block_STATE_{snowy:false}"),
                2 if out.len() > 300 => {
                    let start = out.len() - 1 - (x as usize % 300);
                    let n = (x as usize >> 3) % 64;
                    for k in 0..n {
                        let b = out[start + k % (out.len() - start)];
                        out.push(b);
                    }
                }
                3 => out.extend((0..(x % 20)).map(|k| (x >> (k % 24)) as u8)),
                _ => out.push((x >> 24) as u8),
            }
        }
        out.truncate(len);
        out
    }

    #[test]
    fn hashes_match_reference_values() {
        assert_eq!(hash::xxh32(b"", 0), 0x02CC_5D05);
        assert_eq!(hash::xxh64(b"", 0), 0xEF46_DB37_51D8_E999);
        assert_eq!(hash::crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(hash::crc64(b"123456789"), 0x995D_C9BB_DF19_39FA);
    }

    #[test]
    fn decodes_reference_fixtures() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/fixtures");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            assert_ne!(env!("CARGO_PKG_NAME"), "distant-horizons-for-horizon-client", "missing {}", dir.display());
            return;
        };
        let mut seen = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some((stem, ext)) = name.rsplit_once('.') else { continue };
            let Some((_, len)) = stem.rsplit_once('-') else { continue };
            let Ok(len) = len.parse::<usize>() else { continue };
            let m = match ext {
                "zst" => mode::Z_STD_BLOCK,
                "lz4" => mode::LZ4,
                "xz" => mode::LZMA2,
                _ => continue,
            };
            let data = std::fs::read(entry.path()).unwrap();
            let decoded = decompress(m, &data).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(decoded, sample(len), "{name}");
            seen += 1;
        }
        assert!(seen >= 9, "only {seen} fixtures");
    }

    #[test]
    fn rejects_garbage() {
        for m in [mode::LZ4, mode::Z_STD_BLOCK, mode::LZMA2] {
            assert!(decompress(m, b"definitely not compressed data").is_err());
            assert!(decompress(m, &[]).is_ok_and(|v| v.is_empty()) || decompress(m, &[]).is_err());
        }
    }
}
