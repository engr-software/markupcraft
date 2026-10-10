//! The Digital ID manager's store: password-protected PKCS #12 IDs kept in a folder, each with
//! its public certificate beside it (`<name>.pem`, so the list shows who an ID names without
//! its password). Create, import, export the public certificate, change the password and
//! delete. Also clearing a certification (the certifier, with their ID, removes it).

use std::path::{Path, PathBuf};

use markupcraft_revu::cos::{ObjRef, Object};

use crate::signatures::{IdentityInfo, certificate_pem, create_digital_id, open_digital_id};
use crate::{Result, Session, invalid};

/// An ID in the store.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredId {
    /// The file name without `.p12`.
    pub name: String,
    pub path: PathBuf,
    /// Who the certificate names.
    pub subject: String,
    /// SHA-256 fingerprint of the certificate.
    pub fingerprint: String,
}

fn io(p: &Path) -> impl Fn(std::io::Error) -> crate::EngineError + '_ {
    move |e| crate::EngineError::Io {
        path: p.display().to_string(),
        source: e,
    }
}

fn clean(name: &str) -> Result<String> {
    let t = name.trim();
    if t.is_empty() || t.chars().count() > 120 || t.chars().any(|c| "/\\:*?\"<>|".contains(c) || c.is_control()) {
        return Err(invalid("an ID name of 1 to 120 characters without / \\ : * ? \" < > |"));
    }
    Ok(t.to_string())
}

fn read_small(p: &Path) -> Result<Vec<u8>> {
    let len = std::fs::metadata(p).map_err(io(p))?.len();
    if len > 4 << 20 {
        return Err(invalid(format!("{} is too large to be a digital ID", p.display())));
    }
    markupcraft_revu::fsio::read(p).map_err(io(p))
}

/// The IDs in `dir`, by name.
pub fn list_ids(dir: &Path) -> Vec<StoredId> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<StoredId> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("p12")))
        .take(500)
        .filter_map(|p| {
            let name = p.file_stem()?.to_string_lossy().into_owned();
            let pem = markupcraft_revu::fsio::read(p.with_extension("pem")).unwrap_or_default();
            let cert = crate::signatures::load_certificates(&pem)
                .ok()
                .and_then(|v| v.into_iter().next());
            Some(StoredId {
                name,
                subject: cert.as_ref().map(|c| c.display_name()).unwrap_or_default(),
                fingerprint: cert.as_ref().map(|c| c.fingerprint()).unwrap_or_default(),
                path: p,
            })
        })
        .collect();
    out.sort_by_key(|i| i.name.to_lowercase());
    out
}

fn store(dir: &Path, name: &str, p12: &[u8], password: &str) -> Result<StoredId> {
    let id = open_digital_id(p12, password)?;
    let name = clean(name)?;
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    let path = dir.join(format!("{name}.p12"));
    if path.exists() {
        return Err(invalid(format!("an ID named {name} is already in the store")));
    }
    crate::write_atomic(&path, p12)?;
    crate::write_atomic(&path.with_extension("pem"), certificate_pem(&id).as_bytes())?;
    Ok(StoredId {
        name,
        subject: id.certificate.display_name(),
        fingerprint: id.certificate.fingerprint(),
        path,
    })
}

/// A new self-signed ID in the store.
pub fn create_id(dir: &Path, name: &str, who: &IdentityInfo, years: u32, password: &str) -> Result<StoredId> {
    let p12 = create_digital_id(who, years, password)?;
    store(dir, name, &p12, password)
}

/// Import a `.p12` / `.pfx` file (its password checks it) into the store under `name`.
pub fn import_id(dir: &Path, file: &Path, name: &str, password: &str) -> Result<StoredId> {
    store(dir, name, &read_small(file)?, password)
}

/// Write the public certificate of ID `name` to `out` (PEM).
pub fn export_certificate(dir: &Path, name: &str, out: &Path) -> Result<()> {
    let pem = read_small(&dir.join(format!("{}.pem", clean(name)?)))?;
    crate::write_atomic(out, &pem)
}

/// Change the password of ID `name`.
pub fn change_password(dir: &Path, name: &str, old: &str, new: &str) -> Result<()> {
    if new.is_empty() {
        return Err(invalid("the new password cannot be empty"));
    }
    let path = dir.join(format!("{}.p12", clean(name)?));
    let id = open_digital_id(&read_small(&path)?, old)?;
    let p12 = pdfcraft_sign::pkcs12::write(&id, new).map_err(|e| invalid(e.to_string()))?;
    crate::write_atomic(&path, &p12)
}

