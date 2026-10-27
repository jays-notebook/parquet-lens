use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use datafusion::datasource::listing::ListingTableUrl;
use datafusion::execution::object_store::ObjectStoreUrl;
use object_store::{ObjectStore, ObjectStoreExt, memory::InMemory, path::Path};
use parquet_lens_lib::engine::QueryEngine;
use parquet_lens_lib::storage::browser::{RemoteCredentials, list_buckets, list_prefix};
use parquet_lens_lib::storage::{DataSource, RemoteS3Source};
use std::sync::Arc;

fn server(responses: Vec<(u16, impl Into<String>)>) -> (RemoteCredentials, Receiver<String>) {
    let responses: Vec<(u16, String)> = responses
        .into_iter()
        .map(|(status, body)| (status, body.into()))
        .collect();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let count = socket.read(&mut buffer).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
            }
            let _ = tx.send(String::from_utf8(request).unwrap());
            write!(socket, "HTTP/1.1 {status} Response\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (
        RemoteCredentials {
            endpoint,
            access_key_id: "test-access-key".into(),
            secret_access_key: "test-secret-key".into(),
        },
        rx,
    )
}

#[tokio::test]
async fn buckets_are_signed_and_paginated_without_needing_a_bucket() {
    let (credentials, requests) = server(vec![
        (
            200,
            r#"<ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Buckets><Bucket><Name>analytics</Name></Bucket></Buckets><ContinuationToken>next+/=</ContinuationToken></ListAllMyBucketsResult>"#,
        ),
        (
            200,
            "<ListAllMyBucketsResult><Buckets><Bucket><Name>warehouse</Name></Bucket></Buckets></ListAllMyBucketsResult>",
        ),
    ]);
    let first = list_buckets(&credentials, None).await.unwrap();
    assert_eq!(first.buckets, ["analytics"]);
    assert_eq!(first.next_token.as_deref(), Some("next+/="));
    let second = list_buckets(&credentials, first.next_token).await.unwrap();
    assert_eq!(second.buckets, ["warehouse"]);
    assert_eq!(second.next_token, None);
    let first_request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(first_request.starts_with("GET /?max-buckets=200 "));
    assert!(
        first_request
            .to_lowercase()
            .contains("authorization: aws4-hmac-sha256 credential=test-access-key/")
    );
    assert!(!first_request.contains("test-secret-key"));
    let next_request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(next_request.contains("continuation-token=next%2B%2F%3D"));
}

#[tokio::test]
async fn prefix_listing_preserves_keys_and_only_loads_one_level_and_page() {
    let (credentials, requests) = server(vec![(
        200,
        r#"<ListBucketResult>
        <Name>analytics</Name><Prefix>raw/</Prefix><IsTruncated>true</IsTruncated>
        <CommonPrefixes><Prefix>raw/2026/</Prefix></CommonPrefixes>
        <Contents><Key>raw/</Key><Size>0</Size><LastModified>2026-10-20T00:00:00Z</LastModified></Contents>
        <Contents><Key>raw/ 한글 %#?.parquet </Key><Size>42</Size><LastModified>2026-10-20T00:00:00Z</LastModified></Contents>
        <NextContinuationToken>page+/=</NextContinuationToken>
    </ListBucketResult>"#,
    )]);
    let page = list_prefix(&credentials, "analytics", "raw/", Some("start+/=".into()))
        .await
        .unwrap();
    assert_eq!(page.prefixes, ["raw/2026/"]);
    assert_eq!(page.objects.len(), 1);
    assert_eq!(page.objects[0].key, "raw/ 한글 %#?.parquet ");
    assert_eq!(page.objects[0].size, 42);
    assert!(page.objects[0].last_modified.starts_with("2026-10-20"));
    assert_eq!(page.next_token.as_deref(), Some("page+/="));
    let request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(request.starts_with("GET /analytics?"));
    assert!(request.contains("delimiter=%2F"));
    assert!(request.contains("prefix=raw%2F"));
    assert!(request.contains("max-keys=200"));
    assert!(request.contains("continuation-token=start%2B%2F%3D"));
}

#[tokio::test]
async fn denied_bucket_listing_is_actionable_and_does_not_echo_server_content() {
    let (credentials, _) = server(vec![(
        403,
        "<Error><Message>test-secret-key</Message></Error>",
    )]);
    let error = list_buckets(&credentials, None).await.unwrap_err();
    assert!(error.contains("denied"));
    assert!(error.contains("bucket"));
    assert!(!error.contains("test-secret-key"));
}

