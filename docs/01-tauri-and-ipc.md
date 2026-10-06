# Tauri & IPC

Rust 백엔드와 React 프론트엔드가 어떻게 연결되고, 어떻게 통신하는지 설명합니다.

---

## 1. Tauri란

[Tauri](https://tauri.app/)는 다음 구조로 네이티브 데스크톱 앱을 만드는 프레임워크입니다:

- **백엔드**는 네이티브 **Rust** 프로세스 (파일시스템, 다이얼로그, 네트워크 등 OS 전 영역 접근)
- **프론트엔드**는 OS의 **WebView**(Windows에서는 WebView2)에 렌더링되는 웹 UI(HTML/CSS/JS).
  Electron처럼 Chromium을 통째로 번들하지 않습니다.

이 분리가 핵심입니다: 두 절반은 **서로 다른 프로세스**에서 돌며 **메모리를 공유하지 않습니다**.
UI는 디스크를 직접 만지거나 SQL을 직접 실행할 수 없고, 반드시 Rust 쪽에 요청해야 합니다.
그 요청/응답 통로가 **IPC**(프로세스 간 통신)입니다.

```
main.rs ──► lib.rs::run()
              tauri::Builder
                ├─ .plugin(...)          네이티브 기능 (다이얼로그 등)
                ├─ .manage(AppState)     백엔드 공유 상태
                ├─ .invoke_handler(...)  IPC 커맨드 표면
                └─ .run(generate_context!())
```

실제 빌더는 `src-tauri/src/lib.rs`를 참고하세요.

## 2. 애플리케이션 골격

`src-tauri/src/lib.rs::run()`에서 모든 것이 등록됩니다:

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())          // 네이티브 파일 다이얼로그
    .manage(AppState {                            // 앱당 하나인 공유 상태
        engine: tokio::sync::Mutex::new(QueryEngine::new()),
    })
    .invoke_handler(tauri::generate_handler![     // IPC 표면
        commands::file::open_file,
        commands::file::get_file_metadata,
        commands::file::open_remote_file,
        commands::query::run_query,
        commands::query::get_last_result_meta,
        commands::query::get_page,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

여기서 이해할 세 가지:

- **플러그인**은 네이티브 기능을 추가합니다. `tauri-plugin-dialog`가 OS의 "파일 열기"
  다이얼로그를 띄웁니다.
- **관리 상태**(`.manage(...)`)는 앱 생애 전체 동안 백엔드가 소유하는 싱글턴입니다. 여기서는
  `QueryEngine`을 담고 있으며, IPC 커맨드가 `async`이고 동시에 실행될 수 있으므로
  `tokio::sync::Mutex`로 감쌉니다. 커맨드는 `tauri::State<'_, AppState>`로 이를 빌려옵니다.
- **`invoke_handler` + `generate_handler!`**는 프론트엔드가 호출할 수 있는 Rust 함수를 정확히
  선언합니다. 여기에 없는 것은 WebView에서 도달할 수 없습니다.

## 3. 커맨드: IPC의 동사(verb)

**커맨드**는 `#[tauri::command]` 애너테이션이 붙은 Rust 함수입니다. 프론트엔드가 백엔드 로직을
트리거할 수 있는 유일한 방법입니다. 예시(`src-tauri/src/commands/query.rs`):

```rust
#[tauri::command]
pub async fn run_query(
    sql: String,
    state: tauri::State<'_, AppState>,
) -> Result<tauri::ipc::Response, String> {
    guard_select_only(&sql)?;
    let (meta, ipc_bytes) = {
        let mut engine = state.engine.lock().await;
        engine.execute(&sql).await?
    };
    let frame = crate::ipc::frame::frame_meta_and_bytes(&meta, &ipc_bytes)?;
    Ok(tauri::ipc::Response::new(frame))
}
```

이 코드베이스의 핵심 관례:

- **모든 커맨드는 `Result<T, String>`을 반환합니다.** 오류는 프론트엔드에서 reject된 Promise가
  되고, 성공은 `T`로 resolve됩니다. 커맨드 경로에서 `.unwrap()`/`.expect()`는 쓰지 않습니다.
- **async 커맨드는 Tauri 자체 Tokio 런타임에서 돕니다.** 커맨드 안에서
  `tokio::runtime::Runtime`을 직접 만들면 안 됩니다 — Tauri가 이미 런타임을 소유합니다
  (`lib.rs` 주석 참고).
- **보안 가드는 백엔드에서 실행됩니다.** `guard_select_only`는 `run_query`의 첫 줄에서, 엔진
  락 획득 *이전에* 호출됩니다. 따라서 직접 만든 IPC 호출로도 SELECT 전용 정책을 우회할 수
  없습니다. 문자열 매칭이 아니라 `DFParser::parse_sql`로 SQL을 AST로 파싱하므로, 대소문자
  혼용·주석·다중 구문 인젝션으로 회피할 수 없습니다.

### 커맨드의 프론트엔드 쪽

프론트엔드는 `@tauri-apps/api/core`의 `invoke()`로 커맨드를 호출합니다. 이 프로젝트는 모든
커맨드를 `src/lib/tauri.ts`의 타입 지정 함수로 감싸며, 컴포넌트는 `invoke`를 직접 호출하지
않습니다:

```ts
import { invoke } from "@tauri-apps/api/core";

export async function openFile(path: string): Promise<OpenFileResponse> {
  return invoke<OpenFileResponse>("open_file", { path });
}
```

**인자 객체의 키는 Rust 파라미터 이름과 일치해야 합니다**(`path`, `sql`, `conn`, …). Tauri는
Rust 구조체를 `serde`로 **이름 변경 없이** 직렬화하므로, TypeScript 인터페이스는 Rust 필드명을
그대로 미러링하는 **snake_case** 키를 사용합니다(`total_rows`, `object_key` 등).

## 4. 이 앱의 커맨드 표면

| 커맨드 | 인자 | 반환 | 목적 |
|--------|------|------|------|
| `open_file` | `path` | `OpenFileResponse { schema }` | 로컬 Parquet 파일을 테이블 `data`로 등록 |
| `open_remote_file` | `conn: RemoteConnection` | `OpenFileResponse { schema }` | 원격 S3/MinIO Parquet 객체를 `data`로 등록 |
| `get_file_metadata` | – | `FileMetadata` | Parquet footer 통계(총 행 수 + row group별 정보) |
| `run_query` | `sql` | `tauri::ipc::Response` containing a framed JSON header and Arrow IPC bytes | Executes a read-only query and returns its metadata and up to 100 rows together |
| `get_last_result_meta` | – | `RunQueryResponse { total_rows, capped, schema }` | Secondary accessor for the latest cached result metadata; not called by the frontend query flow |
| `get_page` | `offset, size` | `PageResponse { rows, offset, has_more }` | Slices the cached result of at most 100 rows; does not fetch additional rows from the source |

## 5. Single-response framing

`run_query` makes **one IPC round trip**. Its binary response contains a JSON
metadata segment and an Arrow IPC stream from the same execution:

```text
[0..4)              metadata byte length (u32, little-endian)
[4..4+meta_len)     UTF-8 JSON: { total_rows, capped, schema }
[4+meta_len..)      Arrow IPC stream bytes
```

The backend builds this frame in `src-tauri/src/ipc/frame.rs`. The frontend
validates the length prefix, parses the metadata, and decodes the remaining
bytes in `decodeQueryFrame`:

```ts
// src/lib/tauri.ts — runQuery()
const frame = await invoke<ArrayBuffer>("run_query", { sql });
return decodeQueryFrame(frame);
```

The metadata schema describes the **query result**, including aliases and
aggregates, rather than the opened file. A zero-row result has an empty Arrow
payload but still includes its result schema in the JSON segment. Metadata
size depends on the columns; there is no enforced 10 KB metadata limit.

Returning both parts together prevents interleaved commands from pairing one
query's rows with another query's metadata. `get_last_result_meta` remains a
secondary cache accessor; it is not a second step in `runQuery`.

`get_page` returns JSON rows from the already retained cache. Neither it nor
the current grid retrieves rows beyond the 100-row cap.

## 6. 엔드 투 엔드 왕복(쿼리 실행)

```
Run button / editor shortcut → useRunQuery → runQuery(sql)
  → invoke("run_query", { sql })
  → guard_select_only → engine.execute → frame_meta_and_bytes
  ← one binary response: [length | JSON metadata | Arrow IPC]
  → decodeQueryFrame → tableFromIPC → arrowTableToRows
  → setResults(rows and result schema) → TanStack Table
```

`execute`와 `record_batches_to_ipc`의 컬럼나/Arrow 세부 사항은
[DataFusion & Arrow](./02-datafusion-and-arrow.md)를 참고하세요.

## 7. 이 설계의 이유

- **프로세스 격리 = 안전성.** WebView는 커맨드 표면이 허용하는 일만 할 수 있습니다. SELECT
  전용 가드가 Rust에 있으므로, 읽기 전용 보장은 UI에서 무력화할 수 없습니다.
- **바이너리 Arrow 전송 = 속도.** 표 형식 데이터가 JSON을 거치지 않고, 프론트엔드는 엔진이
  이미 만들어낸 Arrow 포맷을 그대로 소비합니다 — 전송 중 데이터 재구성이 없습니다.
- **타입 지정 래퍼 모듈 = 단일 진실 공급원.** `src/lib/tauri.ts`가 계약입니다. Rust serde
  이름과 TS 인터페이스의 불일치가 한 파일에서 드러납니다.
