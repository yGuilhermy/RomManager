//! Bounded CHD v5 metadata inspection. This does not decompress or certify data.
use serde::Serialize;
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Debug, Serialize)]
pub struct ChdInfo {
    pub version: u32,
    pub logical_bytes: u64,
    pub unit_bytes: u32,
    pub representation: String,
    pub iso_bytes: Option<u64>,
    pub tracks: Vec<String>,
    pub blocked_reason: Option<String>,
}
fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().expect("fixed-size CHD field"))
}
fn be64(bytes: &[u8]) -> u64 {
    u64::from_be_bytes(bytes.try_into().expect("fixed-size CHD field"))
}
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.trim_end_matches('\0')
        .split_whitespace()
        .find_map(|token| {
            token
                .split_once(':')
                .filter(|(k, _)| *k == key)
                .map(|(_, v)| v)
        })
}

pub fn inspect(path: &Path) -> Result<ChdInfo, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let file_size = file.metadata().map_err(|e| e.to_string())?.len();
    let mut header = [0u8; 124];
    file.read_exact(&mut header)
        .map_err(|_| "Cabeçalho CHD truncado.".to_string())?;
    if &header[..8] != b"MComprHD" {
        return Err("Assinatura CHD inválida.".into());
    }
    let version = be32(&header[12..16]);
    if version != 5 || be32(&header[8..12]) != 124 {
        return Err("Inspeção Console suporta somente cabeçalho CHD v5 de 124 bytes.".into());
    }
    let logical_bytes = be64(&header[32..40]);
    let hunk = be32(&header[56..60]);
    let unit_bytes = be32(&header[60..64]);
    if unit_bytes == 0
        || hunk == 0
        || hunk % unit_bytes != 0
        || logical_bytes == 0
        || logical_bytes % u64::from(unit_bytes) != 0
    {
        return Err("Geometria CHD inconsistente.".into());
    }
    let mut next = be64(&header[48..56]);
    let mut visited = HashSet::new();
    let mut total = 0usize;
    let mut dvd = false;
    let mut cd = false;
    let mut tracks = vec![];
    let mut gd = false;
    while next != 0 {
        if visited.len() >= 512 || !visited.insert(next) {
            return Err("Cadeia de metadados CHD cíclica ou excessiva.".into());
        }
        if next < 124 || next.checked_add(16).is_none_or(|n| n > file_size) {
            return Err("Offset de metadados CHD fora do arquivo.".into());
        }
        file.seek(SeekFrom::Start(next))
            .map_err(|e| e.to_string())?;
        let mut item = [0u8; 16];
        file.read_exact(&mut item).map_err(|e| e.to_string())?;
        let length = (be32(&item[4..8]) & 0x00ff_ffff) as usize;
        total += length;
        if length > 1024 * 1024 || total > 4 * 1024 * 1024 || next + 16 + length as u64 > file_size
        {
            return Err("Metadados CHD truncados ou acima do limite.".into());
        }
        match &item[..4] {
            b"DVD " => dvd = true,
            b"CHT2" | b"CHTR" => {
                cd = true;
                let mut bytes = vec![0; length];
                file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
                tracks.push(String::from_utf8(bytes).map_err(|_| {
                    "Metadados de trilha CHD não são ASCII/UTF-8 reconhecível.".to_string()
                })?);
            }
            b"CHCD" => cd = true,
            b"CHGT" | b"CHGD" => gd = true,
            _ => {}
        }
        next = be64(&item[8..16]);
    }
    if dvd && (cd || gd) {
        return Err("Metadados CHD misturam DVD e CD/GD-ROM.".into());
    }
    let (representation, iso_bytes, mut blocked_reason) = if dvd && unit_bytes == 2048 {
        ("dvd", Some(logical_bytes), None)
    } else if cd
        && !gd
        && tracks.len() == 1
        && unit_bytes == 2448
        && field(&tracks[0], "TRACK") == Some("1")
        && field(&tracks[0], "TYPE") == Some("MODE1")
        && field(&tracks[0], "SUBTYPE") == Some("NONE")
        && field(&tracks[0], "PREGAP").is_none_or(|v| v == "0")
        && field(&tracks[0], "POSTGAP").is_none_or(|v| v == "0")
    {
        let frames = field(&tracks[0], "FRAMES")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&n| n > 0)
            .ok_or("Quantidade de setores CD inválida.")?;
        if frames
            .checked_mul(u64::from(unit_bytes))
            .is_none_or(|n| n > logical_bytes)
        {
            return Err("Quantidade de setores CD excede o tamanho lógico do CHD.".into());
        }
        let size = frames
            .checked_mul(2048)
            .filter(|&n| n <= logical_bytes)
            .ok_or("Tamanho de saída CD inconsistente.")?;
        ("cd_mode1",Some(size),Some("Container CD MODE1: requer pipeline extractcd e validação de trilhas; não enviar automaticamente para extractdvd.".to_string()))
    } else if cd {
        (
            "cd_tracks",
            None,
            Some(
                "CD com trilhas ou representação ainda não suportada pelo instalador inicial."
                    .into(),
            ),
        )
    } else {
        (
            "unknown",
            None,
            Some("Representação CHD não comprovada como DVD PS2.".into()),
        )
    };
    if header[104..124].iter().any(|&b| b != 0) {
        blocked_reason = Some("CHD depende de um parent; conversão standalone bloqueada.".into());
    }
    Ok(ChdInfo {
        version,
        logical_bytes,
        unit_bytes,
        representation: representation.into(),
        iso_bytes,
        tracks,
        blocked_reason,
    })
}
