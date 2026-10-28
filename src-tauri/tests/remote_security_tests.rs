use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use object_store::{ObjectStoreExt, path::Path};
use parquet_lens_lib::storage::browser::{RemoteCredentials, list_buckets, list_prefix};
use parquet_lens_lib::storage::{DataSource, RemoteS3Source};

#[test]
fn remote_files_reject_insecure_or_credential_bearing_endpoints() {
    for endpoint in [
        "http://example.com",
        "http://192.168.1.10:9000",
        "http://[::]:9000",
        "https://user:password@example.com",
        "https://example.com?secret=key",
        "https://example.com#secret",
        "file:///tmp/data",
    ] {
        assert!(
            RemoteS3Source::new(endpoint, "bucket", "data.parquet", "access", "secret").is_err(),
            "accepted unsafe endpoint: {endpoint}"
        );
    }
    for endpoint in [
        "https://example.com",
        "http://localhost:9000",
        "http://127.0.0.1:9000",
        "http://[::1]:9000",
    ] {
        assert!(
            RemoteS3Source::new(endpoint, "bucket", "data.parquet", "access", "secret").is_ok(),
            "rejected secure endpoint or local tunnel: {endpoint}"
        );
    }
}

#[tokio::test]
async fn redirects_cannot_bypass_endpoint_validation() {
    for operation in ["buckets", "prefix", "file"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let redirect = format!(
            "http://localhost.:{}",
            listener.local_addr().unwrap().port()
        );
        assert!(
            RemoteS3Source::new(&redirect, "bucket", "data.parquet", "access", "secret").is_err()
        );
        let done = Arc::new(AtomicBool::new(false));
        let server_done = Arc::clone(&done);
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut requests = 0;
            while !server_done.load(Ordering::SeqCst) && Instant::now() < deadline {
                let (mut socket, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("accept failed: {error}"),
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let size = socket.read(&mut buffer).unwrap();
                    assert!(size > 0);
                    request.extend_from_slice(&buffer[..size]);
                }
                requests += 1;
                if requests == 1 {
                    write!(socket, "HTTP/1.1 302 Found\r\nLocation: {redirect}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    write!(
                        socket,
                        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                }
            }
            requests
        });
        let credentials = RemoteCredentials {
            endpoint: endpoint.clone(),
            access_key_id: "access".into(),
            secret_access_key: "secret".into(),
        };
        let result = match operation {
            "buckets" => list_buckets(&credentials, None).await.map(|_| ()),
            "prefix" => list_prefix(&credentials, "bucket", "", None)
                .await
                .map(|_| ()),
            _ => RemoteS3Source::new(&endpoint, "bucket", "data.parquet", "access", "secret")
                .unwrap()
                .object_store()
                .head(&Path::from("data.parquet"))
                .await
                .map(|_| ())
                .map_err(|error| error.to_string()),
        };
        done.store(true, Ordering::SeqCst);
        assert_eq!(
            server.join().unwrap(),
            1,
            "{operation} followed an unchecked redirect"
        );
        assert!(result.is_err(), "{operation} accepted a redirect");
    }
}

#[tokio::test]
async fn bucket_listing_rejects_untrusted_tls_before_sending_credentials() {
    let certificate = rustls::pki_types::CertificateDer::from(
        include_bytes!("fixtures/untrusted-cert.der").to_vec(),
    );
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(
        include_bytes!("fixtures/untrusted-key.der").to_vec(),
    );
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key.into())
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("https://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let connection = rustls::ServerConnection::new(Arc::new(config)).unwrap();
        let mut tls = rustls::StreamOwned::new(connection, socket);
        let mut request = [0; 4096];
        match tls.read(&mut request) {
            Ok(size) if size > 0 => {
                let body = "<ListAllMyBucketsResult><Buckets/></ListAllMyBucketsResult>";
                write!(
                    tls,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                true
            }
            _ => false,
        }
    });
    let result = list_buckets(
        &RemoteCredentials {
            endpoint,
            access_key_id: "test-access".into(),
            secret_access_key: "test-secret".into(),
        },
        None,
    )
    .await;
    let received_request = server.join().unwrap();
    assert!(result.is_err(), "accepted an untrusted TLS certificate");
    assert!(
        !received_request,
        "sent an authenticated request before verifying TLS"
    );
}
