# 헥사고날 아키텍처로 모듈화와 보안 계층(PDP/PEP) 분리 — 리팩터링 계획

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. Steps use `- [ ]`.

**Goal:** 동작을 바꾸지 않고 `rebuild/`를 **헥사고날(Ports & Adapters) + Clean Architecture 의존 규칙**으로 재구성하고, 평문을 다루는 모든 로직을 **보안 계층 `keyharbor-guard`(PDP = policy, PEP = substitute·mask)**로 모은다. 브라우저 접근은 **데코레이터 `GuardedBrowser`**를 거쳐야만 인터페이스에 노출되고, 평문 복호화는 guard만 만들 수 있는 **capability `PlaintextAccess`**를 요구한다. [사용자 요구 R-30, R-31, R-32]

## 계층과 의존 방향 (위 → 아래만)

```
interfaces (composition root)   keyharbor (cli) · keyharbor-mcp · (5단계 app)
adapters                         keyharbor-broker (CDP·supervisor: BrowserPort·FrameInspector 구현)
                                 keyharbor-sqlite (저장소 포트 구현)
                                 keyharbor-vault  (암호 레코드·KeyStore·Keychain)
security (PDP/PEP)               keyharbor-guard  (policy · substitute · mask · origin · audit · run)
application (use cases)          keyharbor-core   (vault · session · handoff · rotate · transfer) — 포트만 사용
domain + ports                   keyharbor-domain (엔티티·값 객체·규칙 · ports 모듈의 trait)
```

허용 의존(정상 dependencies, dev 제외):
| crate | 의존 가능 |
|---|---|
| domain | (없음) |
| vault | domain |
| core | domain, vault(암호 레코드 함수만: seal/open/wrap/meta_mac — I/O 없음) |
| guard | domain, core |
| sqlite | domain |
| broker | domain, guard |
| mcp | domain, core, guard, broker, sqlite, vault(KeyStore 생성만) |
| cli | 전부 |

## 포트 (domain::ports)

- `VaultStore` — 현재 `SqliteStore`의 vault_* / item_* / domain_keys / rekey / replace_domain / vault_version 메서드 집합 (행 타입은 domain으로 이동하거나 ports 모듈에 정의)
- `SessionStore` — 세션·generation·인계 메서드
- `PolicyStore`, `AuditStore`, `SettingsStore`
- `KeyStore` — 현재 `keyharbor_vault::keystore::KeyStore` trait을 domain::ports로 이동(구현 MacKeychain·FileKeyStore는 vault에 남음)
- `BrowserPort` — navigate/snapshot/screenshot/click/fill/type/press/tabs/new_tab/switch_tab/wait/close/status (응답은 `serde_json::Value` 또는 타입). 구현은 broker의 CDP 어댑터.
- `FrameInspector` — `focused_frame_origin(&self, target) -> Result<Origin>` (broker 구현)

core는 `SqliteStore` 같은 구체 타입이 아니라 위 trait에만 의존한다(제네릭 또는 `Box<dyn …>` — 구현자가 테스트 편의와 단순성으로 선택). sqlite는 domain::ports를 구현한다(dependency inversion).

## 영속성 계층 (keyharbor-sqlite = Diesel 어댑터) [R-32]

- `rusqlite`를 rebuild 워크스페이스에서 제거하고 **Diesel 2.3**(`sqlite`, `returning_clauses_for_sqlite_3_35`)과 `diesel_migrations` 2.3, `libsqlite3-sys`(`bundled`, Diesel이 허용하는 <0.39 범위)로 바꾼다. 한 워크스페이스에 libsqlite3-sys는 하나만 링크될 수 있으므로 테스트도 rusqlite를 쓰지 않는다(변조 테스트는 Diesel `sql_query` 또는 repo의 `#[cfg(test)]` 헬퍼).
- 구조: `migrations/<버전>_<이름>/up.sql`(현재 스키마를 그대로 옮긴 초기 마이그레이션 + 이후 변경), `schema.rs`(diesel `table!`), `models/`(Queryable·Insertable·AsChangeset 영속성 모델 — domain 타입과 분리), `mappers/`(영속성 ↔ domain 변환), `repos/{vault, session, handoff, policy, audit, settings}.rs`(domain::ports 구현), `connection.rs`(열기: 0700 디렉터리 검사, WAL, busy_timeout 5s, foreign_keys, secure_delete, 마이그레이션 실행).
- 쓰기 트랜잭션은 `conn.immediate_transaction(|c| …)`, CAS는 `diesel::update(…).filter(generation.eq(…))` 영향 행 수로 판정. `vault.version` 증가와 `bump_from(expected)` 의미 유지.
- 원시 SQL은 `migrations/`에만. `check-layers.sh`가 `crates/sqlite/src/repos/` 밖의 `sql_query(` 사용과, rebuild 전체의 `rusqlite` 의존을 금지한다.
- DB 스키마(테이블·컬럼·CHECK 제약)는 바꾸지 않는다. 기존 사용자 DB(테이블이 이미 있음)를 열 때 초기 마이그레이션이 `CREATE TABLE IF NOT EXISTS`라 충돌하지 않게 한다.
- `cargo deny`: Diesel·diesel_migrations 라이선스(MIT OR Apache-2.0) 확인.

