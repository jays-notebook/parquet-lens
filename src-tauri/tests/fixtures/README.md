# TLS test fixtures

`untrusted-cert.der` and `untrusted-key.der` are a self-signed localhost certificate
and its disposable PKCS#8 private key, generated for the TLS rejection test.
They are public test data, not application credentials or a trusted CA.
The certificate includes `localhost` and `127.0.0.1` as subject alternative names.
