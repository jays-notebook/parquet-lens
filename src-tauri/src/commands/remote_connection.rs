//! Persist the last successful connection as one secret in the OS credential store.

use keyring::Entry;

use super::file::RemoteConnection;

const READ_ERROR: &str =
    "Could not restore the saved remote connection. Enter connection details manually.";
const SAVE_ERROR: &str = "Connected, but the connection could not be saved securely. It will only be remembered until the app closes.";

fn entry() -> Result<Entry, String> {
    // keyring falls back to a non-persistent mock on unsupported platforms.
    if !cfg!(any(target_os = "macos", target_os = "windows")) {
        return Err("Secure connection storage is unavailable on this platform.".into());
    }
    Entry::new("com.parquet-lens.app", "last-remote-connection")
        .map_err(|_| "Could not access the OS credential store.".into())
}

fn read(entry: &Entry) -> Result<Option<RemoteConnection>, String> {
    match entry.get_secret() {
        Ok(bytes) => {
            let conn: RemoteConnection =
                serde_json::from_slice(&bytes).map_err(|_| READ_ERROR.to_string())?;
            if [
                &conn.endpoint,
                &conn.bucket,
                &conn.object_key,
                &conn.access_key_id,
                &conn.secret_access_key,
            ]
            .iter()
            .any(|value| value.trim().is_empty())
            {
                return Err(READ_ERROR.into());
            }
            Ok(Some(conn))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(READ_ERROR.into()),
    }
}

fn write(entry: &Entry, conn: &RemoteConnection) -> Result<(), String> {
    let bytes = serde_json::to_vec(conn).map_err(|_| SAVE_ERROR.to_string())?;
    entry.set_secret(&bytes).map_err(|_| SAVE_ERROR.into())
}

#[tauri::command]
pub async fn get_last_remote_connection() -> Result<Option<RemoteConnection>, String> {
    tauri::async_runtime::spawn_blocking(|| read(&entry()?))
        .await
        .map_err(|_| READ_ERROR.to_string())?
}

pub(super) async fn remember(conn: RemoteConnection) -> Option<String> {
    // A credential-store failure must not turn a successful file open into an error.
    match tauri::async_runtime::spawn_blocking(move || write(&entry()?, &conn)).await {
        Ok(Ok(())) => None,
        _ => Some(SAVE_ERROR.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyring::credential::CredentialBuilderApi;

    fn mock_entry() -> Entry {
        let credential = keyring::mock::MockCredentialBuilder {}
            .build(None, "parquet-lens-test", "connection")
            .unwrap();
        Entry::new_with_credential(credential)
    }

    fn connection() -> RemoteConnection {
        RemoteConnection {
            endpoint: "https://example.com:9000".into(),
            bucket: "bucket".into(),
            object_key: "folder/한글.parquet".into(),
            access_key_id: "access".into(),
            secret_access_key: " secret with spaces ".into(),
        }
    }

    #[test]
    fn restores_all_fields_and_replaces_the_previous_connection() {
        let entry = mock_entry();
        assert!(read(&entry).unwrap().is_none());
        let mut conn = connection();
        write(&entry, &conn).unwrap();
        let restored = read(&entry).unwrap().unwrap();
        assert_eq!(restored.endpoint, "https://example.com:9000");
        assert_eq!(restored.bucket, "bucket");
        assert_eq!(restored.object_key, "folder/한글.parquet");
        assert_eq!(restored.access_key_id, "access");
        assert_eq!(restored.secret_access_key, " secret with spaces ");

        conn.endpoint = "https://next.example.com".into();
        write(&entry, &conn).unwrap();
        assert_eq!(
            read(&entry).unwrap().unwrap().endpoint,
            "https://next.example.com"
        );
    }

    #[test]
    fn rejects_corrupt_or_incomplete_saved_data_without_exposing_it() {
        let entry = mock_entry();
        for bytes in [b"private-corrupt-value".as_slice(), b"{}"] {
            entry.set_secret(bytes).unwrap();
            assert_eq!(read(&entry).err().unwrap(), READ_ERROR);
        }
        let mut conn = connection();
        conn.endpoint = " ".into();
        write(&entry, &conn).unwrap();
        assert_eq!(read(&entry).err().unwrap(), READ_ERROR);
    }

    #[test]
    fn storage_failure_preserves_previous_data_and_redacts_errors() {
        let entry = mock_entry();
        write(&entry, &connection()).unwrap();
        let mock = entry
            .get_credential()
            .downcast_ref::<keyring::mock::MockCredential>()
            .unwrap();
        mock.set_error(keyring::Error::Invalid(
            "private-value".into(),
            "private-value".into(),
        ));
        assert_eq!(write(&entry, &connection()).unwrap_err(), SAVE_ERROR);
        assert_eq!(
            read(&entry).unwrap().unwrap().endpoint,
            "https://example.com:9000"
        );
        mock.set_error(keyring::Error::Invalid(
            "private-value".into(),
            "private-value".into(),
        ));
        assert_eq!(read(&entry).err().unwrap(), READ_ERROR);
    }
}