## 보안 계층 (keyharbor-guard)

- `pdp` (policy): `decide(rules, agent, action) -> Decision` — core/src/policy.rs에서 이동.
- `pep::substitute`: `prepare_text(&GuardContext, &dyn FrameInspector, &str) -> Result<Zeroizing<String>, Denied>`.
- `pep::mask`: `Masker`, `redact_json`.
- `pep::origin`: frame origin 판정 규칙.
- `audit`: 사용 기록(`audit_enabled` 존중).
- `run`: `keyharbor run`의 정책·치환·마스킹(프로세스 실행은 cli).
- `GuardedBrowser<B: BrowserPort>`: `BrowserPort`를 구현하는 데코레이터. fill/type 입력은 `prepare_text`, 모든 응답은 `redact`. **mcp·cli는 `GuardedBrowser`만 받는다**(broker는 raw 어댑터를 crate 밖에 `pub`으로 노출하지 않고, 조립 함수 `broker::guarded(...) -> GuardedBrowser<…>`만 제공하거나, 감독자 내부에서 GuardedBrowser로 감싼 뒤 control op에 연결).
- **Capability**: core의 평문 API(`resolve`, 마스킹 값 목록)는 `&PlaintextAccess` 인자를 요구한다. `PlaintextAccess`는 guard crate에 정의된 private 생성자를 가진 타입이어야 하지만 core가 guard를 알 수 없으므로, **core에 `pub struct PlaintextAccess(())`를 두고 생성자 `PlaintextAccess::issue()`를 `#[doc(hidden)] pub`으로 두되 `scripts/check-layers.sh`가 `PlaintextAccess::issue` 호출을 `crates/guard/`와 core 내부(transfer export) 및 테스트 외에서 금지**한다. (Rust에서 crate 간 sealed 생성자의 가장 단순한 근사.)

## 모듈 분할 (한 파일 400줄 이하 목표, 테스트 모듈 제외)

- core: `vault/{mod, open, items, summary, resolve}`, `session/{mod, lifecycle, handoff}`(broker/lifecycle.rs의 순수 유스케이스 부분 이동 — 프로세스 띄우기는 broker), `rotate`, `transfer`, `dotenv`.
- broker: `cdp/{client, protocol}`, `browser/{navigate, snapshot, input, tabs, screenshot, wait, frames}`(BrowserPort·FrameInspector 구현), `supervisor/{mod, launch, lock}`, `control`, `paths`, `ops`(control op → GuardedBrowser 분배만).
- mcp: `server`, `scope`, `tools/{session, browser, vault}`.
- guard: 위 구조.

## Global Constraints
- **동작 변경 금지**: CLI 명령·MCP 도구 이름·응답 형식·DB 스키마·암호문 형식(KAT) 그대로. 모든 기존 테스트(125 + 1 ignored) 통과. 테스트 이동·경로 수정은 허용, 단언 변경 금지.
- 1·3단계 제약 유지(PATH, 허용 명령, Chrome env로 실제 브라우저 테스트 실행, 키체인 금지, 평문 금지, Conventional Commits, AI trailer 금지).

## Tasks
1. `scripts/check-layers.sh`(허용 의존 표, `PlaintextAccess::issue` 호출 위치, `chacha20poly1305|hkdf|hmac` 직접 사용은 vault만) 작성 → 현재 코드에서 실패 확인 → 커밋 `chore: layer rules check`(test-scripts 연결은 Task 6)
2. domain::ports 도입, sqlite가 포트 구현, core가 포트에만 의존 → 테스트 통과 → 커밋 `refactor: ports in domain, adapters implement them`
2b. sqlite 어댑터를 Diesel로 교체(migrations·schema·models·mappers·repos·connection) → 테스트 통과(동시 CAS 경합 테스트 포함) → 커밋 `refactor(sqlite): Diesel repositories with embedded migrations`
3. keyharbor-guard 생성(pdp·pep·audit·run 이동, PlaintextAccess, GuardedBrowser 데코레이터) → 커밋 `refactor: guard security layer with policy decision and enforcement points`
4. broker 분할(cdp/·browser/·supervisor/), BrowserPort·FrameInspector 구현, control op는 GuardedBrowser 경유 → 커밋 `refactor(broker): CDP adapter, browser operations and supervisor modules`
5. core·mcp 모듈 분할 → 커밋 `refactor: split use cases and MCP tools by feature`
6. check-layers를 test-scripts·CI repo-checks에 연결; docs/design/02 §2.1·§2.4 갱신(헥사고날·PDP/PEP·데코레이터 다이어그램), `docs/design/adr/0014-hexagonal-guard-layer.md`, 06에 R-30·R-31과 J 기록, rebuild/README crate 지도 → 전체 검증(fmt·clippy·test with Chrome·check-docs·test-scripts·check-layers·deny·check-release-features) → 커밋 `docs: hexagonal architecture and guard security layer`
