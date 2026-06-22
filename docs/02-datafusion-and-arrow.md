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
   │  SessionContext::sql(sql)
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

이것이 앱의 성능적 심장입니다. 실행기(`src-tauri/src/engine/executor.rs`)는 **절대 `.collect()`를
호출하지 않습니다** — collect는 결과 전체를 메모리에 구체화하므로, GB급 파일에서는 목적을
무너뜨립니다.

대신 `DataFrame::execute_stream()`을 사용해 배치를 100행이 될 때까지 끌어오고, 그 시점에 멈춥니다:

```rust
const ROW_CAP: usize = 100;

let mut stream = df.execute_stream().await?;
'drain: while let Some(batch) = stream.next().await {
    let batch = batch?;
    let remaining = ROW_CAP.saturating_sub(total_rows);
    if batch.num_rows() <= remaining {
        total_rows += batch.num_rows();
        retained_batches.push(batch);
    } else {
        retained_batches.push(batch.slice(0, remaining)); // 마지막 배치를 잘라 정확히 100행
        capped = true;
        break 'drain;
    }
    if total_rows >= ROW_CAP { capped = true; break 'drain; }
}
```

실행이 지연(lazy)되고 스캔으로 pushdown되므로, 일찍 멈춘다는 것은 엔진이 그 앞쪽 100행을
만들기 위해 필요한 row group과 열만 읽는다는 뜻입니다. `capped` 플래그는 캡을 넘어 더 많은 행이
있었는지를 UI에 알려줍니다.

## 4. Arrow 데이터를 프론트엔드로 옮기기

직렬화 경로는 두 가지이며, 모두 `datafusion::arrow::*` re-export를 통해서만 접근합니다(직접
`arrow` 크레이트 사용 금지 — §6 참고):

- **Arrow IPC 스트림(행 데이터 경로)** — `src-tauri/src/ipc/serializer.rs`가 `StreamWriter`로
  `&[RecordBatch]`를 Arrow IPC 스트림 바이트로 변환합니다. 이 바이트는 바이너리 IPC 채널로
  반환되며 프론트엔드에서 `apache-arrow`의 `tableFromIPC()`로 디코딩됩니다. 포맷이
  자기 기술적(self-describing)이라 스키마를 함께 담으므로, 프론트엔드는 추가 메타데이터 없이
  정확한 컬럼나 테이블을 복원합니다.

- **Arrow JSON(페이징 경로)** — `get_page`는 슬라이스된 `RecordBatch`를 Arrow의 `ArrayWriter`로
  `serde_json::Value` 행 객체로 변환합니다. 이미 캡이 적용된 작은 캐시에만 사용됩니다.

행 데이터가 바이너리 채널을, 메타데이터가 JSON을 쓰는 이유는
[Tauri & IPC §5](./01-tauri-and-ipc.md#5-2채널-직렬화-전략)를 참고하세요.

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
