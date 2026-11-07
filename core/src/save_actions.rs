use crate::{memory_cards as mc, save_formats};
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
};
type R<T> = Result<T, String>;
fn e(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn remove(path: &Path) -> R<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
    .map_err(e)
}
fn revision(path: &Path, expected: &str) -> R<()> {
    if expected.is_empty() || mc::fingerprint(path)? != expected {
        Err("O cartão mudou. Atualize antes de continuar.".into())
    } else {
        Ok(())
    }
}

fn exported(db: &Path, card: &Path, save: &str, work: &Path) -> R<Vec<u8>> {
    fs::create_dir_all(work).map_err(e)?;
    if card.is_dir() {
        save_formats::folder_to_psu(&card.join(save))
    } else {
        mc::run_tool(db, card, "export", &[save.into()], work)?;
        mc::read_bounded(&work.join(format!("{save}.psu")))
    }
}
fn payload_digest(folder: &Path) -> R<String> {
    let mut parts = Vec::new();
    for item in walkdir::WalkDir::new(folder)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !e.file_name().to_string_lossy().starts_with("_pcsx2_"))
    {
        let item = item.map_err(e)?;
        let m = item.metadata().map_err(e)?;
        mc::plain_meta(&m)?;
        if m.is_file() {
            parts.push(format!(
                "{}:{}",
                item.path()
                    .strip_prefix(folder)
                    .map_err(e)?
                    .to_string_lossy(),
                mc::digest(&mc::read_bounded(item.path())?)
            ));
        }
    }
    parts.sort();
    Ok(mc::digest(parts.join("\n").as_bytes()))
}
fn saved_payload(db: &Path, card: &Path, save: &str, work: &Path) -> R<String> {
    if card.is_dir() {
        return payload_digest(&card.join(save));
    }
    let bytes = exported(db, card, save, work)?;
    let unpacked = work.join("unpacked");
    fs::create_dir(&unpacked).map_err(e)?;
    save_formats::psu_to_folder(&bytes, &unpacked)?;
    payload_digest(&unpacked.join(save))
}
fn delete_staged(_db: &Path, stage: &Path, save: &str, _work: &Path) -> R<()> {
    if stage.is_dir() {
        let selected = stage.join(save);
        mc::plain(&selected)?;
        remove(&selected)?;
    } else {
        mc::delete_save_file(stage, save)?;
    }
    if mc::inspect(stage)?.0.iter().any(|s| s.name == save) {
        return Err("O save ainda está presente na cópia de trabalho.".into());
    }
    Ok(())
}
fn same_saves(original: &[mc::SaveInfo], edited: &[mc::SaveInfo], excluded: &str) -> bool {
    original.iter().filter(|s| s.name != excluded).all(|s| {
        edited
            .iter()
            .any(|v| v.name == s.name && v.size == s.size && v.files == s.files)
    }) && edited.iter().filter(|s| s.name != excluded).count()
        == original.iter().filter(|s| s.name != excluded).count()
}
fn backup_folder_save(card: &Path, save: &str, dest_card: &Path, require_save: bool) -> R<()> {
    mc::plain(card)?;
    fs::create_dir_all(dest_card).map_err(e)?;
    for item in fs::read_dir(card).map_err(e)? {
        let item = item.map_err(e)?;
        let m = item.metadata().map_err(e)?;
        mc::plain_meta(&m)?;
        let name = item.file_name().to_string_lossy().into_owned();
        if m.is_file() && name.starts_with("_pcsx2_") {
            mc::copy_tree(&item.path(), &dest_card.join(&name))?;
        }
    }
    let save_src = card.join(save);
    if save_src.exists() {
        mc::plain(&save_src)?;
        let save_dst = dest_card.join(save);
        mc::copy_tree(&save_src, &save_dst)?;
        if payload_digest(&save_src)? != payload_digest(&save_dst)? {
            return Err("Backup do save divergente.".into());
        }
    } else if require_save {
        return Err("Save não encontrado para backup.".into());
    }
    Ok(())
}
enum SwapKind {
    Card,
    FolderPut { had_original: bool },
    FolderDelete,
}
struct Swap {
    card: PathBuf,
    sibling: PathBuf,
    old: PathBuf,
    expected: String,
    published: bool,
    kind: SwapKind,
}
impl Swap {
    fn prepare(card: &Path, stage: &Path) -> R<Self> {
        if card.is_file() && fs::metadata(card).map_err(e)?.permissions().readonly() {
            return Err("Cartão protegido contra escrita.".into());
        }
        let sibling = card.with_file_name(format!(".rommanager-save-new-{}", mc::stamp()));
        let expected = mc::fingerprint(stage)?;
        if let Err(error) = mc::copy_tree(stage, &sibling) {
            if sibling.exists() {
                let _ = remove(&sibling);
            }
            return Err(error);
        }
        if mc::fingerprint(&sibling)? != expected {
            let _ = remove(&sibling);
            return Err("Cópia de publicação divergente.".into());
        }
        Ok(Self {
            card: card.into(),
            sibling,
            old: card.with_file_name(format!(".rommanager-save-old-{}", mc::stamp())),
            expected,
            published: false,
            kind: SwapKind::Card,
        })
    }
    fn prepare_folder_put(card: &Path, save: &str, stage_save: &Path) -> R<Self> {
        mc::plain(card)?;
        let target_save = card.join(save);
        let had_original = target_save.exists();
        if had_original {
            mc::plain(&target_save)?;
        }
        let sibling = card.join(format!(".rommanager-save-new-{}", mc::stamp()));
        let expected = payload_digest(stage_save)?;
        if let Err(error) = mc::copy_tree(stage_save, &sibling) {
            if sibling.exists() {
                let _ = remove(&sibling);
            }
            return Err(error);
        }
        if payload_digest(&sibling)? != expected {
            let _ = remove(&sibling);
            return Err("Cópia de publicação divergente.".into());
        }
        Ok(Self {
            card: target_save,
            sibling,
            old: card.join(format!(".rommanager-save-old-{}", mc::stamp())),
            expected,
            published: false,
            kind: SwapKind::FolderPut { had_original },
        })
    }
    fn prepare_folder_delete(card: &Path, save: &str) -> R<Self> {
        mc::plain(card)?;
        let target_save = card.join(save);
        mc::plain(&target_save)?;
        Ok(Self {
            card: target_save,
            sibling: PathBuf::new(),
            old: card.join(format!(".rommanager-save-old-{}", mc::stamp())),
            expected: String::new(),
            published: false,
            kind: SwapKind::FolderDelete,
        })
    }
    fn publish(&mut self) -> R<()> {
        match self.kind {
            SwapKind::Card => {
                fs::rename(&self.card, &self.old).map_err(e)?;
                if let Err(error) = fs::rename(&self.sibling, &self.card) {
                    fs::rename(&self.old, &self.card).map_err(|rollback| {
                        format!(
                            "Falha ao publicar: {error}; original preservado em {}: {rollback}",
                            self.old.display()
                        )
                    })?;
                    return Err(e(error));
                }
            }
            SwapKind::FolderPut { had_original } => {
                if had_original {
                    fs::rename(&self.card, &self.old).map_err(e)?;
                }
                if let Err(error) = fs::rename(&self.sibling, &self.card) {
                    if had_original {
                        fs::rename(&self.old, &self.card).map_err(|rollback| {
                            format!(
                                "Falha ao publicar: {error}; original preservado em {}: {rollback}",
                                self.old.display()
                            )
                        })?;
                    }
                    return Err(e(error));
                }
            }
            SwapKind::FolderDelete => {
                fs::rename(&self.card, &self.old).map_err(e)?;
            }
        }
        self.published = true;
        Ok(())
    }
    fn rollback(&mut self) -> R<()> {
        if !self.published {
            return Ok(());
        }
        match self.kind {
            SwapKind::Card => {
                if mc::fingerprint(&self.card)? != self.expected {
                    return Err(
                        "O destino foi alterado externamente; o backup foi preservado para recuperação."
                            .into(),
                    );
                }
                fs::rename(&self.card, &self.sibling).map_err(e)?;
                if let Err(error) = fs::rename(&self.old, &self.card) {
                    let _ = fs::rename(&self.sibling, &self.card);
                    return Err(e(error));
                }
            }
            SwapKind::FolderPut { had_original } => {
                if payload_digest(&self.card)? != self.expected {
                    return Err(
                        "O destino foi alterado externamente; o backup foi preservado para recuperação."
                            .into(),
                    );
                }
                fs::rename(&self.card, &self.sibling).map_err(e)?;
                if had_original {
                    if let Err(error) = fs::rename(&self.old, &self.card) {
                        let _ = fs::rename(&self.sibling, &self.card);
                        return Err(e(error));
                    }
                }
            }
            SwapKind::FolderDelete => {
                fs::rename(&self.old, &self.card).map_err(e)?;
            }
        }
        self.published = false;
        Ok(())
    }
}
impl Drop for Swap {
    fn drop(&mut self) {
        if !self.sibling.as_os_str().is_empty() && self.sibling.exists() {
            let _ = remove(&self.sibling);
        }
        if self.published && self.old.exists() {
            let _ = remove(&self.old);
        }
    }
}
fn publish(swaps: &mut [Swap]) -> R<()> {
    for index in 0..swaps.len() {
        if let Err(error) = swaps[index].publish() {
            let mut failures = Vec::new();
            for previous in swaps[..index].iter_mut().rev() {
                if let Err(e) = previous.rollback() {
                    failures.push(e);
                }
            }
            return Err(format!(
                "Operação não concluída: {error}. {}",
                failures.join("; ")
            ));
        }
    }
    Ok(())
}
pub fn edit(
    conn: &Connection,
    db: &Path,
    p: &mc::CardAction,
    card: &Path,
    temp: &Path,
) -> R<mc::ActionResult> {
    mc::name(&p.save)?;
    let original = mc::inspect(card)?.0;
    let selected = original
        .iter()
        .find(|s| s.name == p.save)
        .ok_or("Save não encontrado neste cartão.")?;
    let move_save = p.action == "move-save";
    let mut destination = None;
    let mut original_destination = Vec::new();
    if move_save {
        let path = mc::card_path(conn, &p.target)?;
        if path
            .canonicalize()
            .map_err(e)?
            .to_string_lossy()
            .eq_ignore_ascii_case(&card.canonicalize().map_err(e)?.to_string_lossy())
        {
            return Err("Escolha outro memory card como destino.".into());
        }
        revision(&path, &p.target_revision)?;
        original_destination = mc::inspect(&path)?.0;
        if original_destination.iter().any(|s| s.name == p.save) && !p.overwrite {
            return Err(
                "Já existe este save no destino. Confirme a substituição para continuar.".into(),
            );
        }
        destination = Some(path);
    }
    let source_stage = if card.is_dir() {
        mc::plain(&card.join(&p.save))?;
        None
    } else {
        let stage = temp.join("source.ps2");
        mc::copy_tree(card, &stage)?;
        delete_staged(db, &stage, &p.save, temp)?;
        if !same_saves(&original, &mc::inspect(&stage)?.0, &p.save) {
            return Err("A cópia de trabalho alterou outros saves.".into());
        }
        Some(stage)
    };
    let mut destination_stage = None;
    if let Some(target) = &destination {
        let stage = temp.join(if target.is_dir() {
            "destination-folder"
        } else {
            "destination.ps2"
        });
        if target.is_dir() {
            fs::create_dir_all(&stage).map_err(e)?;
            let sb = target.join("_pcsx2_superblock");
            if sb.exists() {
                mc::copy_tree(&sb, &stage.join("_pcsx2_superblock"))?;
            }
        } else {
            mc::copy_tree(target, &stage)?;
            if original_destination.iter().any(|s| s.name == p.save) {
                delete_staged(db, &stage, &p.save, temp)?;
            }
        }
        if card.is_dir() && stage.is_dir() {
            mc::copy_tree(&card.join(&p.save), &stage.join(&p.save))?;
        } else {
            let bytes = exported(db, card, &p.save, &temp.join("export"))?;
            if stage.is_dir() {
                save_formats::psu_to_folder(&bytes, &stage)?;
            } else {
                if mc::inspect(&stage)?
                    .1
                    .is_some_and(|free| free < selected.size)
                {
                    return Err("Espaço insuficiente no cartão de destino.".into());
                }
                let psu = temp.join("incoming.psu");
                fs::write(&psu, bytes).map_err(e)?;
                mc::run_tool(db, &stage, "import", &[psu.to_string_lossy().into()], temp).map_err(
                    |error| {
                        format!(
                            "Não foi possível importar no destino; verifique o espaço livre. {error}"
                        )
                    },
                )?;
            }
        }
        let edited = mc::inspect(&stage)?.0;
        let dest_ok = if target.is_dir() {
            edited
                .iter()
                .any(|s| s.name == p.save && s.size == selected.size && s.files == selected.files)
        } else {
            edited
                .iter()
                .any(|s| s.name == p.save && s.size == selected.size && s.files == selected.files)
                && same_saves(&original_destination, &edited, &p.save)
        };
        if !dest_ok {
            return Err("A verificação do cartão de destino falhou.".into());
        }
        if saved_payload(db, card, &p.save, &temp.join("verify-source"))?
            != saved_payload(db, &stage, &p.save, &temp.join("verify-destination"))?
        {
            return Err("O conteúdo transferido não corresponde ao save de origem.".into());
        }
        destination_stage = Some(stage);
    }
    revision(card, &p.revision)?;
    let backup = mc::store(db).join("backups").join(mc::stamp());
    fs::create_dir_all(backup.join("origem")).map_err(e)?;
    let source_backup = backup.join("origem").join(&p.card);
    if card.is_dir() {
        backup_folder_save(card, &p.save, &source_backup, true)?;
    } else {
        mc::copy_tree(card, &source_backup)?;
        if mc::fingerprint(&source_backup)? != p.revision {
            return Err("Backup da origem divergente.".into());
        }
    }
    if let Some(target) = &destination {
        revision(target, &p.target_revision)?;
        fs::create_dir(backup.join("destino")).map_err(e)?;
        let dest_backup = backup.join("destino").join(&p.target);
        if target.is_dir() {
            backup_folder_save(target, &p.save, &dest_backup, false)?;
        } else {
            mc::copy_tree(target, &dest_backup)?;
            if mc::fingerprint(&dest_backup)? != p.target_revision {
                return Err("Backup do destino divergente.".into());
            }
        }
    }
    let manifest = backup.join("operation.json");
    fs::write(&manifest,serde_json::to_vec_pretty(&serde_json::json!({"action":p.action,"save":p.save,"source":card,"destination":destination,"source_revision":p.revision,"destination_revision":p.target_revision,"state":"prepared"})).map_err(e)?).map_err(e)?;
    let mut swaps = Vec::new();
    if let (Some(target), Some(stage)) = (&destination, &destination_stage) {
        if target.is_dir() {
            swaps.push(Swap::prepare_folder_put(
                target,
                &p.save,
                &stage.join(&p.save),
            )?);
        } else {
            swaps.push(Swap::prepare(target, stage)?);
        }
    }
    if let Some(stage) = &source_stage {
        swaps.push(Swap::prepare(card, stage)?);
    } else {
        swaps.push(Swap::prepare_folder_delete(card, &p.save)?);
    }
    revision(card, &p.revision)?;
    if let Some(target) = &destination {
        revision(target, &p.target_revision)?;
    }
    if crate::pcsx2::installation_id(conn)? != p.installation_id {
        return Err("A instalação mudou. Atualize os cartões.".into());
    }
    mc::writable()?;
    publish(&mut swaps)?;
    let _=fs::write(&manifest,serde_json::to_vec_pretty(&serde_json::json!({"action":p.action,"save":p.save,"source":card,"destination":destination,"source_revision":p.revision,"destination_revision":p.target_revision,"state":"committed"})).map_err(e)?);
    Ok(mc::ActionResult {
        message: if move_save {
            "Save movido; conteúdo e backups verificados."
        } else {
            "Save excluído; backup do cartão preservado."
        }
        .into(),
        path: destination
            .as_deref()
            .unwrap_or(card)
            .to_string_lossy()
            .into(),
        backup: Some(backup.to_string_lossy().into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_failure_restores_both_original_cards() {
        let root = std::env::temp_dir().join(format!("rm-swap-{}", mc::stamp()));
        fs::create_dir_all(&root).unwrap();
        for (name, bytes) in [
            ("source", b"source".as_slice()),
            ("destination", b"destination"),
            ("new-source", b"changed source"),
            ("new-destination", b"changed destination"),
        ] {
            fs::write(root.join(name), bytes).unwrap();
        }
        let mut swaps = vec![
            Swap::prepare(&root.join("destination"), &root.join("new-destination")).unwrap(),
            Swap::prepare(&root.join("source"), &root.join("new-source")).unwrap(),
        ];
        fs::remove_file(&swaps[1].sibling).unwrap();
        assert!(publish(&mut swaps).is_err());
        assert_eq!(fs::read(root.join("source")).unwrap(), b"source");
        assert_eq!(fs::read(root.join("destination")).unwrap(), b"destination");
        drop(swaps);
        fs::remove_dir_all(root).unwrap();
    }
}
