use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    pub payload: Vec<u8>,
    pub stored_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CacheEnvelope {
    stored_at: DateTime<Utc>,
    payload: Value,
}

pub fn encode_entry(payload: &[u8]) -> Result<Vec<u8>, serde_json::Error> {
    let payload = serde_json::from_slice::<Value>(payload)?;
    serde_json::to_vec(&json!({
        "stored_at": Utc::now(),
        "payload": payload,
    }))
}

pub fn decode_entry(bytes: &[u8]) -> Result<CacheEntry, serde_json::Error> {
    let envelope = serde_json::from_slice::<CacheEnvelope>(bytes)?;

    Ok(CacheEntry {
        payload: serde_json::to_vec(&envelope.payload)?,
        stored_at: envelope.stored_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_envelopes_roundtrip_payloads() {
        let encoded = encode_entry(br#"{"name":"Noah","id":1}"#).unwrap();
        let decoded = decode_entry(&encoded).unwrap();

        assert_eq!(decoded.payload, br#"{"id":1,"name":"Noah"}"#);
    }
}
