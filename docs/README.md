# 기술 지식 베이스

**Parquet Lens Desktop**의 엔지니어링 레퍼런스 — 각 구성 요소가 어떻게 맞물리는지와
그 배경 개념을 정리한 문서 모음입니다. 각 문서는 **먼저 일반 개념을 설명한 뒤, 이 프로젝트가
실제로 어떻게 구현했는지**(소스 파일 참조 포함)를 이어서 다룹니다.

## 문서 목록

| # | 문서 | 다루는 내용 |
|---|------|-------------|
| 1 | [Tauri & IPC](./01-tauri-and-ipc.md) | Tauri v2 프로세스 모델, Rust 백엔드와 React 프론트엔드가 IPC로 통신하는 방식, 커맨드 표면, 이 앱의 2채널(JSON + 바이너리) 직렬화 전략 |
| 2 | [DataFusion & Arrow](./02-datafusion-and-arrow.md) | 인메모리 컬럼나 포맷 Apache Arrow, 임베디드 SQL 엔진 DataFusion, 100행 캡을 떠받치는 스트리밍 실행 모델, 단일 버전 의존성 규율 |
| 3 | [Parquet 포맷](./03-parquet-format.md) | Parquet 디스크 컬럼나 포맷, footer/row group/column chunk 구조, pushdown 동작 원리, 이 앱이 footer만 읽어 메타데이터를 얻는 방법 |
| 4 | [Zustand 상태 관리](./04-zustand-state-management.md) | Zustand 스토어가 프론트엔드의 단일 진실 공급원 역할을 하는 방식, selector 구독 패턴, 파일 열기/쿼리/결과 라이프사이클을 구동하는 상태 설계 |

## 10초 멘탈 모델

```
┌──────────────────────────┐         IPC          ┌──────────────────────────────┐
│  프론트엔드 (WebView)      │  ◄───────────────►   │  백엔드 (Rust 프로세스)        │
│  React + TypeScript       │  invoke() / Response │  Tauri core + DataFusion       │
│                           │                      │                                │
│  - SQL 에디터              │   JSON  : 메타데이터  │  - SELECT 전용 가드             │
│  - 결과 그리드             │   binary: Arrow IPC  │  - DataFusion SessionContext   │
│  - 스키마 / row group 탭   │                      │  - Parquet 리더 (footer)        │
└──────────────────────────┘                      └───────────────┬────────────────┘
                                                                   │
                                                      ┌────────────┴────────────┐
                                                      │  Parquet 파일             │
                                                      │  로컬 디스크 / S3·MinIO   │
                                                      └──────────────────────────┘
```

Parquet 파일은 **`data`**라는 이름의 SQL 테이블로 등록됩니다. 사용자가 `SELECT`를 작성하면
DataFusion이 계획을 세워 스트리밍 실행하고, 백엔드는 앞쪽 100행에서 멈춘 뒤 이를 Arrow IPC
바이트로 직렬화하며, 프론트엔드가 이를 디코딩해 그리드에 그립니다.
