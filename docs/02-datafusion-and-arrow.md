# DataFusion & Arrow

쿼리 엔진과, 그것을 떠받치는 인메모리 데이터 포맷을 설명합니다.

---

## 1. Apache Arrow: 인메모리 컬럼나 포맷

[Apache Arrow](https://arrow.apache.org/)는 표 형식 데이터를 **메모리 안에서**, 행 단위가 아니라
**열(column) 단위로** 표현하는 표준입니다. 한 청크의 모든 열은 같은 길이를 가지며, 각각 연속된
타입 버퍼로 저장됩니다.

백엔드 전반에서 보게 될 핵심 타입:

| 타입 | 설명 |
|------|------|
| `Schema` / `SchemaRef` | 열의 순서 있는 목록. 각 열은 `Field`(이름, `DataType`, nullable 여부) |
| `Array` | 한 열의 값들을 담은 연속된 타입 버퍼(예: `Int64Array`, `StringArray`) |
| `RecordBatch` | 테이블의 가로 청크. `Schema` + 열당 `Array` 하나씩, 모두 같은 길이 |

**왜 컬럼나인가?** 먼저 *컬럼나(columnar)*란 표 형식 데이터를 행 단위가 아니라 **열(column) 단위로**
저장·표현하는 방식을 뜻합니다 — 한 행의 모든 필드를 한데 모으는 대신, 한 열의 모든 값을 연속해서
모아 둡니다. 분석 쿼리는 보통 많은 행에 걸쳐 일부 열만 다룹니다. 각 열을 연속 저장하면
쿼리가 필요로 하는 열만 스캔하고, 촘촘한 버퍼에 대해 벡터화 연산을 수행하며, 디스크의 Parquet
포맷과 곧바로 대응되는 레이아웃을 얻습니다([Parquet 포맷](./03-parquet-format.md) 참고).

이 앱에서 쿼리 결과는 단순히 `Vec<RecordBatch>`입니다.

## 2. Apache DataFusion: 임베디드 SQL 엔진

[DataFusion](https://datafusion.apache.org/)은 순수 Rust로 작성되고 Arrow 위에 구축된 완전한 SQL
쿼리 엔진입니다. **임베디드**라는 점이 핵심입니다: 우리 바이너리에 컴파일되어 들어갑니다. 별도의
DB 서버도, 외부 프로세스도, 네트워크 홉도 없습니다 — 독립 실행형 데스크톱 뷰어에 정확히 필요한
성질입니다.

쿼리 한 건의 파이프라인:

```
SQL 문자열
   │  SessionContext::sql_with_options(sql, read_only_options)
   ▼
LogicalPlan      ← 파싱 + 등록된 테이블 스키마에 대해 검증
   │  optimizer
   ▼
PhysicalPlan     ← 실행 가능한 연산자. Parquet 스캔으로 projection/predicate pushdown
   │  execute_stream()
   ▼
Stream<RecordBatch>   ← 배치를 필요에 따라 지연 생성
```

이 프로젝트는 `parquet` feature와 함께 **DataFusion 54**를 고정합니다(`src-tauri/Cargo.toml`).

### SessionContext 라이프사이클

`QueryEngine`(`src-tauri/src/engine/context.rs`)는 **열린 파일당 `SessionContext` 하나**를
소유합니다:

- 파일은 Parquet footer에서 스키마를 **추론**한 `ListingTable`을 통해 SQL 테이블 **`data`**로
  등록됩니다(`ListingTableConfig::infer`).
- 등록은 **build-then-swap** 패턴을 씁니다: 새 `SessionContext`를 지역 변수에 만들고, 실패할 수
  있는 모든 단계(스키마 추론, 테이블 등록, footer 읽기)가 성공한 뒤에만 `self.ctx`를 교체합니다.
  따라서 "열기"가 실패하면 이전에 열린 파일이 그대로 조회 가능 상태로 남습니다.
- 컨텍스트는 **쿼리마다 재생성되지 않습니다** — 새 파일을 열 때만 재생성됩니다.

스토리지는 `DataSource` 트레잇(`src-tauri/src/storage/mod.rs`) 뒤로 추상화되며,
`(ObjectStoreUrl, ObjectStore, ListingTableUrl)` 삼중쌍을 제공합니다. 엔진은 스토리지에
무관(storage-agnostic)합니다: 로컬 파일(`LocalFileSource`)과 원격 S3/MinIO(`RemoteS3Source`)가
동일한 코드 경로를 탑니다.

## 3. 스트리밍 실행과 100행 캡

The executor (`src-tauri/src/engine/executor.rs`) uses
`DataFrame::execute_stream()` rather than collecting the full query result.
It retains at most 100 rows, slicing the last batch when necessary.

- If a batch contains more rows than the remaining capacity, the executor
  retains only the slice that fits and reports `capped: true`.
- If the retained count reaches exactly 100, it checks for another non-empty
  batch. End of stream means `capped: false`; another non-empty batch means
  `capped: true`. Empty batches are skipped during this check.
- An error during that post-cap check conservatively reports `capped: true`
  and preserves the 100 rows already obtained. Earlier stream errors fail
  the query.

A complete result of exactly 100 rows, including the default `LIMIT 100`
query, is therefore not marked as truncated. `total_rows` is the retained
result count, not the total number of matching rows in the source.

This is a **result-row limit**, not a fixed memory, CPU, or time budget.
Projection and predicate pushdown can reduce work, but sorting, aggregation,
and other query plans may process much more input before returning results.
Execution cost depends on the data, query, and user's local hardware; the
app adds no automatic query timeout or fixed engine memory budget.

## 4. Arrow 데이터를 프론트엔드로 옮기기

직렬화 경로는 두 가지이며, 모두 `datafusion::arrow::*` re-export를 통해서만 접근합니다(직접
`arrow` 크레이트 사용 금지 — §6 참고):

- **Arrow IPC stream (query rows)** — `src-tauri/src/ipc/serializer.rs` uses
  `StreamWriter` to serialize retained batches. These bytes are the final
  segment of the framed `run_query` response. `decodeQueryFrame` extracts
  them before passing them to `tableFromIPC()`. The Arrow stream carries
  its own schema for decoding, while the frame's JSON result schema supplies
  the grid's column names and type labels, including for zero-row results.

- **Arrow JSON(페이징 경로)** — `get_page`는 슬라이스된 `RecordBatch`를 Arrow의 `ArrayWriter`로
  `serde_json::Value` 행 객체로 변환합니다. 이미 캡이 적용된 작은 캐시에만 사용됩니다.

See [Tauri & IPC §5](./01-tauri-and-ipc.md#5-single-response-framing) for the
single-response layout containing JSON metadata and Arrow IPC rows.

## 5. view 타입 함정 (Utf8View / BinaryView)

DataFusion 54는 문자열/바이너리 열을 `Utf8View` / `BinaryView`(Arrow 타입 id **24**)로 만들 수
있습니다. 이는 문자열 데이터를 위한 비교적 새로운 레이아웃입니다. 그런데 프론트엔드의
`apache-arrow` JS v21은 **타입 id 24를 디코딩하지 못하고** `"Unrecognized type: undefined (24)"`를
던지며, 그리드가 빈 채로 남습니다.

이 앱은 두 단계로 방어합니다:

1. **업그레이드 방지** — `SessionConfig`에서
   `execution.parquet.schema_force_view_types = false`로 설정(`context.rs`)하여, DataFusion이 평범한
   `Utf8` 열을 `Utf8View`로 업그레이드하지 않게 합니다.
2. **이중 안전 다운캐스트** — `normalize_view_types()`(`executor.rs`)가 직렬화 전에 네이티브
   view 타입 열을 `Utf8` / `Binary`로 캐스팅합니다. view 타입이 없으면 무할당 fast path로 통과합니다.

이는 프론트엔드 디코더의 한계가 백엔드 제약으로 이어진 구체적 사례입니다 — 두 의존성 중 하나를
업그레이드할 때 기억해 둘 만합니다.

## 6. 단일 버전 의존성 규율

DataFusion은 `arrow`, `parquet`, `sqlparser` 크레이트를 **re-export**합니다. 이 프로젝트는 이들을
직접 의존성으로 추가하지 **않습니다**. 모든 접근은 re-export 경로를 거칩니다:

| 용도 | 경로 |
|------|------|
| Arrow 타입 | `datafusion::arrow::*` |
| Parquet 리더/메타데이터 | `datafusion::parquet::*` |
| SQL AST 파서 | `datafusion::sql::parser` / `datafusion::sql::sqlparser` |

**이유:** `arrow`를 직접 추가하면 *두 번째, 서로 다른* 버전이 빌드에 끌려 들어올 위험이 있습니다.
호환되지 않는 두 `arrow` 버전은 혼란스러운 타입 불일치 오류를 냅니다(`arrow@X::RecordBatch`는
`arrow@Y::RecordBatch`가 아닙니다). 모든 것을 DataFusion의 re-export로 통일하면 각 크레이트가 정확히
한 버전임을 보장합니다. 그래서 `Cargo.toml`에는 `datafusion`만 있고 `arrow`나 `parquet`은 없습니다.
