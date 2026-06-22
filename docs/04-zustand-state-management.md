# Zustand 상태 관리

프론트엔드의 단일 진실 공급원(single source of truth)인 Zustand 스토어를 설명합니다.

---

## 1. Zustand란

[Zustand](https://github.com/pmndrs/zustand)는 React를 위한 작고 가벼운 상태 관리 라이브러리입니다.
Redux 같은 보일러플레이트(액션 타입, 리듀서, 디스패처, Provider 래핑)가 없습니다. 핵심 아이디어는
단순합니다:

- `create()`로 **스토어**(상태 + 그 상태를 바꾸는 액션)를 하나 만든다.
- 컴포넌트는 **훅**으로 스토어를 구독하고, 자신이 쓰는 조각만 **selector**로 고른다.
- selector가 고른 값이 바뀔 때만 그 컴포넌트가 리렌더링된다.

Context Provider로 트리를 감쌀 필요가 없습니다 — 스토어는 모듈 수준의 싱글턴이고, 어디서든
훅을 import해서 씁니다.

## 2. 이 앱에서 Zustand의 역할

이 앱의 UI 상태는 전부 하나의 스토어 `useAppStore`(`src/store/appStore.ts`)에 모여 있습니다.
이 스토어는 다음을 구동합니다:

- 어떤 화면을 보여줄지 (welcome 화면 vs 결과 화면)
- 파일 열기/등록 라이프사이클과 그에 따른 차단 오버레이
- SQL 에디터 텍스트, 로딩 상태, 인라인 쿼리 오류
- 쿼리 결과(행, 총 행 수, 캡 여부)
- 사이드바와 파일 메타데이터
- 원격 연결 폼의 세션 내 자동완성 메모리

백엔드(Rust)가 진짜 데이터(파일, 쿼리 엔진)의 주인이라면, 이 스토어는 **UI 상태의 주인**입니다.
[Tauri & IPC](./01-tauri-and-ipc.md)의 `invoke` 호출 결과가 이 스토어로 흘러 들어와 화면을 바꿉니다.

## 3. 스토어 구조

스토어는 **상태 필드 + 액션 함수**가 한 객체에 함께 들어 있습니다(`appStore.ts`, 요약):

```ts
import { create } from "zustand";

export const useAppStore = create<AppState>((set) => ({
  // ── 상태 ──
  filePath: null,             // null이면 welcome 화면, 값이 있으면 결과 화면
  schema: [],
  queryText: "select * from data limit 100",
  isLoading: false,           // 쿼리 전용 로딩 (파일 열기에 재사용하지 않음)
  queryError: null,
  rows: [],
  totalRows: 0,
  capped: false,
  sidebarCollapsed: false,
  fileMetadata: null,
  registrationStatus: "idle", // 파일 열기/등록 라이프사이클의 단일 진실 공급원
  registrationError: null,
  openSeq: 0,                 // 단조 증가 "열기 토큰" (오래된 비동기 작업 무효화)
  lastRemoteConnection: null, // 세션 한정 자동완성 메모리

  // ── 액션 ──
  setFile: (path, schema) => set((state) => ({ /* ... */ })),
  setQueryText: (sql) => set({ queryText: sql, queryError: null }),
  setResults: (totalRows, capped, rows) => set({ rows, totalRows, capped, isLoading: false }),
  reset: () => set((state) => ({ /* 초기값으로 되돌림 */ })),
  // ...
}));
```

`set`은 부분 업데이트를 병합합니다 — 넘긴 키만 바뀌고 나머지는 유지됩니다. 이전 상태가 필요하면
`set((state) => ({ ... }))` 형태의 함수형 업데이트를 씁니다.

## 4. 상태 설계의 핵심 결정

코드 주석에 박혀 있는, 이 스토어를 이해하는 데 중요한 규칙들입니다.

### `filePath`가 화면을 가른다

`filePath === null`이면 welcome 화면(화면 A), 값이 있으면 결과 화면(화면 B)입니다. 별도의
"현재 화면" enum을 두지 않고, 가장 의미 있는 상태 하나로부터 화면을 파생합니다.

### `isLoading`은 쿼리 전용 — 파일 열기에 재사용하지 않는다

로딩 상태가 두 가지 성격(쿼리 실행 중 vs 파일 등록 중)을 한 플래그에 욱여넣으면 서로 간섭합니다.
그래서 파일 열기 라이프사이클은 **별도의** `registrationStatus`로 표현합니다:

```ts
type RegistrationStatus = "idle" | "registering" | "registered" | "error";
```

- 차단 오버레이, 쿼리 실행 차단(gating), 등록 오류 표시를 모두 이 한 필드가 구동합니다.
- `setFile`은 `openFile`이 성공한 뒤에만 호출되므로, 거기 도달했다는 것은 등록 성공을 의미합니다 →
  `registrationStatus`를 `"registered"`로 set해 오버레이를 내립니다.

### `openSeq`: 단조 증가 "열기 토큰"

`setFile`과 `reset`은 `openSeq`를 1 증가시킵니다. 파일 열기 후 비동기 후속 작업(예:
`getFileMetadata`)이 끝났을 때, 그 사이 더 새로운 열기가 일어났는지 이 토큰으로 감지합니다.
같은 경로를 다시 여는 경우 `filePath` 동등 비교로는 구분할 수 없기 때문에, 이런 단조 토큰이
필요합니다(오래된 응답이 새 상태를 덮어쓰는 race 방지).

### `lastRemoteConnection`은 의도적으로 reset/setFile에서 살아남는다

원격 연결 폼의 세션 내 자동완성 메모리입니다. 파일 상태가 아니므로 `reset()`과 `setFile()`에서
**일부러 지우지 않습니다** — 지우면 자동완성 불변식이 깨집니다. 앱을 다시 실행할 때만
사라집니다(인메모리 Zustand 스토어, 영속화 없음).

> 보안 메모: 이 값은 인메모리 스토어에만 저장되며, 디스크·localStorage·sessionStorage에 절대
> 기록되지 않습니다(자격 증명을 디스크에 남기지 않음).

### 오류 상태를 언제 비우는가

- 쿼리를 편집하면(`setQueryText`) `queryError`가 함께 비워집니다 — 사용자가 능동적으로 고치고
  있다는 신호이기 때문입니다.
- 새 파일을 열면(`setFile`) 오류·결과·메타데이터가 깨끗이 초기화됩니다.
- `reset()`은 모든 일시적 상태를 초기값으로 되돌립니다.

## 5. 컴포넌트에서 스토어 소비하기 — selector 패턴

컴포넌트는 필요한 조각만 selector로 골라야 불필요한 리렌더링을 피합니다. 여러 값을 한 번에 고를
때는 `useShallow`로 얕은 비교를 적용합니다. 실제 예시(`src/hooks/useRunQuery.ts`):

```ts
import { useShallow } from "zustand/react/shallow";
import { useAppStore } from "../store/appStore";

const {
  queryText, isLoading, setLoading, setResults, setQueryError, registrationStatus,
} = useAppStore(
  useShallow((s) => ({
    queryText: s.queryText,
    isLoading: s.isLoading,
    setLoading: s.setLoading,
    setResults: s.setResults,
    setQueryError: s.setQueryError,
    registrationStatus: s.registrationStatus,
  }))
);
```

`useShallow`가 없으면, selector가 매번 새 객체를 반환하므로 스토어의 *어떤* 변화에도 리렌더링이
발생합니다. `useShallow`는 객체의 각 필드를 얕게 비교해, 고른 값이 실제로 바뀔 때만 리렌더링하게
합니다.

## 6. 쿼리 실행에서 상태가 흐르는 모습

`useRunQuery` 훅은 스토어와 IPC가 어떻게 맞물리는지 보여주는 좋은 예입니다:

```ts
const runQueryHandler = useCallback(async () => {
  // gating: 파일이 등록되어 있고, 진행 중인 쿼리가 없을 때만 동작
  if (isLoading || registrationStatus !== "registered") return;

  setLoading(true);                       // 상태: 로딩 시작
  try {
    const result = await runQuery(queryText); // IPC 왕복 (01번 문서 참고)
    setQueryError(null);                  // 상태: 이전 오류 제거
    setResults(result.total_rows, result.capped, result.rows); // 상태: 결과 반영
  } catch (err) {
    setQueryError(err instanceof Error ? err.message : String(err)); // 상태: 인라인 오류
  } finally {
    setLoading(false);                    // 성공/실패와 무관하게 항상 로딩 해제
  }
}, [queryText, isLoading, registrationStatus, setLoading, setResults, setQueryError]);
```

핵심 포인트:

- **Gating은 상태로 강제됩니다.** `registrationStatus !== "registered"`거나 이미 로딩 중이면
  핸들러가 no-op입니다. 이는 `Ctrl+Enter` 단축키가 비활성화된 버튼을 우회해 등록되지 않은
  테이블을 조회하는 것을 막습니다.
- **`finally`로 로딩 누수를 차단합니다.** 결과 반영이나 디코딩이 던지더라도 `isLoading`이 항상
  해제되므로, "로딩에서 멈춘" 상태가 구조적으로 발생하지 않습니다.

## 7. 정리: 이 설계가 주는 것

- **단일 진실 공급원** — UI 상태가 한 스토어에 모여 있어, 화면·오버레이·오류가 같은 데이터에서
  파생됩니다.
- **의도가 드러나는 상태 분리** — 쿼리 로딩(`isLoading`)과 파일 등록(`registrationStatus`)을
  나눠, 한 플래그를 과적재했을 때 생기는 간섭을 없앱니다.
- **race에 강함** — `openSeq` 토큰으로 오래된 비동기 응답이 새 상태를 덮어쓰지 못하게 합니다.
- **선택적 구독** — selector + `useShallow`로 필요한 변화에만 리렌더링합니다.