/// Remove ID `name` (its file and certificate) from the store.
pub fn delete_id(dir: &Path, name: &str) -> Result<()> {
    let path = dir.join(format!("{}.p12", clean(name)?));
    if !path.is_file() {
        return Err(invalid(format!("no ID {name}")));
    }
    std::fs::remove_file(&path).map_err(io(&path))?;
    let _ = std::fs::remove_file(path.with_extension("pem"));
    Ok(())
}

/// Open ID `name` of the store with its password.
pub fn open_id(dir: &Path, name: &str, password: &str) -> Result<pdfcraft_sign::DigitalId> {
    open_digital_id(&read_small(&dir.join(format!("{}.p12", clean(name)?)))?, password)
}

impl Session {
    /// Clear the document's certification: only the certifier can, with the ID that certified
    /// it (the signer named on the certification is checked against it). The certifying
    /// signature is removed (its field stays, unsigned) and so is the DocMDP permission, so
    /// later changes are no longer judged against it. Returns the field cleared. Undoable.
    pub fn clear_certification(&mut self, id: &pdfcraft_sign::DigitalId) -> Result<String> {
        let sigs = self.signatures(&[])?;
        let cert = sigs
            .iter()
            .find(|s| s.certify.is_some() && s.signed)
            .cloned()
            .ok_or_else(|| invalid("the document is not certified"))?;
        let me = id.certificate.display_name();
        if cert.signer.as_deref().is_some_and(|s| s != me) {
            return Err(invalid(format!(
                "only the certifier ({}) can clear the certification",
                cert.signer.unwrap_or_default()
            )));
        }
        let field = cert.field.clone();
        self.cos_edit("Clear Certification", |cos| {
            let Some(root) = cos.root() else {
                return Err(invalid("the document has no catalog"));
            };
            cos.update_dict(root, |d| {
                d.remove(b"Perms");
            })?;
            let acro = cos
                .dict(&Object::Ref(root))
                .and_then(|d| d.get(b"AcroForm").cloned())
                .and_then(|a| cos.dict(&a));
            let mut stack: Vec<Object> = acro
                .and_then(|a| a.get(b"Fields").cloned())
                .map(|f| cos.resolve(&f).as_array().cloned().unwrap_or_default())
                .unwrap_or_default();
            let mut seen = 0;
            while let Some(o) = stack.pop() {
                seen += 1;
                if seen > 100_000 {
                    break;
                }
                let Object::Ref(r) = o else { continue };
                let Some(d) = cos.dict(&Object::Ref(r)) else { continue };
                if let Some(k) = d.get(b"Kids") {
                    stack.extend(cos.resolve(k).as_array().cloned().unwrap_or_default());
                }
                let name = d
                    .get(b"T")
                    .and_then(|t| cos.resolve(t).as_string().map(|s| s.to_text()))
                    .unwrap_or_default();
                if name == field {
                    let fr: ObjRef = r;
                    cos.update_dict(fr, |fd| {
                        fd.remove(b"V");
                        fd.remove(b"Lock");
                    })?;
                }
            }
            cos.require_full_save();
            Ok(((), true))
        })?;
        Ok(field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_create_import_export_password_and_delete() {
        let dir = std::env::temp_dir().join(format!("mc-ids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let who = IdentityInfo {
            name: "Test Signer".into(),
            ..Default::default()
        };
        let made = create_id(&dir, "Work", &who, 2, "pw1").unwrap();
        assert_eq!(made.subject, "Test Signer");
        assert_eq!(list_ids(&dir).len(), 1);
        assert!(create_id(&dir, "Work", &who, 2, "pw1").is_err(), "names are unique");
        change_password(&dir, "Work", "pw1", "pw2").unwrap();
        assert!(open_id(&dir, "Work", "pw1").is_err());
        assert!(open_id(&dir, "Work", "pw2").is_ok());
        let out = dir.join("work.pem");
        export_certificate(&dir, "Work", &out).unwrap();
        assert!(std::fs::read_to_string(&out).unwrap().contains("BEGIN CERTIFICATE"));
        // import the same file under another name
        let other = dir.with_extension("copy.p12");
        std::fs::copy(&made.path, &other).unwrap();
        assert!(import_id(&dir.join("store2"), &other, "Copy", "wrong").is_err());
        assert_eq!(
            import_id(&dir.join("store2"), &other, "Copy", "pw2").unwrap().subject,
            "Test Signer"
        );
        delete_id(&dir, "Work").unwrap();
        assert!(list_ids(&dir).is_empty());
        assert!(delete_id(&dir, "a/b").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
