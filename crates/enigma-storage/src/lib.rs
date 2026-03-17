#[cfg(feature = "azure")]
pub mod azure;
#[cfg(feature = "gcs")]
pub mod gcs;
pub mod local;
pub mod provider;
#[cfg(feature = "s3")]
pub mod s3;
