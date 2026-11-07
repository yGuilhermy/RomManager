use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;

fn read_u64_be(b: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(b[offset..offset + 8].try_into().unwrap_or([0; 8]))
}

fn read_u32_be(b: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(b[offset..offset + 4].try_into().unwrap_or([0; 4]))
}

fn lzma_dict_size(hunk_bytes: u32) -> u32 {
    let mut dict_size = hunk_bytes.max(4096);
    if dict_size >= (1 << 21) {
        let mask = (1 << 20) - 1;
        dict_size = (dict_size + mask) & !mask;
    } else {
        let mut i = 11 * 2;
        loop {
            let aligned = (2 + (i & 1)) << (i >> 1);
            if aligned >= dict_size {
                return aligned;
            }
            i += 1;
        }
    }
    dict_size
}

struct BitStream<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitStream<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    fn read(&mut self, n: usize) -> u32 {
        let mut v = 0u32;
        for _ in 0..n {
            let byte_idx = self.bit_pos >> 3;
            let bit = if byte_idx < self.data.len() {
                let shift = 7 - (self.bit_pos & 7);
                ((self.data[byte_idx] >> shift) & 1) as u32
            } else {
                0
            };
            v = (v << 1) | bit;
            self.bit_pos += 1;
        }
        v
    }
}

struct SimpleHuffman {
    lookup: [u16; 256],
}

impl SimpleHuffman {
    fn from_bitstream(bs: &mut BitStream) -> Option<Self> {
        let mut bit_lengths = [0u8; 16];
        let mut cur = 0;
        while cur < 16 {
            let nodebits = bs.read(4) as u8;
            if nodebits != 1 {
                bit_lengths[cur] = nodebits;
                cur += 1;
            } else {
                let nodebits2 = bs.read(4) as u8;
                if nodebits2 == 1 {
                    bit_lengths[cur] = 1;
                    cur += 1;
                } else {
                    let mut repcount = (bs.read(4) + 3) as usize;
                    while repcount > 0 && cur < 16 {
                        bit_lengths[cur] = nodebits2;
                        cur += 1;
                        repcount -= 1;
                    }
                }
            }
        }

        let mut bithisto = [0u32; 33];
        for &len in &bit_lengths {
            if len > 8 {
                return None;
            }
            bithisto[len as usize] += 1;
        }

        let mut curstart = 0u32;
        for codelen in (1..=32).rev() {
            let nextstart = (curstart + bithisto[codelen]) >> 1;
            bithisto[codelen] = curstart;
            curstart = nextstart;
        }

        let mut codes = [0u16; 16];
        for (i, &len) in bit_lengths.iter().enumerate() {
            if len > 0 {
                codes[i] = bithisto[len as usize] as u16;
                bithisto[len as usize] += 1;
            }
        }

        let mut lookup = [0u16; 256];
        for (symbol, &len) in bit_lengths.iter().enumerate() {
            if len > 0 {
                let code = codes[symbol] as usize;
                let shift = 8 - len as usize;
                let start = code << shift;
                let end = ((code + 1) << shift) - 1;
                let val = ((symbol as u16) << 5) | (len as u16 & 0x1F);
                for slot in start..=end {
                    lookup[slot] = val;
                }
            }
        }

        Some(Self { lookup })
    }

    fn decode_one(&self, bs: &mut BitStream) -> u8 {
        let byte_idx = bs.bit_pos >> 3;
        let shift = bs.bit_pos & 7;
        let b0 = bs.data.get(byte_idx).copied().unwrap_or(0);
        let b1 = bs.data.get(byte_idx + 1).copied().unwrap_or(0);
        let peek8 = ((((b0 as u16) << 8) | (b1 as u16)) >> (8 - shift)) as u8;
        let entry = self.lookup[peek8 as usize];
        let symbol = (entry >> 5) as u8;
        let bits = (entry & 0x1F) as usize;
        bs.bit_pos += bits.max(1);
        symbol
    }
}

#[derive(Clone)]
struct MapEntry {
    comp_type: u8,
    offset: u64,
    length: u32,
}

pub struct ChdReader {
    file: File,
    hunk_bytes: u32,
    unit_bytes: u32,
    codecs: [u32; 4],
    entries: Vec<MapEntry>,
    cached_hunk: Option<(usize, Vec<u8>)>,
}

