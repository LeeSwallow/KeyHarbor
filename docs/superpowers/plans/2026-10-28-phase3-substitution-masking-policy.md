# 3단계 치환·마스킹·정책·사용 기록 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (or subagent-driven-development). Steps use checkbox (`- [ ]`) syntax.

**Goal:** 에이전트가 `{{secret:item.field}}` placeholder로 금고의 비밀을 브라우저 입력과 자식 프로세스 환경 변수에 **값을 보지 않고** 쓰게 하고, 에이전트로 가는 모든 응답에서 비밀을 가리며, 사용자 정의 정책과 값 없는 사용 기록을 적용한다.

**Architecture:** 1단계의 `keyharbor-broker::ops`가 입력 텍스트(`prepare_text`)와 응답(`redact`)의 유일한 통로다. 여기에 (1) placeholder 파서(이미 `keyharbor_domain::vault::parse_placeholders`), (2) 정책 판정(`keyharbor-core::policy`), (3) 입력 직전 frame origin 확인, (4) Aho-Corasick 마스커(`keyharbor-core::mask`), (5) 사용 기록(`keyharbor-sqlite` audit 테이블)을 끼운다. `keyharbor run`은 같은 정책·마스커를 자식 프로세스 출력에 적용한다. 기준 설계: docs/design/02 §2.5.2·§2.5.3·§2.5.5·§2.8, 03 §3.7·§3.8, ADR-0010.

**Tech Stack:** 기존 + `aho-corasick` 1(MIT/Unlicense — deny 허용 목록에 `Unlicense` 추가 필요 시 사유 기록).

## Global Constraints

- 1단계 Global Constraints 전부 유지(PATH, 허용 명령, 키체인 금지, Chrome 테스트 env).
- 평문은 `keyharbor-core::Vault::resolve`와 감독자 메모리에서만 존재한다. MCP 응답·CLI 출력·로그·오류 메시지·DB에 평문 금지. 오류 메시지는 항목 이름과 이유 코드만.
- 정책 판정 규칙: 02 §2.8 그대로 — 에이전트 규칙이 `*` 규칙보다, 항목 target이 도메인 target이, 도메인 target이 `*`보다 우선; 같은 단계면 deny 우선; 일치 규칙 없으면 **허용**.
- origin 바인딩: 항목의 `origin_binding`이 true면 대상 요소가 속한 **frame**의 origin이 항목 origin 목록에 정확히 일치해야 한다(02 §2.7.2, 03 §3.8). 입력 후 frame origin을 다시 확인하고 바뀌었으면 `origin_changed_after_fill` 경고를 기록한다.
- 마스킹: 03 §3.7 표 — 대상은 금고 전체 필드 값, 4자 미만 제외(CVC는 4자리만), 원문·percent-encoding·HTML 엔티티 변형, 카드번호는 숫자만·4자리 공백·4자리 하이픈 변형, leftmost-longest, `vault_version` 카운터가 바뀌면 오토마톤 재구성, 감독자는 첫 브라우저 응답 직전에 적재.
- 사용 기록 이벤트 종류: `secret_used`, `secret_denied`, `origin_changed_after_fill`, `session_opened`, `session_stopped`, `handoff_requested`, `handoff_resumed`, `policy_changed`, `vault_item_changed`, `env_injected`. 컬럼: `seq, at, agent, session_id, kind, item_name, origin, result` — **값 컬럼 없음**. 설정 `audit_enabled`(기본 true)가 false면 아무것도 기록하지 않는다(J-23).

## Review Focus

1. **피싱 origin**: `login` 항목(origin `https://github.com`)을 `http://127.0.0.1:<port>` 페이지에 넣으려 하면 거부되고 `secret_denied`가 기록되며, 페이지 입력란은 비어 있어야 한다. → Task 3.
2. **교차 출처 iframe**: 상위 문서는 허용 origin이지만 입력란이 다른 origin iframe 안에 있으면 거부. → Task 3.
3. **비밀이 페이지에 다시 나타남**: 입력한 값이 페이지 텍스트로 표시돼도 snapshot·오류 응답에는 placeholder로 가려진다. 카드번호를 공백 형식으로 보여 줘도 가린다. → Task 4.
4. **`keyharbor run`의 자식이 변수를 출력**: stdout·stderr에서 가려지고 종료 코드는 그대로 전달. 정책 거부면 자식을 실행하지 않는다. → Task 5.
5. **정책 판정 경계**: 02 §2.8의 모든 분기(구체성 순서, deny 우선, 기본 허용, 도메인 규칙). → Task 1.

---

## Task 1: 정책 (`keyharbor-core::policy`, sqlite)
- 테이블 `policy_rules(agent TEXT, resource TEXT CHECK(resource IN ('session','item','domain','create_session','raw_cdp')), target TEXT, effect TEXT CHECK(effect IN ('allow','deny')), PRIMARY KEY(agent,resource,target))`.
- `enum Action { OpenSession(SessionId), CreateSession, UseItem{ item: ItemName, domain: Domain }, RawCdp }`, `fn decide(rules: &[Rule], agent: &str, action: &Action) -> Decision { Allow, Deny{ rule: String } }`.
- CLI `keyharbor policy list|allow|deny|rm --agent A --resource R --target T`(정책 변경은 `policy_changed` 기록).
- [ ] 판정 단위 테스트(Review Focus 5) → 구현 → 커밋 `feat(core): agent policy rules and decisions`

