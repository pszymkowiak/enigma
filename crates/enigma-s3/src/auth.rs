use s3s::auth::{S3Auth, SecretKey};
use s3s::s3_error;

/// Simple static credential auth for Enigma S3 proxy.
pub struct EnigmaS3Auth {
    access_key: String,
    secret_key: String,
}

impl EnigmaS3Auth {
    pub fn new(access_key: String, secret_key: String) -> Self {
        Self {
            access_key,
            secret_key,
        }
    }
}

#[async_trait::async_trait]
impl S3Auth for EnigmaS3Auth {
    async fn get_secret_key(&self, access_key: &str) -> s3s::S3Result<SecretKey> {
        if access_key == self.access_key {
            Ok(SecretKey::from(self.secret_key.clone()))
        } else {
            Err(s3_error!(InvalidAccessKeyId))
        }
    }
}
