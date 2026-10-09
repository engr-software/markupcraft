//! Digital signatures: create a digital ID, sign, list and validate.

use markupcraft_engine::signatures::{
    IdentityInfo, SignRequest, certificate_pem, create_digital_id, load_certificates, open_digital_id,
};
use serde_json::{Value, json};

use super::{Tool, page_arg, path_arg, rect_arg, schema, schema_nodoc};
use crate::{bad_args, failed, summary};

fn read(path: &std::path::Path) -> crate::Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| failed(format!("{}: {e}", path.display())))
}

pub static ID_CREATE: Tool = Tool {
    name: "digital_id_create",
    title: "Create a digital ID",
    description: "Create a self-signed digital ID (P-256 key and certificate) as a password-protected PKCS #12 file (.p12) at `out`; `cert_out` also writes its public certificate (PEM) for others to trust.",
    read_only: false,
    destructive: true,
    schema: || {
        schema_nodoc(
            json!({
                "name": { "type": "string", "description": "The signer's name." },
                "organization": { "type": "string" },
                "unit": { "type": "string" },
                "email": { "type": "string" },
                "country": { "type": "string", "description": "Two letters." },
                "years": { "type": "integer", "minimum": 1, "maximum": 50, "description": "Validity (default 5)." },
                "password": { "type": "string", "description": "Protects the file." },
                "out": path_arg("The .p12 file to write"),
                "cert_out": path_arg("The public certificate (.pem) to write")
            }),
            &["name", "password", "out"],
        )
    },
    run: |a, args| {
        let who = IdentityInfo {
            name: args.str("name")?.to_string(),
            organization: args.opt_string("organization")?.unwrap_or_default(),
            unit: args.opt_string("unit")?.unwrap_or_default(),
            email: args.opt_string("email")?.unwrap_or_default(),
            country: args.opt_string("country")?.unwrap_or_default(),
        };
        let years = args.opt_u64("years")?.unwrap_or(5).min(1000) as u32;
        let password = args.str("password")?;
        let out = a.resolve(args.str("out")?, true)?;
        let cert_out = args.opt_str("cert_out")?.map(|p| a.resolve(p, true)).transpose()?;
        let p12 = create_digital_id(&who, years, password)?;
        std::fs::write(&out, &p12).map_err(|e| failed(format!("{}: {e}", out.display())))?;
        let id = open_digital_id(&p12, password)?;
        if let Some(c) = &cert_out {
            std::fs::write(c, certificate_pem(&id)).map_err(|e| failed(format!("{}: {e}", c.display())))?;
        }
        Ok(json!({
            "out": out.display().to_string(),
            "cert_out": cert_out.map(|c| c.display().to_string()),
            "subject": id.certificate.display_name(),
            "fingerprint": id.certificate.fingerprint(),
        }))
    },
};

pub static SIGN: Tool = Tool {
    name: "signature_sign",
    title: "Sign document",
    description: "Sign the document with a digital ID (.p12 + password): an existing unsigned signature field (`field`), or a new one on `page` at `rect` (no rect = invisible). Optional reason, location, contact, `image` (a PNG shown in the signature) and `certify` (1 no changes, 2 form fill and signing, 3 also comments). Writes the signed file to `out` (default the document's own file) and continues on it.",
    read_only: false,
    destructive: true,
    schema: || {
        schema(
            json!({
                "id": path_arg("The digital ID (.p12 / .pfx)"),
                "password": { "type": "string" },
                "field": { "type": "string", "description": "Sign this unsigned signature field." },
                "page": page_arg("for a new signature field (default 1)"),
                "rect": rect_arg("Where a new visible signature goes"),
                "reason": { "type": "string" },
                "location": { "type": "string" },
                "contact": { "type": "string" },
                "certify": { "type": "integer", "minimum": 1, "maximum": 3 },
                "image": path_arg("A PNG shown in the signature"),
                "out": path_arg("The signed file (default: the document's own)")
            }),
            &["id", "password"],
        )
    },
    run: |a, args| {
        let id_path = a.resolve(args.str("id")?, false)?;
        let id = open_digital_id(&read(&id_path)?, args.str("password")?)?;
        let image = args
            .opt_str("image")?
            .map(|p| a.resolve(p, false).and_then(|p| read(&p)))
            .transpose()?;
        let out = args.opt_str("out")?.map(|p| a.resolve(p, true)).transpose()?;
        let req = SignRequest {
            field: args.opt_string("field")?,
            page: args.opt_page("page")?.unwrap_or(0),
            rect: args.opt_rect("rect")?,
            reason: args.opt_string("reason")?,
            location: args.opt_string("location")?,
            contact: args.opt_string("contact")?,
            certify: args.opt_u64("certify")?.map(|c| c.min(255) as u8),
            image,
        };
        if req.image.is_some() && req.rect.is_none() && req.field.is_none() {
            return Err(bad_args("an image signature needs rect (or a field)"));
        }
        let (doc, s) = a.session(args)?;
        let out = out.unwrap_or_else(|| s.path().to_path_buf());
        s.sign(&id, &req, &out)?;
        let sigs = s.signatures(std::slice::from_ref(&id.certificate))?;
        Ok(json!({ "out": out.display().to_string(), "signatures": sigs.len(), "document": summary(doc, s) }))
    },
};

pub static LIST: Tool = Tool {
    name: "signature_list",
    title: "Signatures",
    description: "List and validate the signatures in the document as saved: field, signer, page, date, reason, certification, status (valid; unknown = intact but the signer is not trusted; invalid = changed or broken) and changes made after signing. `trust` lists certificate files (PEM or DER) to trust.",
    read_only: true,
    destructive: false,
    schema: || {
        schema(
            json!({ "trust": { "type": "array", "items": path_arg("A certificate to trust"), "description": "Trusted certificates." } }),
            &[],
        )
    },
    run: |a, args| {
        let mut trust = Vec::new();
        for p in args.opt_strings("trust")?.unwrap_or_default() {
            let path = a.resolve(&p, false)?;
            trust.extend(load_certificates(&read(&path)?)?);
        }
        let (doc, s) = a.session_ref(args)?;
        let list: Vec<Value> = s
            .signatures(&trust)?
            .iter()
            .map(|g| {
                json!({
                    "field": g.field,
                    "signed": g.signed,
                    "signer": g.signer,
                    "page": g.page.map(|p| p + 1),
                    "rect": g.rect.map(|r| r.as_array()),
                    "date": g.date,
                    "reason": g.reason,
                    "location": g.location,
                    "certify": g.certify,
                    "status": g.status,
                    "summary": g.summary,
                    "modifications": g.modifications,
                    "details": g.details,
                })
            })
            .collect();
        Ok(json!({ "doc": doc, "count": list.len(), "signatures": list }))
    },
};