impl ChdReader {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let mut hdr = [0u8; 124];
        file.read_exact(&mut hdr).map_err(|e| e.to_string())?;
        if &hdr[0..8] != b"MComprHD" {
            return Err("Assinatura CHD inválida".into());
        }
        let ver = read_u32_be(&hdr, 12);
        if ver != 5 {
            return Err(format!("Versão CHD não suportada: {ver}"));
        }
        let codecs = [
            read_u32_be(&hdr, 16),
            read_u32_be(&hdr, 20),
            read_u32_be(&hdr, 24),
            read_u32_be(&hdr, 28),
        ];
        let logical_bytes = read_u64_be(&hdr, 32);
        let map_offset = read_u64_be(&hdr, 40);
        let hunk_bytes = read_u32_be(&hdr, 56);
        let unit_bytes = read_u32_be(&hdr, 60);
        if hunk_bytes == 0 || unit_bytes == 0 {
            return Err("Geometria de hunk CHD inválida".into());
        }
        let total_hunks = ((logical_bytes + hunk_bytes as u64 - 1) / hunk_bytes as u64) as usize;

        file.seek(SeekFrom::Start(map_offset))
            .map_err(|e| e.to_string())?;
        let mut mhdr = [0u8; 16];
        file.read_exact(&mut mhdr).map_err(|e| e.to_string())?;
        let map_bytes = read_u32_be(&mhdr, 0) as usize;
        let first_offs = {
            let b = &mhdr[4..10];
            ((b[0] as u64) << 40)
                | ((b[1] as u64) << 32)
                | ((b[2] as u64) << 24)
                | ((b[3] as u64) << 16)
                | ((b[4] as u64) << 8)
                | (b[5] as u64)
        };
        let length_bits = mhdr[12] as usize;
        let self_bits = mhdr[13] as usize;
        let parent_bits = mhdr[14] as usize;

        let mut comp_map = vec![0u8; map_bytes];
        file.read_exact(&mut comp_map).map_err(|e| e.to_string())?;

        let mut bs = BitStream::new(&comp_map);
        let huff = SimpleHuffman::from_bitstream(&mut bs)
            .ok_or_else(|| "Tabela Huffman da CHD inválida".to_string())?;

        let mut comp_types = vec![0u8; total_hunks];
        let mut repcount = 0usize;
        let mut lastcomp = 0u8;
        for hunknum in 0..total_hunks {
            if repcount > 0 {
                comp_types[hunknum] = lastcomp;
                repcount -= 1;
            } else {
                let val = huff.decode_one(&mut bs);
                if val == 7 {
                    comp_types[hunknum] = lastcomp;
                    repcount = 2 + huff.decode_one(&mut bs) as usize;
                } else if val == 8 {
                    comp_types[hunknum] = lastcomp;
                    let c1 = huff.decode_one(&mut bs) as usize;
                    let c2 = huff.decode_one(&mut bs) as usize;
                    repcount = 2 + 16 + (c1 << 4) + c2;
                } else {
                    comp_types[hunknum] = val;
                    lastcomp = val;
                }
            }
        }

        let mut entries = Vec::with_capacity(total_hunks);
        let mut cur_offset = first_offs;
        let mut last_self = 0u64;
        let mut last_parent = 0u64;

        for (hunknum, &c_type) in comp_types.iter().enumerate() {
            let mut t = c_type;
            let mut off = cur_offset;
            let mut len = 0u32;
            match t {
                0..=3 => {
                    len = bs.read(length_bits);
                    let _crc = bs.read(16);
                    cur_offset += len as u64;
                }
                4 => {
                    len = hunk_bytes;
                    let _crc = bs.read(16);
                    cur_offset += len as u64;
                }
                5 => {
                    off = bs.read(self_bits) as u64;
                    last_self = off;
                }
                6 => {
                    off = bs.read(parent_bits) as u64;
                    last_parent = off;
                }
                10 => {
                    last_self += 1;
                    t = 5;
                    off = last_self;
                }
                9 => {
                    t = 5;
                    off = last_self;
                }
                13 => {
                    t = 6;
                    off = ((hunknum as u64) * (hunk_bytes as u64)) / (unit_bytes as u64);
                    last_parent = off;
                }
                12 => {
                    last_parent += (hunk_bytes / unit_bytes) as u64;
                    t = 6;
                    off = last_parent;
                }
                11 => {
                    t = 6;
                    off = last_parent;
                }
                _ => {}
            }
            entries.push(MapEntry {
                comp_type: t,
                offset: off,
                length: len,
            });
        }

