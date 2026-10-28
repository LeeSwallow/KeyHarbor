# 기능별 모듈화와 보안 계층 분리 (리팩터링) 계획

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. Steps use `- [ ]`.

**Goal:** 동작을 바꾸지 않고 `rebuild/` 코드를 기능별 모듈로 나누고, 평문을 다루는 모든 로직을 새 보안 계층 crate `keyharbor-guard`로 모아, 계층 규칙을 자동 검사로 강제한다. [사용자 요구 R-30: "제대로 기능별로 모듈화해서 구현해, 보안 계층도 나누고"]

**Architecture (목표 의존 방향, 위가 아래에만 의존):**
```
keyharbor (cli)      keyharbor-mcp
        \              /
       keyharbor-broker          브라우저·감독자·CDP (평문을 직접 만들지 않음)
              |
       keyharbor-guard           보안 계층: policy · origin · substitute · mask · audit
              |
       keyharbor-core            금고 유스케이스 (resolve는 guard 전용)
              |
   keyharbor-vault  keyharbor-sqlite    암호 레코드·키 / 저장소
              \      /
          keyharbor-domain       타입·규칙 (I/O 없음)
```

## Global Constraints
- **동작 변경 금지.** 공개 CLI 명령·MCP 도구 이름·응답 형식·DB 스키마·암호문 형식(KAT) 그대로. 모든 기존 테스트(125 통과 + 1 ignored)가 그대로 통과해야 한다(테스트 파일 이동·모듈 경로 수정은 허용, 단언 변경 금지).
- 1·3단계 Global Constraints 유지(PATH, 허용 명령, Chrome 테스트 env, 키체인 금지, 평문 금지, 커밋 규칙).
- 한 파일 400줄 이하를 목표로 하고(테스트 모듈 제외), 넘으면 기능 단위로 더 나눈다.

## 목표 모듈 구조
- `keyharbor-guard` (신규, `crates/guard`)
  - `policy/` — 판정(`decide`), 규칙 타입 사용, 정책 저장소 연동 (core/src/policy.rs에서 이동)
  - `origin.rs` — 입력 대상 frame origin 판정 규칙(CDP와 독립: `trait FrameInspector { fn focused_frame_origin(&self, target) -> Result<Origin> }`를 broker가 구현)
  - `substitute.rs` — `fn prepare_text(ctx: &GuardContext, inspector: &dyn FrameInspector, text: &str) -> Result<Zeroizing<String>, Denied>`: placeholder 파싱 → 정책 → origin 바인딩 → `core` resolve → 사용 기록
  - `mask.rs` — `Masker`(core/src/mask.rs에서 이동), `fn redact_json(&self, Value) -> Value`
  - `audit.rs` — 사용 기록 기록기(core/src/usage.rs의 기록 부분 이동), `audit_enabled` 존중
  - `run.rs` — `keyharbor run`의 정책·치환·마스킹 로직(cli/src/run_cmd.rs에서 프로세스 실행 외 부분 이동)
  - `lib.rs` — `GuardContext { vault: Arc<Mutex<Option<Vault>>>, agent, session, store }`와 공개 API만
- `keyharbor-core`: `lib.rs`를 `vault/{mod.rs, open.rs(init/open/recovery), items.rs(add/edit/remove), summary.rs(list/ItemSummary), resolve.rs}` 등으로 나눈다. `rotate.rs`, `transfer.rs`, `dotenv.rs` 유지. **`resolve`와 `masker`(평문 접근)는 `#[doc(hidden)] pub fn … ` + 이름에 `_for_guard` 접미사**로 바꿔 호출처를 검사로 잡을 수 있게 한다(예: `resolve_for_guard`, `masking_values_for_guard`). export(age)는 core 내부라 예외.
- `keyharbor-broker`: `cdp/{mod.rs(client), protocol.rs}`, `browser/{navigate.rs, snapshot.rs, input.rs(click/fill/type/press), tabs.rs, screenshot.rs, wait.rs, frames.rs(FrameInspector 구현)}`, `ops.rs`(op 분배만, 텍스트 입력은 `guard::substitute`, 응답은 `guard::mask`), `supervisor/{mod.rs, launch.rs, lock.rs}`, `control.rs`, `lifecycle.rs`, `paths.rs`.
- `keyharbor-mcp`: `server.rs`(rmcp 설정·scope), `tools/{session.rs, browser.rs, vault.rs}`, `scope.rs`(--session 잠금).
- `keyharbor` (cli): 그대로 두되 `run_cmd.rs`는 프로세스 실행·출력 중계만.

## 계층 검사 (`scripts/check-layers.sh`, CI repo-checks에 추가)
1. 각 crate `Cargo.toml`의 `[dependencies]`(dev 제외)가 허용 표를 벗어나면 실패:
   domain: (없음) · vault: domain · sqlite: domain · core: domain, vault, sqlite · guard: domain, vault, sqlite, core · broker: domain, sqlite, core, guard, vault · mcp: domain, sqlite, core, guard, broker · cli: 전부.
2. `_for_guard` 함수 호출이 `crates/guard/` 밖(테스트 제외: `#[cfg(test)]`·`tests/`)에 있으면 실패.
3. `chacha20poly1305`·`hkdf`·`hmac` 직접 사용은 `crates/vault/`에서만.
- [ ] 검사 스크립트를 먼저 쓰고 **현재 코드에서 실패하는 것**을 확인(guard 없음, resolve 직접 호출) → 리팩터링 후 통과.

## Tasks
1. `scripts/check-layers.sh` + `scripts/test-scripts.sh`에 연결(지금은 실패 예상 — 이 커밋에서는 test-scripts에서 `check-layers`를 아직 호출하지 않고, Task 5에서 연결) → 커밋 `chore: layer rules check`
2. `keyharbor-guard` 생성: policy·mask·audit·substitute·origin·run 로직 이동, core의 평문 API 이름 변경(`_for_guard`), broker·cli·mcp가 guard를 거치게 수정 → 전체 테스트 통과 → 커밋 `refactor: move policy, substitution, masking and audit into keyharbor-guard`
3. broker를 cdp/·browser/·supervisor/로 분할, ops.rs는 분배만 → 테스트 통과 → 커밋 `refactor(broker): split CDP, browser operations and supervisor into modules`
4. core 금고 facade 분할, mcp tools 분할 → 테스트 통과 → 커밋 `refactor: split the vault facade and MCP tools by feature`
5. check-layers를 test-scripts와 CI에 연결, 통과 확인; docs/design/02 §2.4(컴포넌트 다이어그램·crate 표 갱신), 새 `docs/design/adr/0014-guard-security-layer.md`, 06에 R-30·J 기록, rebuild/README crate 목록 → 전체 검증(fmt·clippy·test with Chrome·check-docs·test-scripts·deny·check-release-features) → 커밋 `docs: guard security layer and module map`