## Task 2: 사용 기록 (`keyharbor-sqlite` audit, core API)
- 테이블 `audit(seq INTEGER PRIMARY KEY, at, agent, session_id, kind, item_name, origin, result)`, 설정 테이블 `settings(key PRIMARY KEY, value)` with `audit_enabled`.
- `fn record(&self, ev: AuditEvent)`, `fn list_audit(filter) -> Vec<AuditEvent>`; CLI `keyharbor audit [--limit N] [--json]`, `keyharbor settings set audit_enabled false`.
- 기존 금고 쓰기(add/edit/rm)와 세션 open/stop/handoff에 기록을 연결.
- [ ] 테스트: 이벤트 기록·조회, 꺼짐 시 미기록, 스키마에 값 컬럼이 없음(`PRAGMA table_info` 검사) → 커밋 `feat: value-free audit log`

## Task 3: 치환 입력과 origin 확인 (`keyharbor-broker::ops`)
- `prepare_text`: placeholder가 없으면 그대로. 있으면 각 ref에 대해 정책 `UseItem` 판정 → 대상 요소의 frame origin 조회(DOM.describeNode → frameId → Page.getFrameTree) → `Vault::item_origins_verified`로 바인딩 확인 → `Vault::resolve` → 치환한 `Zeroizing<String>`. 하나라도 실패하면 입력하지 않고 `{"ok":false,"error":"denied: <item> (<reason>)"}`.
- 감독자는 `Vault::open`을 세션 시작 시가 아니라 첫 치환 또는 첫 마스킹 때 한다(키체인 접근 최소화). 테스트는 `KEYHARBOR_TEST_KEYSTORE_DIR`.
- 감독자가 에이전트 이름을 알도록: MCP 서버가 control 요청에 `agent` 필드를 붙인다(CLI 직접 호출은 `"cli"`).
- [ ] 실제 Chrome 테스트: 로컬 테스트 서버의 origin을 항목 origin으로 등록한 login 항목 → `fill #pw {{secret:test.password}}` 후 Runtime.evaluate로 입력값 길이만 확인(값 비교는 테스트 프로세스 안에서만) → 응답에 값 없음; Review Focus 1·2(두 번째 포트로 다른 origin iframe) → 커밋 `feat(broker): placeholder substitution with policy and frame-origin checks`

## Task 4: 응답 마스킹 (`keyharbor-core::mask`, ops.redact)
- `struct Masker { automaton, version }`, `fn build(values: impl Iterator<Item=(Placeholder, Zeroizing<String>)>) -> Masker`, `fn redact(&self, s: &str) -> String`. 변형 생성 규칙은 Global Constraints.
- `Vault::all_values_for_masking() -> Vec<(PlaceholderRef, Zeroizing<String>)>`(CLI·MCP에서 호출 불가하도록 `pub(crate)` + broker 전용 feature 또는 trait로 노출 — 구현자가 가장 좁은 방법을 고른다).
- `ops.redact`: 모든 op 응답 JSON의 문자열 값에 적용. snapshot은 추가로 `type=password` 입력의 value를 제외.
- [ ] 단위 테스트(겹침, 변형, 짧은 값 제외, 카드 형식) + 실제 Chrome 테스트(Review Focus 3) → 커밋 `feat: mask vault values in every agent-facing response`

## Task 5: `keyharbor run`
- `keyharbor run [--agent A] (--env ITEM | --env-file PATH)... -- CMD ARGS…`: 정책 `UseItem` 판정 → env 항목 변수들 + `.env.keyharbor` 템플릿 placeholder 치환 → 자식 실행 → stdout/stderr를 줄 단위로 마스커에 통과시켜 출력 → 종료 코드 전달 → `env_injected` 기록. 자식에는 `RLIMIT_CORE=0`.
- [ ] 테스트: `sh -c 'echo $API_KEY; echo $API_KEY >&2; exit 3'` → 출력에 값 없음·placeholder 있음·exit 3; 거부 정책이면 자식 미실행(Review Focus 4) → 커밋 `feat(cli): keyharbor run injects env items and masks child output`

## Task 6: MCP 연결과 문서
- MCP `browser_fill`/`browser_type` 설명에 placeholder 사용법, `vault_list`에 placeholder 예시 필드 추가. `session_open`·`browser_*`에 정책(OpenSession, CreateSession, RawCdp) 적용.
- README(루트·rebuild), docs/design/05 3단계 완료 표시, ROADMAP, `.sticky/roadmap.md`.
- [ ] 전체 검증(fmt, clippy, test with Chrome env, check-docs, test-scripts, cargo deny) → 커밋 `docs: phase 3 substitution, masking and policy`
