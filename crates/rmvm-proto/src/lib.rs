use prost::Message;
use sha2::{Digest, Sha256};

pub mod cortex {
    pub mod rmvm {
        pub mod v3_1 {
            include!(concat!(env!("OUT_DIR"), "/cortex.rmvm.v3_1.rs"));
        }
    }
}

pub use cortex::rmvm::v3_1::*;

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn canonical_assertion_bytes(assertion: &VerifiedAssertion) -> Vec<u8> {
    let mut canonical = assertion.clone();
    canonical
        .citations
        .sort_by(|a, b| a.anchor_digest.cmp(&b.anchor_digest));
    canonical.encode_to_vec()
}

pub fn canonical_assertion_hash(assertion: &VerifiedAssertion) -> String {
    sha256_hex(&canonical_assertion_bytes(assertion))
}

pub fn trust_tier_from_i32(v: i32) -> TrustTier {
    TrustTier::try_from(v).unwrap_or(TrustTier::Unspecified)
}

pub fn availability_from_i32(v: i32) -> HandleAvailability {
    HandleAvailability::try_from(v).unwrap_or(HandleAvailability::Unspecified)
}

pub fn selector_return_from_i32(v: i32) -> SelectorReturn {
    SelectorReturn::try_from(v).unwrap_or(SelectorReturn::Unspecified)
}

pub fn param_type_from_i32(v: i32) -> ParamType {
    ParamType::try_from(v).unwrap_or(ParamType::Unspecified)
}

pub fn assertion_type_from_i32(v: i32) -> AssertionType {
    AssertionType::try_from(v).unwrap_or(AssertionType::Unspecified)
}

pub fn edge_type_from_i32(v: i32) -> EdgeType {
    EdgeType::try_from(v).unwrap_or(EdgeType::Unspecified)
}