        Ok(Self {
            file,
            hunk_bytes,
            unit_bytes,
            codecs,
            entries,
            cached_hunk: None,
        })
    }

    pub fn read_hunk(&mut self, hunk_idx: usize) -> Result<Vec<u8>, String> {
        if let Some((idx, ref data)) = self.cached_hunk {
            if idx == hunk_idx {
                return Ok(data.clone());
            }
        }

        let entry = self
            .entries
            .get(hunk_idx)
            .cloned()
            .ok_or("Índice de hunk fora dos limites")?;

        let actual_entry = entry;
        if actual_entry.comp_type == 5 {
            let ref_idx = actual_entry.offset as usize;
            return self.read_hunk(ref_idx);
        }

        if actual_entry.comp_type == 6 {
            let zeroed = vec![0u8; self.hunk_bytes as usize];
            self.cached_hunk = Some((hunk_idx, zeroed.clone()));
            return Ok(zeroed);
        }

        self.file
            .seek(SeekFrom::Start(actual_entry.offset))
            .map_err(|e| e.to_string())?;
        let mut comp_data = vec![0u8; actual_entry.length as usize];
        self.file
            .read_exact(&mut comp_data)
            .map_err(|e| e.to_string())?;

        let mut hunk_data = vec![0u8; self.hunk_bytes as usize];
        let codec = if actual_entry.comp_type < 4 {
            self.codecs[actual_entry.comp_type as usize]
        } else {
            0
        };

        if actual_entry.comp_type == 4 {
            hunk_data = comp_data;
        } else if codec == 0x6c7a6d61 {
            let dict_size = lzma_dict_size(self.hunk_bytes);
            let mut reader = lzma_rust::LZMAReader::new_with_props(
                Cursor::new(comp_data),
                self.hunk_bytes as u64,
                93,
                dict_size,
                None,
            )
            .map_err(|e| e.to_string())?;
            reader
                .read_exact(&mut hunk_data)
                .map_err(|e| e.to_string())?;
        } else if codec == 0x7a6c6962 {
            use flate2::read::DeflateDecoder;
            let mut decoder = DeflateDecoder::new(Cursor::new(comp_data));
            decoder
                .read_exact(&mut hunk_data)
                .map_err(|e| e.to_string())?;
        } else if codec == 0x666c6163 {
            if comp_data.len() == 15 {
                hunk_data = vec![0u8; self.hunk_bytes as usize];
            } else {
                return Err(format!("FLAC não suportado: {} bytes", comp_data.len()));
            }
        } else {
            return Err(format!("Codec não suportado: 0x{codec:08x}"));
        }

        self.cached_hunk = Some((hunk_idx, hunk_data.clone()));
        Ok(hunk_data)
    }

    pub fn read_sector(&mut self, lba: u64) -> Result<Vec<u8>, String> {
        let sectors_per_hunk = (self.hunk_bytes / self.unit_bytes) as u64;
        let hunk_idx = (lba / sectors_per_hunk) as usize;
        let sub_idx = (lba % sectors_per_hunk) as usize;
        let hunk_data = self.read_hunk(hunk_idx)?;
        let start = sub_idx * self.unit_bytes as usize;
        let end = start + 2048;
        if end > hunk_data.len() {
            return Err("Offset do setor além dos limites do hunk".into());
        }
        Ok(hunk_data[start..end].to_vec())
    }

    pub fn read_bytes(&mut self, lba: u64, len: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(len);
        let mut cur_lba = lba;
        while out.len() < len {
            let sec = self.read_sector(cur_lba)?;
            let needed = len - out.len();
            let take = needed.min(2048);
            out.extend_from_slice(&sec[..take]);
            cur_lba += 1;
        }
        Ok(out)
    }
}

fn chd_find_iso_entry(
    chd: &mut ChdReader,
    dir_lba: u64,
    dir_len: u64,
    target_name: &str,
) -> Result<Option<(u64, u64, u8)>, String> {
    if dir_len == 0 || dir_len > 16 * 1024 * 1024 {
        return Ok(None);
    }
    let buf = chd.read_bytes(dir_lba, dir_len as usize)?;
    let wanted = target_name.trim_end_matches(";1").to_ascii_uppercase();
    let wanted_ver = format!("{wanted};1");
    let mut pos = 0usize;
    while pos < buf.len() {
        let rec_len = buf[pos] as usize;
        if rec_len == 0 {
            pos = ((pos / 2048) + 1) * 2048;
            continue;
        }
        if pos + rec_len > buf.len() || rec_len < 34 {
            break;
        }
        let lba = u32::from_le_bytes([buf[pos + 2], buf[pos + 3], buf[pos + 4], buf[pos + 5]]) as u64;
        let size =
            u32::from_le_bytes([buf[pos + 10], buf[pos + 11], buf[pos + 12], buf[pos + 13]]) as u64;
        let flags = buf[pos + 25];
        let name_len = buf[pos + 32] as usize;
        if pos + 33 + name_len <= pos + rec_len {
            let raw_name =
                String::from_utf8_lossy(&buf[pos + 33..pos + 33 + name_len]).to_ascii_uppercase();
            if raw_name == wanted || raw_name == wanted_ver {
                return Ok(Some((lba, size, flags)));
            }
        }
        pos += rec_len;
    }
    Ok(None)
}

