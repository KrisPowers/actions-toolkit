use anyhow::Result;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user id
    pub sid: String, // session id
    pub exp: i64,
}

#[derive(Clone)]
pub struct JwtCodec {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtCodec {
    pub fn new(secret: &str) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    pub fn encode(&self, user_id: &str, session_id: &str, ttl: chrono::Duration) -> Result<String> {
        let exp = (chrono::Utc::now() + ttl).timestamp();
        let claims = Claims {
            sub: user_id.to_string(),
            sid: session_id.to_string(),
            exp,
        };
        Ok(encode(&Header::default(), &claims, &self.encoding_key)?)
    }

    pub fn decode(&self, token: &str) -> Result<Claims> {
        let data = decode::<Claims>(token, &self.decoding_key, &Validation::default())?;
        Ok(data.claims)
    }
}
