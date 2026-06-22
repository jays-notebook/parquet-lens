//! Storage source abstraction — the v1→v2 seam.
//!
//! The `DataSource` trait is the ONLY storage abstraction the engine layer sees.
//! No `PathBuf` or raw `String` path may leak past this boundary into the engine
//! (PITFALLS.md §Pitfall 10, ARCHITECTURE.md Anti-Pattern 3).
//!
//! Adding MinIO/S3 in v2 means adding `remote.rs` — zero changes to engine or commands.

use std::sync::Arc;

use datafusion::execution::object_store::ObjectStoreUrl;
use datafusion::datasource::listing::ListingTableUrl;
use object_store::ObjectStore;

pub mod local;
pub mod remote;

pub use local::LocalFileSource;
pub use remote::RemoteS3Source;

/// Abstraction over a data source that can be registered with a DataFusion `SessionContext`.
///
/// Implementations provide an `(ObjectStoreUrl, Arc<dyn ObjectStore>, ListingTableUrl)` triple.
/// The engine registers the object store and listing table using only this trait — it is
/// completely storage-agnostic.
///
/// `Send + Sync` bounds are required because the trait object may cross async task boundaries.
pub trait DataSource: Send + Sync {
    /// Returns the URL key used to register the object store in DataFusion's registry.
    fn url(&self) -> ObjectStoreUrl;

    /// Returns the concrete `ObjectStore` implementation for this source.
    fn object_store(&self) -> Arc<dyn ObjectStore>;

    /// Returns the listing table URL pointing at the Parquet file or prefix.
    ///
    /// Returns `Err` if the path cannot be parsed as a `ListingTableUrl`.
    fn table_path(&self) -> Result<ListingTableUrl, String>;
}
