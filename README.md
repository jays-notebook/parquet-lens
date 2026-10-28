# Parquet Lens

Parquet Lens is a desktop GUI for exploring Parquet files with SQL on Windows and macOS.
파일을 열면 곧바로 `SELECT` 쿼리로 내용을 들여다보고, 결과를 그리드에서 확인할 수 있습니다.

> **핵심 가치:** Parquet 파일을 열고, 바로 SQL로 그 안을 살펴본다.

---

## 주요 기능

- **SQL 기반 탐색** — 열린 Parquet 파일은 `data`라는 테이블로 등록되며, `SELECT` 쿼리를 작성해 조회합니다 (읽기 전용).
- **파일 열기** — 파일 다이얼로그 또는 드래그 앤 드롭으로 로컬 Parquet 파일을 엽니다.
- **원격 스토리지 연결** — S3 호환 오브젝트 스토리지(AWS S3 / MinIO 등)에 있는 Parquet 파일도 직접 조회합니다.
- **Remember remote connections** — The last successful connection (endpoint, bucket, object key, and credentials) is restored when the remote form opens, including after an app restart. It is stored in macOS Keychain or Windows Credential Manager, never browser storage or a plain-text settings file. Failed opens keep the previous successful connection. If secure storage is unavailable, the app warns and remembers the connection for the current session only.
- **Browse remote storage** — Enter endpoint and credentials, search loaded buckets, and explore folders in a lazy tree before choosing a file. Breadcrumbs, parent navigation, direct prefix entry, and paginated listings work without recursively scanning the bucket. The browser starts at the last successfully opened file's parent folder. Bucket/key entry remains available when list permissions are missing or a bucket is absent from the server's list; only explicitly opening a file changes the active dataset and saved connection.
- **Browse from the query toolbar** — With a remote file open, click **Browse** beside **Open File** and **Open Remote** to reopen the same browser at the file's parent folder using its current credentials. Local files and ongoing open/query operations disable this shortcut.
- **Results grid** — A scrollable TanStack Table shows result column names and types. All returned rows are rendered; the current grid does not use virtualization.
- **메타데이터 검사** — 스키마(Schema)와 Row Group 정보를 탭으로 확인할 수 있습니다.
- **대용량 파일 대응** — DataFusion의 지연 읽기(projection / predicate pushdown)로 수백 MB~GB급 파일도 스캔합니다.

## Remote connection security

Remote endpoints require HTTPS with a valid certificate. Private MinIO deployments must install their CA in the operating system's trusted certificate store. Certificate verification cannot be disabled.

HTTP is accepted only for `localhost`, IPv4 loopback, or IPv6 loopback endpoints, so a local SSH tunnel can connect to an internal HTTP service. Bind host-side tunnels to loopback; public and private-network HTTP endpoints are rejected. This policy applies to browsing and direct file opens, including restored connections.

Automatic HTTP redirects are rejected. Enter the final S3 API endpoint directly;
redirects cannot bypass the transport policy or move requests outside a tunnel.

## 동작 방식 & 성능

- The backend retains and returns **at most 100 result rows**. The grid renders those rows; its cost also depends on the number of columns and cell contents.
- The executor consumes a result stream without collecting the full result. It may inspect another batch to determine whether the result was truncated. This cap does not limit the engine's input scan, intermediate memory, or execution time: those depend on the query, data, and the user's local hardware. The app adds no automatic query timeout or fixed engine memory budget.
- 쿼리는 `SELECT`만 허용됩니다 — 앱은 원본 파일을 **절대 변경하지 않습니다**.

## 기술 스택

| 영역 | 사용 기술 |
|------|-----------|
| 셸 / 런타임 | [Tauri v2](https://tauri.app/) (Rust 백엔드 + 웹 UI) |
| Query engine | [Apache DataFusion](https://datafusion.apache.org/) 55 (embedded Rust engine) |
| 프런트엔드 | React 18 · TypeScript · Vite · Tailwind CSS |
| 에디터 | CodeMirror (`@codemirror/lang-sql`) |
| Results grid | TanStack Table (TanStack Virtual is installed but currently unused) |
| 상태 관리 | Zustand |
| 원격 스토리지 | `object_store` (S3 호환) |

별도의 외부 엔진 바이너리나 서버 프로세스가 필요 없는 **독립 실행형(standalone) 데스크톱 앱**입니다.

## 요구 사항 / 제약

- **Platforms**: Windows and macOS; the release workflow builds Windows installers and a universal macOS DMG.
- **Build tools**: Rust (2024 edition), Node.js 24.22.1 (pinned in `.node-version`), and platform tooling: Visual Studio C++ Build Tools on Windows or Xcode Command Line Tools on macOS. Use rustup 1.29.2 or later.
- **SQL 범위**: `SELECT` 전용 (읽기 전용)

## 개발 환경 실행

```bash
# Install the locked dependencies
npm ci --no-audit --no-fund

# 개발 모드 실행 (Tauri dev — MSVC 환경이 로드된 셸에서 실행)
npm run tauri dev
```

> Windows에서 Rust 백엔드를 컴파일하려면 MSVC 환경(`vcvars64`)이 로드된 셸이 필요합니다.
> "Developer PowerShell for VS 2022"에서 실행하는 것을 권장합니다.

## 설치파일 빌드

Windows 설치파일(NSIS)은 동봉된 PowerShell 스크립트로 생성합니다.

```powershell
# NSIS 설치파일 생성 (기본값)
.\scripts\build-installer.ps1

# MSI로 생성하거나 재시도 횟수 지정
.\scripts\build-installer.ps1 -Bundles msi -MaxRetries 6
```

이 스크립트는 MSVC 환경 자동 로드, 릴리스 번들 빌드, 산출물 경로 출력을 처리합니다.
생성된 설치파일은 다음 경로에 위치합니다:

```
src-tauri/target/release/bundle/nsis/parquet-lens_<버전>_x64-setup.exe
```

## 프로젝트 구조

```
parquet-lens/
├── src/                      # 프런트엔드 (React + TypeScript)
│   ├── components/           # UI 컴포넌트 (에디터, 결과 그리드, 스키마 탭 등)
│   ├── hooks/                # 파일 열기 / 쿼리 실행 훅
│   └── store/                # Zustand 상태 스토어
├── src-tauri/                # 백엔드 (Rust + Tauri)
│   └── src/
│       ├── commands/         # IPC 커맨드 (파일, 쿼리)
│       ├── engine/           # DataFusion 컨텍스트 & 실행기
│       ├── storage/          # 로컬 / 원격 스토리지 어댑터
│       └── ipc/              # 직렬화 계층
└── scripts/                  # 빌드 스크립트
```

## 라이선스

[Apache License 2.0](./LICENSE) 하에 배포됩니다.

이 프로젝트의 핵심 쿼리 엔진인 **Apache DataFusion · Arrow · Parquet**가 모두 Apache-2.0로 배포되며,
프로젝트도 동일 라이선스를 채택해 코어 엔진과 정합성을 맞추고 특허 사용권(patent grant) 조항의
보호를 받습니다. 나머지 의존성(Tauri, serde, React 등)은 모두 MIT 또는 MIT/Apache 듀얼 라이선스로,
Apache-2.0과 호환됩니다.
