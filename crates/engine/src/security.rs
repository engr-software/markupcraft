//! Document security (the standard security handler, ISO 32000-1 §7.6.3): open
//! password-protected PDFs, set an open (user) password and/or a permissions (owner) password
//! with permissions, and remove protection. Encryption itself is PdfCraft's (`pdfcraft-cos`,
//! `pdfcraft-crypt`: RC4 and AES-128/256). New security is applied by the next save, which
//! rewrites the whole file; setting or removing it is an undoable step until then.

use std::path::Path;
use std::sync::Arc;

use markupcraft_revu::PdfFile;
use markupcraft_revu::cos::{Algorithm, Auth, CosError, Document as CosDoc, NewEncryption};

use crate::docutil::random_seed;
use crate::{EngineError, Result, Session, invalid};

/// What a password allows (ISO 32000-1 Table 22).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    pub print: bool,
    pub print_high_quality: bool,
    /// Change the document other than the operations below.
    pub modify: bool,
    pub copy: bool,
    /// Add or change markups (and fill forms).
    pub annotate: bool,
    pub fill_forms: bool,
    pub accessibility: bool,
    /// Insert, rotate or delete pages; bookmarks and thumbnails.
    pub assemble: bool,
}

impl Permissions {
    pub const ALL: Permissions = Permissions {
        print: true,
        print_high_quality: true,
        modify: true,
        copy: true,
        annotate: true,
        fill_forms: true,
        accessibility: true,
        assemble: true,
    };

    fn bits(self) -> i32 {
        let mut p = 0i32;
        let mut set = |on: bool, bit: u32| {
            if on {
                p |= 1 << (bit - 1);
            }
        };
        set(self.print, 3);
        set(self.modify, 4);
        set(self.copy, 5);
        set(self.annotate, 6);
        set(self.fill_forms, 9);
        set(self.accessibility, 10);
        set(self.assemble, 11);
        set(self.print_high_quality, 12);
        p
    }

    fn of(p: &markupcraft_revu::cos::Permissions) -> Self {
        Self {
            print: p.print(),
            print_high_quality: p.print_high_quality(),
            modify: p.modify(),
            copy: p.copy(),
            annotate: p.annotate(),
            fill_forms: p.fill_forms(),
            accessibility: p.extract_for_accessibility(),
            assemble: p.assemble(),
        }
    }
}

/// The encryption a new protection uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encryption {
    Rc4_128,
    Aes128,
    #[default]
    Aes256,
}

impl Encryption {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().replace(['-', '_', ' '], "").as_str() {
            "rc4" | "rc4128" => Encryption::Rc4_128,
            "aes128" => Encryption::Aes128,
            "aes256" | "aes" => Encryption::Aes256,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Encryption::Rc4_128 => "RC4-128",
            Encryption::Aes128 => "AES-128",
            Encryption::Aes256 => "AES-256",
        }
    }
}

/// New protection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecuritySettings {
    /// Needed to open the document ("" = opens without one).
    pub open_password: String,
    /// Needed to change security and to lift the restrictions ("" = the open password).
    pub permissions_password: String,
    pub permissions: Permissions,
    pub encryption: Encryption,
}

/// The document's security as opened, and what the next save writes.
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityInfo {
    pub encrypted: bool,
    /// "R2" .. "R6" with the method, when encrypted.
    pub method: Option<String>,
    /// Opened with the owner (permissions) password: every permission.
    pub owner: bool,
    pub permissions: Permissions,
    /// The next save writes new security (or removes it).
    pub pending_change: bool,
    /// The next save writes the file encrypted.
    pub save_encrypted: bool,
}

const MAX_PASSWORD: usize = 127;

fn method_name(h: &markupcraft_revu::cos::SecurityHandler) -> String {
    let d = h.dict();
    let m = match d.r {
        2 | 3 => "RC4",
        4 => {
            if d.crypt_filters
                .iter()
                .any(|(_, m)| *m == markupcraft_revu::cos::CryptMethod::Aes128)
            {
                "AES-128"
            } else {
                "RC4"
            }
        }
        _ => "AES-256",
    };
    format!("R{} {m} {}-bit", d.r, if d.r >= 5 { 256 } else { d.length_bits })
}

impl Session {
    /// Open a password-protected PDF with its open (user) or permissions (owner) password.
    pub fn open_with_password(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let path = path.as_ref();
        let data = markupcraft_revu::fsio::read(path).map_err(|e| EngineError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let cos = CosDoc::open_with_password(Arc::new(data), Some(password)).map_err(|e| match e {
            CosError::WrongPassword | CosError::NeedsPassword => invalid("the password is incorrect"),
            other => EngineError::Pdf(other),
        })?;
        let doc = markupcraft_revu::read::load(&cos, &path.display().to_string());
        Ok(Self::from_parts(
            PdfFile {
                cos,
                path: path.to_path_buf(),
            },
            doc,
        ))
    }

    pub fn security(&self) -> SecurityInfo {
        let cos = &self.file.cos;
        let h = cos.security();
        let out = cos.output_handler();
        SecurityInfo {
            encrypted: h.is_some(),
            method: h.map(method_name),
            owner: h.is_none_or(|h| h.auth() == Auth::Owner),
            permissions: cos.permissions().map_or(Permissions::ALL, |p| Permissions::of(&p)),
            pending_change: cos.encryption_changed(),
            save_encrypted: out.is_some(),
        }
    }

    fn require_owner(&self) -> Result<()> {
        if self.file.cos.security().is_some_and(|h| h.auth() != Auth::Owner) {
            return Err(invalid(
                "the document was opened with its open password; changing security needs the permissions password",
            ));
        }
        Ok(())
    }

    /// Protect the document; the next save writes it encrypted.
    pub fn set_security(&mut self, s: &SecuritySettings) -> Result<()> {
        self.require_owner()?;
        if s.open_password.is_empty() && s.permissions_password.is_empty() {
            return Err(invalid("give an open password, a permissions password, or both"));
        }
        if s.open_password.chars().count() > MAX_PASSWORD || s.permissions_password.chars().count() > MAX_PASSWORD {
            return Err(invalid(format!("passwords are {MAX_PASSWORD} characters at most")));
        }
        let owner = if s.permissions_password.is_empty() {
            &s.open_password
        } else {
            &s.permissions_password
        };
        let params = NewEncryption {
            algorithm: match s.encryption {
                Encryption::Rc4_128 => Algorithm::Rc4_128,
                Encryption::Aes128 => Algorithm::Aes128,
                Encryption::Aes256 => Algorithm::Aes256,
            },
            user_password: &s.open_password,
            owner_password: owner,
            permissions: s.permissions.bits(),
            encrypt_metadata: true,
            seed: random_seed(),
        };
        self.cos_edit("Security", |cos| {
            cos.set_encryption(&params)?;
            cos.require_full_save();
            Ok(((), true))
        })
    }

    /// Remove protection; the next save writes the file unencrypted.
    pub fn remove_security(&mut self) -> Result<()> {
        self.require_owner()?;
        if self.file.cos.security().is_none() && self.file.cos.output_handler().is_none() {
            return Err(invalid("the document is not protected"));
        }
        self.cos_edit("Remove Security", |cos| {
            cos.remove_encryption();
            cos.require_full_save();
            Ok(((), true))
        })
    }
}