#[tokio::test]
async fn denied_prefix_listing_is_distinct_from_an_empty_folder() {
    let (credentials, _) = server(vec![(
        403,
        "<Error><Code>AccessDenied</Code><Message>test-secret-key</Message></Error>",
    )]);
    let error = list_prefix(&credentials, "analytics", "", None)
        .await
        .unwrap_err();
    assert!(error.contains("denied"));
    assert!(!error.contains("test-secret-key"));
    let (credentials, _) = server(vec![(
        200,
        "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>",
    )]);
    let page = list_prefix(&credentials, "analytics", "", None)
        .await
        .unwrap();
    assert!(page.prefixes.is_empty());
    assert!(page.objects.is_empty());
    assert_eq!(page.next_token, None);
}

#[tokio::test]
async fn invalid_response_is_not_reported_as_an_empty_bucket_list() {
    let (credentials, _) = server(vec![(200, "<html>not an S3 API</html>")]);
    assert!(list_buckets(&credentials, None).await.is_err());
}

#[tokio::test]
async fn excessive_xml_namespaces_are_rejected_by_both_listing_parsers() {
    let namespaces = (0..300)
        .map(|i| format!(" xmlns:n{i}=\"urn:{i}\""))
        .collect::<String>();
    let (credentials, _) = server(vec![(
        200,
        format!("<ListAllMyBucketsResult{namespaces}><Buckets/></ListAllMyBucketsResult>"),
    )]);
    assert!(
        list_buckets(&credentials, None)
            .await
            .unwrap_err()
            .contains("Invalid bucket listing")
    );
    let (credentials, _) = server(vec![(
        200,
        format!(
            "<ListBucketResult{namespaces}><Name>analytics</Name><IsTruncated>false</IsTruncated></ListBucketResult>"
        ),
    )]);
    assert!(
        list_prefix(&credentials, "analytics", "", None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn invalid_endpoints_credentials_and_prefixes_fail_before_network_io() {
    for endpoint in [
        "http://example.com",
        "http://192.168.1.10:9000",
        "file:///tmp/data",
        "http://user:secret@localhost:9",
        "https://localhost:9/?secret=key",
        "https://localhost:9/#secret",
    ] {
        let credentials = RemoteCredentials {
            endpoint: endpoint.into(),
            access_key_id: "access".into(),
            secret_access_key: "secret".into(),
        };
        let error = list_buckets(&credentials, None).await.unwrap_err();
        assert!(error.contains("endpoint"));
        assert!(!error.contains("secret"));
        let error = list_prefix(&credentials, "analytics", "", None)
            .await
            .unwrap_err();
        assert!(error.contains("endpoint"));
        assert!(!error.contains("secret"));
    }
    let mut credentials = RemoteCredentials {
        endpoint: "http://127.0.0.1:9".into(),
        access_key_id: "invalid\nkey".into(),
        secret_access_key: "secret".into(),
    };
    assert!(
        list_buckets(&credentials, None)
            .await
            .unwrap_err()
            .contains("Access key")
    );
    credentials.access_key_id = "access".into();
    assert!(
        list_prefix(&credentials, "analytics", "raw", None)
            .await
            .unwrap_err()
            .contains("prefix")
    );
    assert!(
        list_prefix(&credentials, "", "", None)
            .await
            .unwrap_err()
            .contains("Bucket")
    );
}

#[test]
fn opening_a_selected_key_preserves_url_delimiters_and_whitespace() {
    let key = "raw/ 한글 %#?.parquet ";
    let source =
        RemoteS3Source::new("https://example.com", "analytics", key, "access", "secret").unwrap();
    let table = source.table_path().unwrap();
    assert_eq!(table.prefix().as_ref(), key);
}

#[tokio::test]
async fn selected_parquet_objects_open_even_without_a_standard_extension() {
    struct MemorySource(Arc<InMemory>, RemoteS3Source);
    impl DataSource for MemorySource {
        fn url(&self) -> ObjectStoreUrl {
            self.1.url()
        }
        fn object_store(&self) -> Arc<dyn ObjectStore> {
            self.0.clone()
        }
        fn table_path(&self) -> Result<ListingTableUrl, String> {
            self.1.table_path()
        }
    }
    let store = Arc::new(InMemory::new());
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/sample.parquet"
    ))
    .unwrap();
    for key in ["raw/ 한글 %#?.parquet ", "raw/part-0000"] {
        store
            .put(&Path::parse(key).unwrap(), bytes.clone().into())
            .await
            .unwrap();
        let source = MemorySource(
            store.clone(),
            RemoteS3Source::new("https://example.com", "analytics", key, "access", "secret")
                .unwrap(),
        );
        let mut engine = QueryEngine::new();
        engine.register_source(&source).await.unwrap();
        let batches = engine
            .ctx()
            .sql("select * from data limit 1")
            .await
            .unwrap()
            .collect()
            .await
            .unwrap();
        assert_eq!(batches[0].num_rows(), 1);
        assert_eq!(engine.get_file_metadata().unwrap().total_rows, 5);
    }
}