fn chd_resolve_iso_path(
    chd: &mut ChdReader,
    root_lba: u64,
    root_len: u64,
    rel_path: &str,
) -> Result<Option<(u64, u64)>, String> {
    let parts: Vec<&str> = rel_path
        .split(['\\', '/'])
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return Ok(None);
    }
    let mut cur_lba = root_lba;
    let mut cur_len = root_len;
    for (idx, part) in parts.iter().enumerate() {
        let Some((lba, size, flags)) = chd_find_iso_entry(chd, cur_lba, cur_len, part)? else {
            return Ok(None);
        };
        let is_last = idx + 1 == parts.len();
        if is_last {
            return Ok(Some((lba, size)));
        }
        if flags & 0x02 == 0 {
            return Ok(None);
        }
        cur_lba = lba;
        cur_len = size;
    }
    Ok(None)
}

fn parse_boot2_directive(sys_text: &str) -> Option<String> {
    for line in sys_text.lines() {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim().eq_ignore_ascii_case("BOOT2") {
                let rhs = v.trim();
                let after_colon = rhs
                    .split_once(':')
                    .map(|(_, rest)| rest.trim())
                    .unwrap_or(rhs);
                let without_leading = after_colon.trim_start_matches(['\\', '/']);
                let clean = without_leading
                    .split(';')
                    .next()
                    .unwrap_or(without_leading)
                    .trim();
                if !clean.is_empty() {
                    return Some(clean.to_string());
                }
            }
        }
    }
    None
}

pub fn extract_chd_serial(path: &Path) -> Option<String> {
    let mut chd = ChdReader::open(path).ok()?;
    let pvd = chd.read_sector(16).ok()?;
    if &pvd[0..6] != b"\x01CD001" {
        return None;
    }
    let root_lba = u32::from_le_bytes([pvd[158], pvd[159], pvd[160], pvd[161]]) as u64;
    let root_len = u32::from_le_bytes([pvd[166], pvd[167], pvd[168], pvd[169]]) as u64;
    let (sys_lba, sys_len) = chd_resolve_iso_path(&mut chd, root_lba, root_len, "SYSTEM.CNF")
        .ok()?
        .filter(|&(_, len)| len > 0 && len <= 64 * 1024)?;
    let sys_bytes = chd.read_bytes(sys_lba, sys_len as usize).ok()?;
    let sys_text = String::from_utf8_lossy(&sys_bytes);
    let boot = parse_boot2_directive(&sys_text)?;
    let serial = boot
        .split(';')
        .next()
        .unwrap_or(&boot)
        .replace('_', "-")
        .replace('.', "");
    if serial.len() >= 8 {
        Some(serial)
    } else {
        None
    }
}

pub fn compute_chd_ps2_hash(path: &Path) -> Result<(String, String), String> {
    let mut chd = ChdReader::open(path)?;
    let pvd = chd.read_sector(16)?;
    if &pvd[0..6] != b"\x01CD001" {
        return Err("PVD ISO9660 não encontrado".into());
    }
    let root_lba = u32::from_le_bytes([pvd[158], pvd[159], pvd[160], pvd[161]]) as u64;
    let root_len = u32::from_le_bytes([pvd[166], pvd[167], pvd[168], pvd[169]]) as u64;

    let Some((sys_lba, sys_len)) = chd_resolve_iso_path(&mut chd, root_lba, root_len, "SYSTEM.CNF")?
    else {
        return Err("SYSTEM.CNF não encontrado".into());
    };
    let sys_bytes = chd.read_bytes(sys_lba, sys_len as usize)?;
    let sys_text = String::from_utf8_lossy(&sys_bytes);

    let boot_rel = parse_boot2_directive(&sys_text)
        .ok_or_else(|| "BOOT2 não encontrado no SYSTEM.CNF".to_string())?;

    let Some((exe_lba, exe_len)) = chd_resolve_iso_path(&mut chd, root_lba, root_len, &boot_rel)?
    else {
        return Err(format!("Executável BOOT2 '{boot_rel}' não localizado"));
    };

    let exe_buf = chd.read_bytes(exe_lba, exe_len as usize)?;
    if !exe_buf.starts_with(b"\x7fELF") {
        return Err("Arquivo não é um ELF válido".into());
    }

    let mut combined = Vec::with_capacity(boot_rel.len() + exe_len as usize);
    combined.extend_from_slice(boot_rel.as_bytes());
    combined.extend_from_slice(&exe_buf);

    let hash = crate::achievements::compute_md5_hex(&combined);
    Ok((hash, boot_rel))
}
