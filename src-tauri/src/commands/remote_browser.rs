use crate::storage::browser::{self, BucketPage, PrefixPage, RemoteCredentials};

#[tauri::command]
pub async fn list_remote_buckets(
    credentials: RemoteCredentials,
    page_token: Option<String>,
) -> Result<BucketPage, String> {
    browser::list_buckets(&credentials, page_token).await
}

#[tauri::command]
pub async fn list_remote_prefix(
    credentials: RemoteCredentials,
    bucket: String,
    prefix: String,
    page_token: Option<String>,
) -> Result<PrefixPage, String> {
    browser::list_prefix(&credentials, &bucket, &prefix, page_token).await
}
