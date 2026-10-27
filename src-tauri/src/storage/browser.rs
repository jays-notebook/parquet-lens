//! Read-only S3 browsing, independent of the active DataFusion table.

use std::time::Duration;

use futures::TryStreamExt;
use object_store::aws::{AwsAuthorizer, AwsCredential};
use object_store::client::{HttpConnector, HttpRequest, HttpRequestBody, ReqwestConnector};
use object_store::list::{PaginatedListOptions, PaginatedListStore};
use serde::{Deserialize, Serialize};
use url::Url;

use super::remote::{REGION, build_store, client_options, validate_endpoint};

const PAGE_SIZE: usize = 200;

// Never derive Debug: these parameters cross the IPC boundary with credentials.
#[derive(Deserialize)]
pub struct RemoteCredentials {
    pub endpoint: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

impl RemoteCredentials {
    fn validate(&self) -> Result<Url, String> {
        let url = validate_endpoint(&self.endpoint)?;
        // AwsAuthorizer builds an HTTP header from the key ID.
        if self.access_key_id.is_empty()
            || !self.access_key_id.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err("Access key ID must contain printable ASCII without whitespace.".into());
        }
        if self.secret_access_key.trim().is_empty() {
            return Err("Secret key must not be empty.".into());
        }
        Ok(url)
    }
}

#[derive(Debug, Serialize)]
pub struct BucketPage {
    pub buckets: Vec<String>,
    pub next_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RemoteObject {
    pub key: String,
    pub size: u64,
    pub last_modified: String,
}

#[derive(Debug, Serialize)]
pub struct PrefixPage {
    pub prefixes: Vec<String>,
    pub objects: Vec<RemoteObject>,
    pub next_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BucketResponse {
    buckets: Buckets,
    continuation_token: Option<String>,
}

#[derive(Deserialize)]
struct Buckets {
    #[serde(rename = "Bucket", default)]
    bucket: Vec<Bucket>,
}

#[derive(Deserialize)]
struct Bucket {
    #[serde(rename = "Name")]
    name: String,
}

fn next_token(token: Option<String>, previous: Option<&str>) -> Result<Option<String>, String> {
    let token = token.filter(|t| !t.is_empty());
    if token.is_some() && token.as_deref() == previous {
        return Err("The server returned a repeated page token. Refresh the listing.".into());
    }
    Ok(token)
}

fn listing_error(error: object_store::Error) -> String {
    use std::error::Error;
    let mut denied = matches!(
        error,
        object_store::Error::PermissionDenied { .. } | object_store::Error::Unauthenticated { .. }
    );
    let mut missing = matches!(error, object_store::Error::NotFound { .. });
    // object_store 0.13 wraps list HTTP failures in a private RetryError.
    // Inspect its status prefix only; never return a server body or request URL.
    let mut source = error.source();
    while let Some(cause) = source {
        let message = cause.to_string();
        denied |= message.starts_with("Server returned non-2xx status code: 403 ")
            || message.starts_with("Server returned non-2xx status code: 401 ");
        missing |= message.starts_with("Server returned non-2xx status code: 404 ");
        source = cause.source();
    }
    if denied {
        "Folder listing was denied. Enter an allowed prefix or open an object by its exact key."
            .into()
    } else if missing {
        "Bucket not found. Check the bucket name.".into()
    } else {
        "Could not list this folder. Check the connection and bucket, then retry.".into()
    }
}

pub async fn list_buckets(
    credentials: &RemoteCredentials,
    page_token: Option<String>,
) -> Result<BucketPage, String> {
    let mut url = credentials.validate()?;
    url.query_pairs_mut()
        .append_pair("max-buckets", &PAGE_SIZE.to_string());
    if let Some(token) = &page_token {
        url.query_pairs_mut()
            .append_pair("continuation-token", token);
    }
    let mut request = HttpRequest::new(HttpRequestBody::empty());
    *request.uri_mut() = url
        .as_str()
        .parse()
        .map_err(|_| "Invalid endpoint.".to_string())?;
    let credential = AwsCredential {
        key_id: credentials.access_key_id.clone(),
        secret_key: credentials.secret_access_key.clone(),
        token: None,
    };
    AwsAuthorizer::new(&credential, "s3", REGION).authorize(&mut request, None);
    let client = ReqwestConnector::default()
        .connect(&client_options(&url).with_timeout(Duration::from_secs(30)))
        .map_err(|_| "Could not configure the remote connection.".to_string())?;
    let response = client.execute(request).await.map_err(|_| {
        "Could not reach the endpoint. Check the address and connection, then retry.".to_string()
    })?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => {
            return Err(
                "Bucket listing was denied. Check credentials or enter a bucket directly.".into(),
            );
        }
        status => {
            return Err(format!(
                "Bucket listing failed (HTTP {status}). Check the S3 endpoint or enter a bucket directly."
            ));
        }
    }
    // Some compatible servers ignore pagination; bound their XML response.
    let mut stream = response.into_body().bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream
        .try_next()
        .await
        .map_err(|_| "Could not read the bucket listing.".to_string())?
    {
        if body.len() + chunk.len() > 8 * 1024 * 1024 {
            return Err("The bucket list is too large. Enter a bucket directly.".into());
        }
        body.extend_from_slice(&chunk);
    }
    let parsed: BucketResponse = quick_xml::de::from_reader(body.as_slice()).map_err(|_| {
        "Invalid bucket listing response. Check that this is an S3 API endpoint.".to_string()
    })?;
    Ok(BucketPage {
        buckets: parsed.buckets.bucket.into_iter().map(|b| b.name).collect(),
        next_token: next_token(parsed.continuation_token, page_token.as_deref())?,
    })
}

pub async fn list_prefix(
    credentials: &RemoteCredentials,
    bucket: &str,
    prefix: &str,
    page_token: Option<String>,
) -> Result<PrefixPage, String> {
    credentials.validate()?;
    if !prefix.is_empty() && !prefix.ends_with('/') {
        return Err("A folder prefix must end with '/'.".into());
    }
    let store = build_store(
        &credentials.endpoint,
        bucket,
        &credentials.access_key_id,
        &credentials.secret_access_key,
    )?;
    let page = store
        .list_paginated(
            Some(prefix),
            PaginatedListOptions {
                delimiter: Some("/".into()),
                max_keys: Some(PAGE_SIZE),
                page_token: page_token.clone(),
                ..Default::default()
            },
        )
        .await
        .map_err(listing_error)?;
    Ok(PrefixPage {
        // object_store paths omit the trailing delimiter.
        prefixes: page
            .result
            .common_prefixes
            .into_iter()
            .map(|p| format!("{p}/"))
            .collect(),
        objects: page
            .result
            .objects
            .into_iter()
            .filter(|object| object.location.as_ref() != prefix.trim_end_matches('/'))
            .map(|object| RemoteObject {
                key: object.location.to_string(),
                size: object.size,
                last_modified: object.last_modified.to_rfc3339(),
            })
            .collect(),
        next_token: next_token(page.page_token, page_token.as_deref())?,
    })
}
