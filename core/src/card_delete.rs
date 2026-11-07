//! Bounded PS2 FAT deletion; works on a staged image and preserves its geometry and ECC layout.
use super::{CardReader, McResult, read_bounded, u16le, u32le};
use std::{collections::HashSet, fs, path::Path};

// PS2 Hamming parity definition: Ross Ridge, public-domain mymc/ps2mc_ecc.py.
fn ecc(bytes: &[u8]) -> [u8; 3] {
    static TABLE: std::sync::OnceLock<[u8; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        std::array::from_fn(|value| {
            [0x55u8, 0x33, 0x0f, 0, 0xaa, 0xcc, 0xf0]
                .iter()
                .enumerate()
                .fold(0, |mask, (bit, select)| {
                    mask | ((((value as u8 & select).count_ones() & 1) as u8) << bit)
                })
        })
    });
    let mut result = [0x77, 0x7f, 0x7f];
    for (index, &value) in bytes.iter().enumerate() {
        result[0] ^= table[value as usize];
        if value.count_ones() & 1 != 0 {
            result[1] ^= !(index as u8);
            result[2] ^= index as u8;
        }
    }
    result[1] &= 0x7f;
    result
}
fn fat_offset(card: &CardReader, index: usize) -> McResult<usize> {
    let indirect = u32le(&card.raw, 80 + (index / 65536) * 4)? as usize;
    if indirect >= card.alloc {
        return Err("Tabela indireta fora da área de metadados.".into());
    }
    let table = u32le(card.cluster(indirect)?, ((index / 256) % 256) * 4)? as usize;
    if table >= card.alloc {
        return Err("Tabela FAT fora da área de metadados.".into());
    }
    let offset = table * 1024 + (index % 256) * 4;
    u32le(&card.raw, offset)?;
    Ok(offset)
}
fn chain(card: &CardReader, first: usize) -> McResult<Vec<usize>> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut index = first;
    loop {
        if index >= card.count || !seen.insert(index) {
            return Err("Cadeia FAT inválida ou cíclica.".into());
        }
        let next = u32le(&card.raw, fat_offset(card, index)?)?;
        if next & 0x80000000 == 0 {
            return Err("Cluster livre em cadeia ativa.".into());
        }
        result.push(index);
        if next == u32::MAX {
            break;
        }
        index = (next & 0x7fffffff) as usize;
    }
    Ok(result)
}
struct Ownership<'a> {
    card: &'a CardReader,
    seen: HashSet<usize>,
    free: HashSet<usize>,
    target_slot: usize,
}
impl Ownership<'_> {
    fn visit(
        &mut self,
        first: usize,
        length: usize,
        directory: bool,
        selected: bool,
        depth: usize,
    ) -> McResult<()> {
        if depth > 64 {
            return Err("Diretórios aninhados além do limite.".into());
        }
        if length == 0 && !directory {
            return Ok(());
        }
        let clusters = chain(self.card, first)?;
        for &cluster in &clusters {
            if !self.seen.insert(cluster) {
                return Err("Cartão com clusters compartilhados; operação bloqueada.".into());
            }
            if selected {
                self.free.insert(cluster);
            }
        }
        if !directory {
            if length > clusters.len() * 1024 {
                return Err("Arquivo excede a própria cadeia FAT.".into());
            }
            return Ok(());
        }
        if length > clusters.len() * 2 {
            return Err("Diretório truncado.".into());
        }
        for slot in 0..length {
            let offset = (self.card.alloc + clusters[slot / 2]) * 1024 + (slot % 2) * 512;
            let mode = u16le(&self.card.raw, offset)?;
            if mode & 0x8000 == 0 {
                continue;
            }
            let name = &self.card.raw[offset + 64..offset + 96];
            let end = name.iter().position(|b| *b == 0).unwrap_or(32);
            if &name[..end] == b"." || &name[..end] == b".." {
                continue;
            }
            self.visit(
                u32le(&self.card.raw, offset + 16)? as usize,
                u32le(&self.card.raw, offset + 4)? as usize,
                mode & 0x20 != 0,
                selected || (depth == 0 && slot == self.target_slot),
                depth + 1,
            )?;
        }
        Ok(())
    }
}
pub fn delete(path: &Path, save: &str) -> McResult<()> {
    let original = read_bounded(path)?;
    let mut card = CardReader::open(&original)?;
    let root = chain(&card, card.root)?;
    let length = u32le(&card.raw, (card.alloc + root[0]) * 1024 + 4)? as usize;
    if length > root.len() * 2 {
        return Err("Diretório raiz truncado.".into());
    }
    let slot = (0..length)
        .find(|slot| {
            let at = (card.alloc + root[slot / 2]) * 1024 + (slot % 2) * 512;
            let name = &card.raw[at + 64..at + 96];
            let end = name.iter().position(|b| *b == 0).unwrap_or(32);
            u16le(&card.raw, at).is_ok_and(|mode| mode & 0x8020 == 0x8020)
                && &name[..end] == save.as_bytes()
        })
        .ok_or("Save não encontrado.")?;
    let mut ownership = Ownership {
        card: &card,
        seen: HashSet::new(),
        free: HashSet::new(),
        target_slot: slot,
    };
    ownership.visit(card.root, length, true, false, 0)?;
    let freed = ownership.free;
    let free_before = card.free()?;
    let mut changed = HashSet::new();
    for cluster in &freed {
        let at = fat_offset(&card, *cluster)?;
        card.raw[at..at + 4].copy_from_slice(&0x7fffffffu32.to_le_bytes());
        changed.insert(at / 512);
    }
    let at = (card.alloc + root[slot / 2]) * 1024 + (slot % 2) * 512;
    let mode = u16le(&card.raw, at)? & 0x7fff;
    card.raw[at..at + 2].copy_from_slice(&mode.to_le_bytes());
    changed.insert(at / 512);
    let output = if original.len() == card.raw.len() {
        card.raw
    } else {
        let mut output = original.clone();
        for page in changed {
            let raw_at = page * 512;
            let physical = page * 528;
            for chunk in 0..4 {
                let from = physical + chunk * 128;
                let code = ecc(&original[from..from + 128]);
                if original[physical + 512 + chunk * 3..physical + 515 + chunk * 3] != code {
                    return Err("ECC inválido nos metadados. O cartão não foi alterado.".into());
                }
            }
            output[physical..physical + 512].copy_from_slice(&card.raw[raw_at..raw_at + 512]);
            for chunk in 0..4 {
                let code = ecc(&card.raw[raw_at + chunk * 128..raw_at + (chunk + 1) * 128]);
                output[physical + 512 + chunk * 3..physical + 515 + chunk * 3]
                    .copy_from_slice(&code);
            }
        }
        output
    };
    let after = CardReader::open(&output)?;
    if after.saves()?.iter().any(|s| s.name == save)
        || after.free() != Ok(free_before + freed.len() as u64 * 1024)
    {
        return Err("A exclusão não passou na verificação FAT.".into());
    }
    fs::write(path, output).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parity_vectors() {
        assert_eq!(ecc(&[0; 128]), [0x77, 0x7f, 0x7f]);
        let mut data = [0; 128];
        data[0] = 1;
        assert_eq!(ecc(&data), [0x70, 0, 0x7f]);
        data = [0; 128];
        data[127] = 128;
        assert_eq!(ecc(&data), [7, 0x7f, 0]);
    }
}
