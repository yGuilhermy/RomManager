//! USBExtreme index reader. Offsets follow OPL supportbase.h at
//! b3a2199795fe89ba7e2dd7b843354472dff2c3eb. No device writes occur here.

const RECORD_SIZE: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Media {
    Cd,
    Dvd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    raw: [u8; RECORD_SIZE],
    startup: String,
    media: Media,
}

impl Entry {
    pub fn name_bytes(&self) -> &[u8] {
        let field = &self.raw[..32];
        &field[..field.iter().position(|&b| b == 0).unwrap_or(32)]
    }

    pub fn startup(&self) -> &str {
        &self.startup
    }
    pub fn media(&self) -> Media {
        self.media
    }
    pub fn parts(&self) -> u8 {
        self.raw[47]
    }

    pub fn part_names(&self) -> Result<Vec<String>, String> {
        let checksum = name_checksum(self.name_bytes())?;
        Ok((0..self.parts())
            .map(|part| format!("ul.{checksum:08X}.{}.{part:02x}", self.startup))
            .collect())
    }
}

pub fn parse_index(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    if bytes.len() % RECORD_SIZE != 0 {
        return Err("ul.cfg truncado: cada entrada precisa de 64 bytes.".into());
    }
    bytes
        .chunks_exact(RECORD_SIZE)
        .enumerate()
        .map(|(index, record)| {
            let invalid = |reason: &str| format!("Entrada UL {}: {reason}", index + 1);
            if &record[32..35] != b"ul." {
                return Err(invalid("assinatura inválida."));
            }
            if record[47] == 0 {
                return Err(invalid("nenhuma parte declarada."));
            }
            let media = match record[48] {
                0x12 => Media::Cd,
                0x14 => Media::Dvd,
                _ => return Err(invalid("tipo de mídia não suportado.")),
            };
            let startup_field = &record[35..47];
            let end = startup_field.iter().position(|&b| b == 0).unwrap_or(12);
            let startup = &startup_field[..end];
            // A startup becomes part of a filename; accept disc IDs, never paths.
            if startup.len() != 11
                || !startup[..4].iter().all(u8::is_ascii_uppercase)
                || startup[4] != b'_'
                || !startup[5..8].iter().all(u8::is_ascii_digit)
                || startup[8] != b'.'
                || !startup[9..].iter().all(u8::is_ascii_digit)
            {
                return Err(invalid("identificador de inicialização não reconhecido."));
            }
            if record[0] == 0 {
                return Err(invalid("nome vazio."));
            }
            Ok(Entry {
                raw: record.try_into().expect("chunks_exact garante 64 bytes"),
                startup: String::from_utf8(startup.to_vec()).expect("startup validado como ASCII"),
                media,
            })
        })
        .collect()
}

/// Serializes only parsed entries, retaining unknown/reserved bytes exactly.
/// This is an in-memory round trip, not an installer or filesystem writer.
pub fn serialize_index(entries: &[Entry]) -> Vec<u8> {
    entries.iter().flat_map(|entry| entry.raw).collect()
}

pub fn name_checksum(name: &[u8]) -> Result<u32, String> {
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    let name = &name[..end];
    if name.is_empty() || name.len() > 32 || !name.iter().all(|b| (0x20..=0x7e).contains(b)) {
        return Err("Nome UL sem codificação comprovada: identificação de partes limitada a ASCII imprimível, até 32 bytes.".into());
    }
    // OPL's USBA_crc32 includes the terminating NUL and uses a reversed table
    // index. It is not the usual reflected ZIP CRC32.
    let mut table = [0u32; 256];
    let mut crc = 0;
    for value in 0..256 {
        crc = (value as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x04c1_1db7
            };
        }
        table[255 - value] = crc;
    }
    for byte in name.iter().copied().chain(std::iter::once(0)) {
        crc = table[(u32::from(byte) ^ (crc >> 24)) as usize] ^ (crc << 8);
    }
    Ok(crc)
}
