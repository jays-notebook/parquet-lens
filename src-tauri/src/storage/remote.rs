//! Remote S3/MinIO implementation of `DataSource`.
//!
//! `RemoteS3Source` wraps an `AmazonS3` object store and implements the `DataSource` trait
//! for Parquet objects hosted on S3-compatible endpoints (MinIO, AWS S3, etc.).
//!
//! All S3 client construction — custom endpoint, path-style addressing, HTTP allowance,
//! certificate verification, and credentials — is encapsulated here. Nothing leaks
//! past the `storage/` boundary into the engine or command layers
//! (ARCHITECTURE.md Anti-Pattern 3).
//!
//! # Credential Safety
//!
//! This struct intentionally does NOT derive `Debug` to prevent credentials from being
//! exposed in log output (RESEARCH §Pitfall 3, T-05-01).

use std::sync::Arc;

use datafusion::datasource::listing::ListingTableUrl;
use datafusion::execution::object_store::ObjectStoreUrl;
use object_store::ObjectStore;
use object_store::aws::{AmazonS3, AmazonS3Builder};
use object_store::client::ClientOptions;
use url::{Host, Url};

use super::DataSource;

pub(super) const REGION: &str = "us-east-1";

pub(super) fn validate_endpoint(endpoint: &str) -> Result<Url, String> {
    let invalid = || {
        "Invalid endpoint. Use HTTPS without embedded credentials, query, or fragment. HTTP is allowed only for loopback tunnels.".to_string()
    };
    let url = Url::parse(endpoint).map_err(|_| invalid())?;
    let loopback = match url.host() {
        Some(Host::Domain("localhost")) => true,
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if url.host().is_none()
        || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(url)
}

pub(super) fn client_options(endpoint: &Url) -> ClientOptions {
    // Endpoint validation permits plaintext only inside a local tunnel.
    ClientOptions::new().with_allow_http(endpoint.scheme() == "http")
}

pub(super) fn build_store(
    endpoint: &str,
    bucket: &str,
    access_key_id: &str,
    secret_access_key: &str,
) -> Result<AmazonS3, String> {
    let endpoint = validate_endpoint(endpoint)?;
    if bucket.trim().is_empty() {
        return Err("Bucket name must not be empty".into());
    }
    ObjectStoreUrl::parse(format!("s3://{bucket}"))
        .map_err(|_| "Invalid bucket name: must be a valid S3 bucket identifier".to_string())?;
    AmazonS3Builder::new()
        .with_endpoint(endpoint.as_str().trim_end_matches('/'))
        .with_bucket_name(bucket)
        .with_access_key_id(access_key_id)
        .with_secret_access_key(secret_access_key)
        .with_region(REGION)
        .with_client_options(client_options(&endpoint))
        .build()
        .map_err(|_| "Could not configure remote storage. Check the connection details.".into())
}

/// A remote Parquet object on an S3-compatible endpoint, ready to be registered with DataFusion.
///
/// Credentials are not kept as separate struct fields, but they are NOT discarded after
/// construction: `AmazonS3Builder` bakes them into the `Arc<AmazonS3>` client, which retains
/// them internally to sign every request for the store's lifetime. They are never re-exposed
/// through this struct (no accessor, no `Debug`).
///
/// # Credential Exposure
///
/// `Debug` is deliberately NOT derived on this struct to prevent `secret_access_key` from
/// appearing in log output (RESEARCH §Pitfall 3).
pub struct RemoteS3Source {
    bucket: String,
    object_key: String,
    store: Arc<AmazonS3>,
}

impl RemoteS3Source {
    /// Creates a new `RemoteS3Source` from S3/MinIO connection parameters.
    ///
    /// Constructs an `AmazonS3` store configured for path-style addressing (MinIO default),
    /// verified HTTPS and loopback HTTP tunnels. Returns `Err` if the store cannot be built
    /// (e.g., invalid endpoint URL) — no panic.
    ///
    /// # Arguments
    ///
    /// * `endpoint` — Full URL of the MinIO/S3 endpoint, e.g. `https://minio.example.com`
    /// * `bucket` — Bucket name
    /// * `object_key` — Object key (path within the bucket), e.g. `folder/data.parquet`
    /// * `access_key_id` — S3 access key ID
    /// * `secret_access_key` — S3 secret access key
    pub fn new(
        endpoint: &str,
        bucket: &str,
        object_key: &str,
        access_key_id: &str,
        secret_access_key: &str,
    ) -> Result<Self, String> {
        // Defense-in-depth input guards (WR-03): the frontend gates on non-empty fields,
        // but a direct `invoke("open_remote_file", …)` must not bypass validation.
        if bucket.trim().is_empty() {
            return Err("Bucket name must not be empty".to_string());
        }
        if object_key.trim().is_empty() {
            return Err("Object key must not be empty".to_string());
        }
        // Validate the bucket forms a parseable `s3://` authority *before* storing it, so
        // `url()` (whose trait signature is infallible) can never panic on user input later
        // (CR-01 / REMOTE-02 "non-crashing" / PITFALLS §9 "never panics").
        ObjectStoreUrl::parse(format!("s3://{}", bucket))
            .map_err(|_| "Invalid bucket name: must be a valid S3 bucket identifier".to_string())?;

        let store = build_store(endpoint, bucket, access_key_id, secret_access_key)?;

        Ok(Self {
            bucket: bucket.to_string(),
            object_key: object_key.to_string(),
            store: Arc::new(store),
        })
    }
}

impl DataSource for RemoteS3Source {
    /// Returns the URL key used to register the object store in DataFusion's registry.
    ///
    /// The key is `s3://{bucket}` — scheme + authority only. DataFusion's `ObjectStoreRegistry`
    /// matches on scheme+authority during query execution, not the full object path
    /// (RESEARCH §Pitfall 1). The custom endpoint is encoded in the `AmazonS3` store itself.
    fn url(&self) -> ObjectStoreUrl {
        // `new()` already proved this parses (CR-01), so the bucket cannot be unparseable
        // here — this expect is unreachable on any value that survived construction.
        ObjectStoreUrl::parse(format!("s3://{}", self.bucket))
            .expect("bucket validated parseable in RemoteS3Source::new()")
    }

    /// Returns the pre-built `AmazonS3` store.
    fn object_store(&self) -> Arc<dyn ObjectStore> {
        // Explicit coercion from Arc<AmazonS3> to Arc<dyn ObjectStore> is required because
        // Arc::clone preserves the concrete type; the trait object must be constructed explicitly.
        Arc::clone(&self.store) as Arc<dyn ObjectStore>
    }

    /// Returns the listing table URL pointing at the single remote Parquet object.
    ///
    /// The URL is `s3://{bucket}/{object_key}` with no trailing slash — single-file semantics,
    /// not a prefix scan (RESEARCH §Pattern 1).
    fn table_path(&self) -> Result<ListingTableUrl, String> {
        let mut url = url::Url::parse(&format!("s3://{}", self.bucket))
            .map_err(|_| "Invalid bucket name".to_string())?;
        // A key is literal data, not a URL: escape %, # and ? as well as Unicode.
        url.path_segments_mut()
            .map_err(|_| "Invalid remote path".to_string())?
            .clear()
            .extend(self.object_key.split('/'));
        ListingTableUrl::parse(url.as_str())
            .map_err(|e| format!("Failed to parse remote path as ListingTableUrl: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::execution::object_store::ObjectStoreUrl;

    const EP: &str = "https://example.com";
    const AK: &str = "ak";
    const SK: &str = "sk";

    // Note: RemoteS3Source deliberately does not implement Debug (credential safety), so the
    // Ok variant cannot be unwrapped — use `.err().expect(...)` to inspect the error string.

    // CR-01: a bucket name that is not a valid s3:// authority must return Err, NOT panic.
    #[test]
    fn invalid_bucket_returns_err_not_panic() {
        let err = RemoteS3Source::new(EP, "my bucket", "k.parquet", AK, SK)
            .err()
            .expect("invalid bucket should be Err");
        assert!(
            err.contains("Invalid bucket name"),
            "unexpected error: {err}"
        );
    }

    // WR-03: blank bucket / object key are rejected at the backend boundary.
    #[test]
    fn empty_bucket_rejected() {
        let err = RemoteS3Source::new(EP, "   ", "k.parquet", AK, SK)
            .err()
            .expect("empty bucket should be Err");
        assert!(
            err.contains("Bucket name must not be empty"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn empty_object_key_rejected() {
        let err = RemoteS3Source::new(EP, "bucket", "  ", AK, SK)
            .err()
            .expect("empty object key should be Err");
        assert!(
            err.contains("Object key must not be empty"),
            "unexpected error: {err}"
        );
    }

    // A valid bucket constructs successfully and url() does not panic.
    #[test]
    fn valid_bucket_builds_and_url_is_infallible() {
        let src = RemoteS3Source::new(EP, "valid-bucket", "dir/data.parquet", AK, SK)
            .expect("valid params should build");
        let expected = ObjectStoreUrl::parse("s3://valid-bucket").unwrap();
        assert_eq!(src.url(), expected);
    }

    // REMOTE-01: table_path() returns the correct s3://{bucket}/{object_key} ListingTableUrl.
    #[test]
    fn table_path_returns_s3_listing_url() {
        let src = RemoteS3Source::new(EP, "valid-bucket", "dir/data.parquet", AK, SK)
            .expect("valid params should build");
        let result = src.table_path();
        assert!(
            result.is_ok(),
            "table_path() should return Ok, got: {:?}",
            result.err()
        );
        let expected = ListingTableUrl::parse("s3://valid-bucket/dir/data.parquet").unwrap();
        assert_eq!(result.unwrap(), expected);
    }
}
