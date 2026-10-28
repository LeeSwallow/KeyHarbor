# 4단계 사이트 로그인 상태와 미등록 정보 등록 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. Steps use `- [ ]`.

**Goal:** 세션 × 사이트별 로그인 상태를 기록하고(02 §2.7 SITE_STATE), 금고에 없는 정보를 에이전트가 값을 보지 않고 등록하는 네 경로(08: 생성·포착·요청·위임 저장)와 draft 후 확정 모델을 구현한다.

**Architecture:** sqlite에 `site_states`, 금고 항목에 이미 있는 `status`(draft/active/discarded)·`source`를 사용한다. MCP에 `site_state_get/set`, `vault_generate`, `vault_put`, `vault_request`, `vault_wait`를 추가한다. 포착은 감독자가 Chrome isolated world에 submit 감지 스크립트를 넣고 `Runtime.addBinding`으로 받는다. 요청(`vault_request`)은 5단계 관리 앱의 보안 입력 창이 생기기 전까지 CLI `keyharbor vault fulfill <request-id>`(숨김 프롬프트)로 응답한다. 기준: docs/design/08, ADR-0013.

## Global Constraints
- 1·3단계 제약 유지. 생성 비밀번호는 OS CSPRNG, 기본 20자, 규칙 `{length, symbols: bool, forbid: String}`.
- `vault_put`/포착은 **기존 항목의 값을 덮어쓰지 않는다**: 같은 이름이면 `<name>~draft-N` 새 draft를 만들고 기존 값 유지(08 §8.3). 확정 시 원본을 교체한다.
- 자동 확정: 같은 세션·origin에 `site_state_set(logged_in)`이 기록되면 그 origin의 `login` draft 중 최신 하나를 active로(또는 원본 교체).
- draft 자동 폐기: 7일(`settings.draft_ttl_days`), `vault list`/`keyharbor vault prune-drafts` 시 처리.
- 포착된 값은 감독자 안에서 곧바로 `Vault::add`(draft, source=captured)로 가고 응답·로그에 나오지 않는다.

## Review Focus
1. 포착이 이미 있는 계정의 비밀번호 변경을 감지 → 기존 값은 그대로, draft 후보만 생김.
2. `vault_put`으로 기존 항목 이름을 보내면 덮어쓰지 않음(인젝션 바꿔치기 방지).
3. `vault_wait` 타임아웃은 대기만 끝내고 요청은 남음; 거절하면 `declined`.
4. `site_state_set`의 origin이 잘못된 형식이면 거부.
5. draft 항목도 placeholder로 즉시 입력 가능(P3).

## Tasks
1. `site_states(session_id, origin, state CHECK IN('logged_in','expired','unknown'), item_name, checked_at, recorded_by, PRIMARY KEY(session_id,origin))` + core API + MCP `site_state_get {session, origin}`, `site_state_set {session, origin, state, item?}` + CLI `keyharbor site list <session>`. 테스트 → 커밋 `feat: per-site login state`
2. `vault_generate {name, kind=login, origin, username?, password_rule?}` → draft 항목, 반환 `{placeholders, draft:true}`; `vault_put {name, kind, fields, origins?}` → draft(source=agent), 같은 이름은 `~draft-N`. 테스트(Review Focus 2·5) → 커밋 `feat(mcp): generate and put vault items as drafts`
3. 요청 흐름: 테이블 `vault_requests(id, agent, kind, name, reason, origin, status CHECK IN('pending','fulfilled','declined','expired'), created_at)`; MCP `vault_request`, `vault_wait {request_id, timeout_seconds 1..=300}`; CLI `keyharbor vault requests`, `keyharbor vault fulfill <id> [--stdin]`, `keyharbor vault decline <id>`. 테스트(Review Focus 3) → 커밋 `feat: secure input requests answered outside the agent`
4. 포착: 감독자가 각 페이지에 `Page.addScriptToEvaluateOnNewDocument {worldName:"keyharbor"}` + `Runtime.addBinding {name:"__khCapture", executionContextName:"keyharbor"}`; 스크립트는 password 입력이 있는 form submit/Enter/버튼 클릭 시 `{origin, username, password}`를 binding으로 보낸다. 감독자는 08 §8.2 ② 규칙대로 draft 저장 + 사용 기록 `credential_captured`. 설정 `capture_enabled`(기본 true). 실제 Chrome 테스트(Review Focus 1). 커밋 `feat(broker): capture logins typed by the user as drafts`
5. draft 확정·폐기: `site_state_set(logged_in)` 자동 확정, CLI `keyharbor vault confirm <name>`, `discard <name>`, `prune-drafts`. 테스트 → 커밋 `feat: confirm drafts on login success`
6. 문서(README·rebuild/README·05·ROADMAP·.sticky) + 전체 검증 → 커밋 `docs: phase 4 site state and registration`

## 추가 (2026-10-29): 사이트 넘기기와 grant
설계: [사이트 넘기기와 grant](../specs/2026-10-29-site-handoff-and-grants-design.md) (R-42, R-43).
7. grant 모델: access 컨텍스트에 `grants(id, subject_kind, subject, scope_kind, scope, until, created_by, reason, created_at, revoked_at)` 새 마이그레이션. guard PDP가 세션의 Origins grant로 최상위·frame 탐색을 제한(`denied: navigation (out-of-grant)`), 만료·회수 시 해당 쿠키 삭제. CLI `keyharbor grant list|add|revoke`(add는 약화 작업). 실제 Chrome 테스트: grant 밖 탐색 거부, 하위 리소스는 허용, 만료 후 쿠키 없음. 커밋 `feat: grants scope sessions to origins`
8. 사이트 넘기기: `integrations/chrome-extension/`(MV3, 압축 해제 설치, 권한 `cookies`, `nativeMessaging`, `activeTab`), native host `keyharbor browser host`(stdin/stdout 길이 접두 JSON), 등록 `keyharbor browser install-host`(매니페스트를 `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/`에, `allowed_origins`는 확장 ID 하나). 감독자 control 명령 `receive_site {origin, cookies, local_storage?, ttl}` → `Network.setCookies` + grant + 사용 기록 `site_handed_off`. 쿠키 값은 응답·로그·DB에 없음(누출 테스트). 커밋 `feat: hand a site's login from your Chrome to an agent session`
