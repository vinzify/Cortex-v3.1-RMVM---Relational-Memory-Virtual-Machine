use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use argon2::Argon2;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const FORMAT_VERSION: &str = "brain/v1";
const RMVM_PROTO_VERSION: &str = "cortex_rmvm_v3_1";
const DEFAULT_SECRET_ENV: &str = "CORTEX_BRAIN_SECRET";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainManifest {
    pub format_version: String,
    pub brain_id: String,
    pub name: String,
    pub tenant_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub rmvm_proto_version: String,
    pub schema_migrations: Vec<String>,
    pub active_branch: String,
    pub kdf_salt_b64: String,
    pub signing_public_key_b64: String,
    pub state_sha256: String,
    pub secret_env_var: String,
    pub signature_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainSummary {
    pub brain_id: String,
    pub name: String,
    pub tenant_id: String,
    pub updated_at: String,
    pub active_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrainState {
    pub branches: BTreeMap<String, BranchState>,
    pub attachments: Vec<AttachmentGrant>,
    pub audit: Vec<AuditEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BranchState {
    pub name: String,
    pub memory_objects: BTreeMap<String, MemoryObject>,
    pub rules: Vec<RuleEntry>,
    pub ledger: Vec<LedgerEvent>,
    pub suppressions: Vec<SuppressionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryObject {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub value: serde_json::Value,
    pub memory_type: String,
    pub suppressed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEntry {
    pub id: String,
    pub description: String,
    pub allowed_sinks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEvent {
    pub id: String,
    pub ts: String,
    pub operation: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuppressionRecord {
    pub id: String,
    pub ts: String,
    pub subject: String,
    pub predicate: String,
    pub scope: String,
    pub reason: String,
    pub suppressed_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentGrant {
    pub agent_id: String,
    pub model_id: String,
    pub read_classes: Vec<String>,
    pub write_classes: Vec<String>,
    pub sinks: Vec<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub ts: String,
    pub actor: String,
    pub action: String,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CreateBrainRequest {
    pub name: String,
    pub tenant_id: String,
    pub passphrase_env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MergeStrategy {
    Ours,
    Theirs,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeReport {
    pub merged: usize,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrainPackage {
    package_version: String,
    manifest: BrainManifest,
    state: EncryptedBlob,
    signing_key: EncryptedBlob,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptedBlob {
    nonce_b64: String,
    ciphertext_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppConfig {
    active_brain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyMapping {
    pub key_hash: String,
    pub tenant_id: String,
    pub brain_id: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ApiKeyMappings {
    mappings: Vec<ApiKeyMapping>,
}

#[derive(Debug, Clone)]
pub struct BrainStore {
    home_dir: PathBuf,
}

impl BrainStore {
    pub fn new(home_override: Option<PathBuf>) -> Result<Self> {
        let home_dir = if let Some(path) = home_override {
            path
        } else if let Ok(v) = env::var("CORTEX_HOME") {
            PathBuf::from(v)
        } else {
            dirs::home_dir()
                .ok_or_else(|| anyhow!("cannot resolve home dir"))?
                .join(".cortex")
        };

        fs::create_dir_all(home_dir.join("brains"))?;
        fs::create_dir_all(home_dir.join("auth"))?;

        Ok(Self { home_dir })
    }

    pub fn home_dir(&self) -> &Path {
        &self.home_dir
    }

    pub fn create_brain(&self, req: CreateBrainRequest) -> Result<BrainSummary> {
        let secret_env = req
            .passphrase_env
            .unwrap_or_else(|| DEFAULT_SECRET_ENV.to_string());
        let secret = env::var(&secret_env).with_context(|| {
            format!("missing passphrase env var {secret_env}; set it before creating brain")
        })?;

        let slug = slugify(&req.name);
        let brain_id = format!("{}-{}", slug, &Uuid::new_v4().to_string()[..8]);
        let brain_dir = self.brains_dir().join(&brain_id);
        fs::create_dir_all(brain_dir.join("keys"))?;

        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let key = derive_key(secret.as_bytes(), &salt)?;

        let signing_key = SigningKey::generate(&mut OsRng);
        let signing_key_bytes = signing_key.to_bytes();
        let signing_key_enc = encrypt_bytes(&key, brain_id.as_bytes(), &signing_key_bytes)?;

        let now = Utc::now().to_rfc3339();
        let mut state = BrainState::default();
        state.branches.insert(
            "main".to_string(),
            BranchState {
                name: "main".to_string(),
                ..BranchState::default()
            },
        );
        state.audit.push(audit_entry(
            "system",
            "brain.create",
            serde_json::json!({"brain_id": brain_id, "tenant_id": req.tenant_id}),
        ));

        let state_enc = encrypt_json(&key, brain_id.as_bytes(), &state)?;
        let mut manifest = BrainManifest {
            format_version: FORMAT_VERSION.to_string(),
            brain_id: brain_id.clone(),
            name: req.name,
            tenant_id: req.tenant_id,
            created_at: now.clone(),
            updated_at: now,
            rmvm_proto_version: RMVM_PROTO_VERSION.to_string(),
            schema_migrations: vec!["brain/v1:init".to_string()],
            active_branch: "main".to_string(),
            kdf_salt_b64: B64.encode(salt),
            signing_public_key_b64: B64.encode(signing_key.verifying_key().to_bytes()),
            state_sha256: sha256_hex(&serde_json::to_vec(&state_enc)?),
            secret_env_var: secret_env,
            signature_b64: String::new(),
        };
        manifest.signature_b64 = sign_manifest(&manifest, &signing_key)?;

        write_json(brain_dir.join("brain.json"), &manifest)?;
        write_json(brain_dir.join("state.enc"), &state_enc)?;
        write_json(
            brain_dir.join("keys").join("signing_key.enc"),
            &signing_key_enc,
        )?;

        Ok(BrainSummary {
            brain_id: manifest.brain_id,
            name: manifest.name,
            tenant_id: manifest.tenant_id,
            updated_at: manifest.updated_at,
            active_branch: manifest.active_branch,
        })
    }

    pub fn list_brains(&self) -> Result<Vec<BrainSummary>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(self.brains_dir())? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let manifest_path = entry.path().join("brain.json");
            if !manifest_path.exists() {
                continue;
            }
            let manifest: BrainManifest = read_json(&manifest_path)?;
            out.push(BrainSummary {
                brain_id: manifest.brain_id,
                name: manifest.name,
                tenant_id: manifest.tenant_id,
                updated_at: manifest.updated_at,
                active_branch: manifest.active_branch,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    pub fn set_active_brain(&self, brain_ref: &str) -> Result<BrainSummary> {
        let summary = self.resolve_brain(brain_ref)?;
        let mut cfg = self.read_config()?;
        cfg.active_brain = Some(summary.brain_id.clone());
        write_json(self.config_path(), &cfg)?;
        Ok(summary)
    }

    pub fn active_brain_id(&self) -> Result<Option<String>> {
        Ok(self.read_config()?.active_brain)
    }

    pub fn export_brain(&self, brain_ref: &str, out_file: &Path) -> Result<()> {
        let summary = self.resolve_brain(brain_ref)?;
        let dir = self.brains_dir().join(&summary.brain_id);
        let manifest: BrainManifest = read_json(dir.join("brain.json"))?;
        let state: EncryptedBlob = read_json(dir.join("state.enc"))?;
        let signing_key: EncryptedBlob = read_json(dir.join("keys").join("signing_key.enc"))?;

        verify_manifest_signature(&manifest)?;

        let package = BrainPackage {
            package_version: FORMAT_VERSION.to_string(),
            manifest,
            state,
            signing_key,
        };
        write_json(out_file, &package)
    }

    pub fn import_brain(
        &self,
        in_file: &Path,
        name_override: Option<String>,
        verify_only: bool,
    ) -> Result<Option<BrainSummary>> {
        let package: BrainPackage = read_json(in_file)?;
        verify_manifest_signature(&package.manifest)?;
        let computed_state_hash = sha256_hex(&serde_json::to_vec(&package.state)?);
        if computed_state_hash != package.manifest.state_sha256 {
            bail!("state checksum mismatch on import package");
        }
        if verify_only {
            return Ok(None);
        }

        let mut manifest = package.manifest;
        if let Some(name) = name_override {
            manifest.name = name;
            manifest.updated_at = Utc::now().to_rfc3339();
        }

        let mut brain_id = manifest.brain_id.clone();
        let mut target = self.brains_dir().join(&brain_id);
        if target.exists() {
            brain_id = format!("{}-{}", brain_id, &Uuid::new_v4().to_string()[..6]);
            target = self.brains_dir().join(&brain_id);
        }
        fs::create_dir_all(target.join("keys"))?;
        manifest.brain_id = brain_id;

        write_json(target.join("brain.json"), &manifest)?;
        write_json(target.join("state.enc"), &package.state)?;
        write_json(
            target.join("keys").join("signing_key.enc"),
            &package.signing_key,
        )?;

        Ok(Some(BrainSummary {
            brain_id: manifest.brain_id,
            name: manifest.name,
            tenant_id: manifest.tenant_id,
            updated_at: manifest.updated_at,
            active_branch: manifest.active_branch,
        }))
    }

    pub fn branch(&self, brain_ref: &str, new_branch: &str) -> Result<()> {
        self.mutate_brain(brain_ref, |manifest, state| {
            if state.branches.contains_key(new_branch) {
                bail!("branch already exists: {new_branch}");
            }
            let source = state
                .branches
                .get(&manifest.active_branch)
                .cloned()
                .ok_or_else(|| anyhow!("active branch missing"))?;
            let mut cloned = source;
            cloned.name = new_branch.to_string();
            state.branches.insert(new_branch.to_string(), cloned);
            state.audit.push(audit_entry(
                "user",
                "brain.branch",
                serde_json::json!({"from": manifest.active_branch, "to": new_branch}),
            ));
            Ok(())
        })
    }

    pub fn merge(
        &self,
        brain_ref: &str,
        source: &str,
        target: &str,
        strategy: MergeStrategy,
    ) -> Result<MergeReport> {
        let mut report = MergeReport {
            merged: 0,
            conflicts: Vec::new(),
        };
        self.mutate_brain(brain_ref, |_, state| {
            let source_branch = state
                .branches
                .get(source)
                .cloned()
                .ok_or_else(|| anyhow!("unknown source branch {source}"))?;
            let target_branch = state
                .branches
                .get_mut(target)
                .ok_or_else(|| anyhow!("unknown target branch {target}"))?;

            for (id, src_obj) in source_branch.memory_objects {
                match target_branch.memory_objects.get(&id) {
                    None => {
                        target_branch.memory_objects.insert(id, src_obj);
                        report.merged += 1;
                    }
                    Some(dst_obj) => {
                        if dst_obj.value == src_obj.value
                            && dst_obj.suppressed == src_obj.suppressed
                        {
                            continue;
                        }
                        match strategy {
                            MergeStrategy::Ours => {}
                            MergeStrategy::Theirs => {
                                target_branch.memory_objects.insert(id, src_obj);
                                report.merged += 1;
                            }
                            MergeStrategy::Manual => {
                                report.conflicts.push(id);
                            }
                        }
                    }
                }
            }
            if !report.conflicts.is_empty() {
                bail!("merge conflicts: {}", report.conflicts.join(","));
            }
            state.audit.push(audit_entry(
                "user",
                "brain.merge",
                serde_json::json!({"source": source, "target": target, "merged": report.merged}),
            ));
            Ok(())
        })?;
        Ok(report)
    }

    pub fn forget_suppress(
        &self,
        brain_ref: &str,
        subject: &str,
        predicate: &str,
        scope: &str,
        reason: &str,
    ) -> Result<usize> {
        let mut suppressed = 0usize;
        self.mutate_brain(brain_ref, |manifest, state| {
            let branch = state
                .branches
                .get_mut(&manifest.active_branch)
                .ok_or_else(|| anyhow!("active branch missing"))?;
            for obj in branch.memory_objects.values_mut() {
                if obj.subject == subject && obj.predicate == predicate && !obj.suppressed {
                    obj.suppressed = true;
                    suppressed += 1;
                }
            }
            branch.suppressions.push(SuppressionRecord {
                id: Uuid::new_v4().to_string(),
                ts: Utc::now().to_rfc3339(),
                subject: subject.to_string(),
                predicate: predicate.to_string(),
                scope: scope.to_string(),
                reason: reason.to_string(),
                suppressed_count: suppressed,
            });
            state.audit.push(audit_entry(
                "user",
                "brain.forget.suppress",
                serde_json::json!({"subject": subject, "predicate": predicate, "scope": scope, "suppressed": suppressed}),
            ));
            Ok(())
        })?;
        Ok(suppressed)
    }

    pub fn attach(&self, brain_ref: &str, grant: AttachmentGrant) -> Result<()> {
        self.mutate_brain(brain_ref, |_, state| {
            state
                .attachments
                .retain(|a| !(a.agent_id == grant.agent_id && a.model_id == grant.model_id));
            state.attachments.push(grant.clone());
            state.audit.push(audit_entry(
                "user",
                "brain.attach",
                serde_json::json!({"agent": grant.agent_id, "model": grant.model_id}),
            ));
            Ok(())
        })
    }

    pub fn detach(&self, brain_ref: &str, agent: &str, model: Option<&str>) -> Result<usize> {
        let mut removed = 0usize;
        self.mutate_brain(brain_ref, |_, state| {
            state.attachments.retain(|a| {
                let hit = a.agent_id == agent && model.is_none_or(|m| m == a.model_id);
                if hit {
                    removed += 1;
                }
                !hit
            });
            state.audit.push(audit_entry(
                "user",
                "brain.detach",
                serde_json::json!({"agent": agent, "model": model, "removed": removed}),
            ));
            Ok(())
        })?;
        Ok(removed)
    }

    pub fn audit_trace(&self, brain_ref: &str) -> Result<Vec<AuditEntry>> {
        let (_, state, _) = self.load_brain_with_secret(brain_ref)?;
        Ok(state.audit)
    }

    pub fn map_api_key(
        &self,
        api_key_plain: &str,
        tenant_id: &str,
        brain_id: &str,
        subject: &str,
    ) -> Result<()> {
        let mut mappings = self.read_api_mappings()?;
        let hash = sha256_hex(api_key_plain.as_bytes());
        mappings.mappings.retain(|m| m.key_hash != hash);
        mappings.mappings.push(ApiKeyMapping {
            key_hash: hash,
            tenant_id: tenant_id.to_string(),
            brain_id: brain_id.to_string(),
            subject: subject.to_string(),
        });
        write_json(self.api_mapping_path(), &mappings)
    }

    pub fn resolve_api_key(&self, api_key_plain: &str) -> Result<Option<ApiKeyMapping>> {
        let hash = sha256_hex(api_key_plain.as_bytes());
        let mappings = self.read_api_mappings()?;
        Ok(mappings.mappings.into_iter().find(|m| m.key_hash == hash))
    }

    pub fn resolve_brain(&self, brain_ref: &str) -> Result<BrainSummary> {
        let all = self.list_brains()?;
        all.into_iter()
            .find(|b| b.brain_id == brain_ref || b.name == brain_ref)
            .ok_or_else(|| anyhow!("brain not found: {brain_ref}"))
    }

    pub fn resolve_brain_or_active(&self, brain_ref: Option<&str>) -> Result<BrainSummary> {
        if let Some(brain_ref) = brain_ref {
            return self.resolve_brain(brain_ref);
        }
        if let Ok(v) = env::var("CORTEX_BRAIN") {
            return self.resolve_brain(v.trim());
        }
        let active = self
            .active_brain_id()?
            .ok_or_else(|| anyhow!("no active brain; run `cortex brain use <brain>`"))?;
        self.resolve_brain(&active)
    }

    fn mutate_brain<F>(&self, brain_ref: &str, f: F) -> Result<()>
    where
        F: FnOnce(&mut BrainManifest, &mut BrainState) -> Result<()>,
    {
        let summary = self.resolve_brain(brain_ref)?;
        let dir = self.brains_dir().join(&summary.brain_id);
        let (mut manifest, mut state, signing_key) = self.load_by_dir(&dir)?;

        f(&mut manifest, &mut state)?;

        manifest.updated_at = Utc::now().to_rfc3339();
        let secret = env::var(&manifest.secret_env_var)
            .with_context(|| format!("missing secret env var {}", manifest.secret_env_var))?;
        let key = derive_key(secret.as_bytes(), &B64.decode(&manifest.kdf_salt_b64)?)?;
        let state_enc = encrypt_json(&key, manifest.brain_id.as_bytes(), &state)?;
        manifest.state_sha256 = sha256_hex(&serde_json::to_vec(&state_enc)?);
        manifest.signature_b64 = sign_manifest(&manifest, &signing_key)?;

        write_json(dir.join("brain.json"), &manifest)?;
        write_json(dir.join("state.enc"), &state_enc)?;
        Ok(())
    }

    fn load_brain_with_secret(
        &self,
        brain_ref: &str,
    ) -> Result<(BrainManifest, BrainState, SigningKey)> {
        let summary = self.resolve_brain(brain_ref)?;
        self.load_by_dir(&self.brains_dir().join(summary.brain_id))
    }

    fn load_by_dir(&self, brain_dir: &Path) -> Result<(BrainManifest, BrainState, SigningKey)> {
        let manifest: BrainManifest = read_json(brain_dir.join("brain.json"))?;
        verify_manifest_signature(&manifest)?;

        let secret = env::var(&manifest.secret_env_var)
            .with_context(|| format!("missing secret env var {}", manifest.secret_env_var))?;
        let key = derive_key(secret.as_bytes(), &B64.decode(&manifest.kdf_salt_b64)?)?;

        let state_enc: EncryptedBlob = read_json(brain_dir.join("state.enc"))?;
        if sha256_hex(&serde_json::to_vec(&state_enc)?) != manifest.state_sha256 {
            bail!("state checksum mismatch for brain {}", manifest.brain_id);
        }
        let state: BrainState = decrypt_json(&key, manifest.brain_id.as_bytes(), &state_enc)?;

        let signing_key_enc: EncryptedBlob =
            read_json(brain_dir.join("keys").join("signing_key.enc"))?;
        let signing_bytes = decrypt_bytes(&key, manifest.brain_id.as_bytes(), &signing_key_enc)?;
        let signing_key = SigningKey::from_bytes(
            &signing_bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow!("invalid signing key bytes"))?,
        );

        Ok((manifest, state, signing_key))
    }

    fn read_config(&self) -> Result<AppConfig> {
        if !self.config_path().exists() {
            return Ok(AppConfig { active_brain: None });
        }
        read_json(self.config_path())
    }

    fn read_api_mappings(&self) -> Result<ApiKeyMappings> {
        if !self.api_mapping_path().exists() {
            return Ok(ApiKeyMappings::default());
        }
        read_json(self.api_mapping_path())
    }

    fn brains_dir(&self) -> PathBuf {
        self.home_dir.join("brains")
    }

    fn config_path(&self) -> PathBuf {
        self.home_dir.join("config.json")
    }

    fn api_mapping_path(&self) -> PathBuf {
        self.home_dir.join("auth").join("api_keys.json")
    }
}

fn audit_entry(actor: &str, action: &str, details: serde_json::Value) -> AuditEntry {
    AuditEntry {
        id: Uuid::new_v4().to_string(),
        ts: Utc::now().to_rfc3339(),
        actor: actor.to_string(),
        action: action.to_string(),
        details,
    }
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in input.chars() {
        let mapped = if c.is_ascii_alphanumeric() {
            c.to_ascii_lowercase()
        } else {
            '-'
        };
        if mapped == '-' {
            if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        } else {
            out.push(mapped);
            prev_dash = false;
        }
    }
    out.trim_matches('-').to_string()
}

fn derive_key(secret: &[u8], salt: &[u8]) -> Result<[u8; 32]> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(secret, salt, &mut key)
        .map_err(|e| anyhow!("argon2 key derivation failed: {e}"))?;
    Ok(key)
}

fn encrypt_json<T: Serialize>(key: &[u8; 32], aad: &[u8], value: &T) -> Result<EncryptedBlob> {
    encrypt_bytes(key, aad, &serde_json::to_vec(value)?)
}

fn decrypt_json<T: for<'de> Deserialize<'de>>(
    key: &[u8; 32],
    aad: &[u8],
    blob: &EncryptedBlob,
) -> Result<T> {
    let bytes = decrypt_bytes(key, aad, blob)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn encrypt_bytes(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> Result<EncryptedBlob> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .map_err(|_| anyhow!("encryption failed"))?;
    Ok(EncryptedBlob {
        nonce_b64: B64.encode(nonce),
        ciphertext_b64: B64.encode(ciphertext),
    })
}

fn decrypt_bytes(key: &[u8; 32], aad: &[u8], blob: &EncryptedBlob) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = B64.decode(&blob.nonce_b64)?;
    let ciphertext = B64.decode(&blob.ciphertext_b64)?;
    let plain = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad,
            },
        )
        .map_err(|_| anyhow!("decryption failed"))?;
    Ok(plain)
}

fn sign_manifest(manifest: &BrainManifest, signing_key: &SigningKey) -> Result<String> {
    let payload = manifest_signing_payload(manifest)?;
    let signature: Signature = signing_key.sign(&payload);
    Ok(B64.encode(signature.to_bytes()))
}

fn verify_manifest_signature(manifest: &BrainManifest) -> Result<()> {
    let key_bytes = B64.decode(&manifest.signing_public_key_b64)?;
    let verifying_key = VerifyingKey::from_bytes(
        &key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow!("invalid verifying key"))?,
    )?;
    let sig_bytes = B64.decode(&manifest.signature_b64)?;
    let signature = Signature::from_bytes(
        &sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow!("invalid signature"))?,
    );

    verifying_key
        .verify(&manifest_signing_payload(manifest)?, &signature)
        .map_err(|_| anyhow!("manifest signature verification failed"))
}

fn manifest_signing_payload(manifest: &BrainManifest) -> Result<Vec<u8>> {
    let mut copy = manifest.clone();
    copy.signature_b64.clear();
    Ok(serde_json::to_vec(&copy)?)
}

fn write_json<P: AsRef<Path>, T: Serialize>(path: P, value: &T) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<P: AsRef<Path>, T: for<'de> Deserialize<'de>>(path: P) -> Result<T> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_export_import_roundtrip() -> Result<()> {
        let temp = tempfile::tempdir()?;
        unsafe {
            env::set_var("TEST_BRAIN_SECRET", "test-secret");
        }

        let store = BrainStore::new(Some(temp.path().to_path_buf()))?;
        let created = store.create_brain(CreateBrainRequest {
            name: "demo".to_string(),
            tenant_id: "tenant-a".to_string(),
            passphrase_env: Some("TEST_BRAIN_SECRET".to_string()),
        })?;
        store.set_active_brain(&created.brain_id)?;

        let out = temp.path().join("demo.cbrain");
        store.export_brain(&created.brain_id, &out)?;

        let verify = store.import_brain(&out, None, true)?;
        assert!(verify.is_none());

        let imported = store.import_brain(&out, Some("demo-copy".to_string()), false)?;
        assert!(imported.is_some());

        let listed = store.list_brains()?;
        assert!(listed.len() >= 2);
        Ok(())
    }

    #[test]
    fn branch_attach_forget_merge_audit() -> Result<()> {
        let temp = tempfile::tempdir()?;
        unsafe {
            env::set_var("TEST_BRAIN_SECRET_2", "test-secret-2");
        }

        let store = BrainStore::new(Some(temp.path().to_path_buf()))?;
        let created = store.create_brain(CreateBrainRequest {
            name: "ops".to_string(),
            tenant_id: "tenant-b".to_string(),
            passphrase_env: Some("TEST_BRAIN_SECRET_2".to_string()),
        })?;

        store.branch(&created.brain_id, "exp-a")?;
        store.attach(
            &created.brain_id,
            AttachmentGrant {
                agent_id: "agent-1".to_string(),
                model_id: "gpt-test".to_string(),
                read_classes: vec!["normative.preference".to_string()],
                write_classes: vec!["normative.preference".to_string()],
                sinks: vec!["none".to_string()],
                expires_at: None,
            },
        )?;

        let suppressed = store.forget_suppress(
            &created.brain_id,
            "user:x",
            "prefers_beverage",
            "SCOPE_GLOBAL",
            "test",
        )?;
        assert_eq!(suppressed, 0);

        let report = store.merge(&created.brain_id, "exp-a", "main", MergeStrategy::Ours)?;
        assert!(report.conflicts.is_empty());

        let audit = store.audit_trace(&created.brain_id)?;
        assert!(!audit.is_empty());
        Ok(())
    }
}
