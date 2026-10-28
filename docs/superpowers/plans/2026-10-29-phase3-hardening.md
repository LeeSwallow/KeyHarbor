# 3단계 보안 강화 계획 (독립 보안 검토 C1–C3, I1–I5 대응)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. Steps use `- [ ]`. 각 항목은 **재현 테스트를 먼저** 쓰고(검토자 PoC: `/private/tmp/khpoc/KeyHarbor-rebuild/crates/cli/tests/zz_poc.rs` 참고) 실패를 본 뒤 고친다.

**Goal:** 검토에서 재현된 평문 유출(S1 위반)과 origin 바인딩 우회(S4 위반)를 막고, 사용자 결정 R-35(C3)를 구현한다. 모든 수정은 보안 계층 `keyharbor-guard`(PEP)와 broker의 `FrameInspector`·browser 모듈 안에서 한다.

## Global Constraints
- 리팩터링 후 구조(헥사고날, guard PDP/PEP, GuardedBrowser, PlaintextAccess, Diesel 컨텍스트) 유지, `scripts/check-layers.sh` 통과.
- PATH, Chrome env, 허용 명령, 키체인 금지, 평문 금지, 커밋 규칙은 이전과 같다.

## 수정 항목
### C1 — 편집 후 재조회로 마스킹 우회
- guard가 세션 동안 **치환 텍스트를 받은 요소의 backendNodeId 집합**(`tainted_nodes`)을 기억한다. snapshot·selector 결과·오류에서 이 노드의 value와 자손 텍스트를 항상 `[redacted]`로 바꾼다(값이 바뀌어도).
- `login.password`(그리고 FieldSpec에 `input_kind: Password` 표시가 있는 필드)는 `input[type=password]` 또는 `autocomplete∈{current-password,new-password}` 요소에만 입력을 허용한다. 아니면 `denied: <item> (field-kind)`.
- type이 password→text로 바뀐 tainted 노드도 계속 가린다.
- 테스트: username 칸에 password 입력 시 거부; password 칸 입력 후 Backspace·Home/Delete·type 전환 → snapshot에 값 조각 없음.
### C2 — shadow DOM·object로 origin 우회
- 포커스 추적: `document.activeElement` → 열린 `shadowRoot.activeElement`를 끝까지 따라가고, 하위 frame 진입은 localName이 아니라 CDP `DOM.describeNode`의 `frameId`/`contentDocument` 유무로 판단(iframe·frame·object·embed·fencedframe 모두).
- 최종 포커스 요소가 `input`·`textarea`·contenteditable이 아니면 거부(`focus`). 입력 직후 같은 방식으로 다시 판정해 frame·node가 같은지 비교, 다르면 경고가 아니라 **세션의 치환 중지(`substitution_locked`)** + audit.
- 테스트: shadow root 안 다른 origin iframe, `<object data=B>` 각각 `type` 거부.
### C3 — 셸 에이전트와 `run`·약화 작업 [사용자 결정 R-35]
- 항목 속성 `run_allowed: bool`(DB 컬럼 추가 — vault 컨텍스트 새 마이그레이션, 메타 MAC 입력에 포함). 기본값: `env` 종류 true, 그 외 false. `keyharbor run`은 `run_allowed`가 false인 항목을 거부(`denied: <item> (run-not-allowed)`).
- **약화 작업은 사람 확인 필요**: `vault edit --origin-binding off`, `--origin` 추가, `run_allowed` 켜기, `policy allow`(어떤 대상이든), `policy rm`(deny 규칙 제거), `settings set audit_enabled false`, `settings set capture_enabled …`. 확인 방식: stdin과 stderr가 모두 TTY일 때 `/dev/tty`에서 `yes` 입력을 받는다. TTY가 없으면 `"<작업> needs confirmation from a person at a terminal (or the admin app)"`로 거부. 5단계 관리 앱은 자체 확인 UI로 같은 core API를 쓴다.
- 문서: 03 §3.1·§3.12·T13·T14에 "터미널 확인은 pty를 흉내 내면 우회 가능한 약한 경계" 명시, J-기록.
- 테스트: 비TTY에서 각 약화 작업 거부; run이 비허용 항목 거부; env 항목은 기본 허용.
### I1 — 마스커를 추측 확인 도구로 사용
- guard가 에이전트가 보낸 리터럴(URL, fill/type 텍스트, selector, new_tab url)에 마스커를 돌려 **일치하면 요청 자체를 거부**하고 audit `mask_probe`; 한 세션에서 3회 이상이면 세션 치환 중지.
- 응답 마스킹 placeholder에서 항목 이름을 빼고 `{{secret:redacted}}`로 통일할지: 에이전트가 자기가 넣은 값을 확인하는 용도가 있어 **유지**하되, I1 차단으로 추측 경로를 막는다(J 기록).
- 03에 "엔트로피 낮은 값(4–6자리 PIN 등)은 S1 보장 약함" 명시.
### I2 — JSON 이스케이프 우회
- snapshot 등 문자열을 JSON으로 만들기 **전에** 원문을 마스킹. 추가로 마스커 변형에 JSON 이스케이프 형태(`\"`, `\\`, `\uXXXX` 제어 문자) 추가.
### I3 — selector 오라클
- selector 허용 문법: 태그, `#id`, `.class`, `[name=…]`·`[type=…]`·`[id=…]`·`[aria-label=…]`·`[data-testid=…]`, 결합자(공백, `>`), `:nth-of-type(n)`. 그 외(특히 `value`·`:has`·`:placeholder-shown`·`:invalid`·`:valid`) 거부. CSS 이스케이프를 해제한 뒤 판정.
### I4 — 에이전트가 만드는 TOCTOU
- `click`·`press` 뒤 내비게이션이 시작되면(Page.frameRequestedNavigation/frameStartedNavigating) 완료까지 치환을 거부하고, 확인~insertText 사이에 해당 frame에서 내비게이션 이벤트가 오면 중단.
### I5 — form-urlencoded 변형
- 변형 추가: 영숫자와 `*-._`만 남기고 공백은 `+`, 나머지 `%XX`(대문자).
### Minor (같이 처리)
- control socket `agent`에 `*`·빈 문자열 거부. 감독자·CLI 프로세스에도 `RLIMIT_CORE=0`. `denied` audit에 세션당 초당 상한(초과분은 카운터 이벤트 하나로 합침).

## Tasks
1. C1 2. C2 3. C3 4. I1+I2+I5(마스커) 5. I3 6. I4 7. Minor 8. 문서(03 §3.7–§3.9 한계 갱신, 06 R-35·J, 05 3단계 보강 표시) + 전체 검증. 각 task 커밋 `fix(guard): …` / `feat: …`.

## 추가: 안전한 기본값 (R-36: "OPENAI_KEY 같은 것도 넣을 수 있게 하는 게 목적")
- `secret`·`env` 항목: `run_allowed` 기본 **true**(API 키를 코드·도구에 막힘없이 주입하는 것이 주 용도). 브라우저 `fill/type`은 **origin 목록이 있을 때만** 허용 — origin이 비어 있으면 `denied: <item> (no-origin)` (프롬프트 인젝션으로 공격자 사이트 입력란에 키를 넣는 유출 차단). origin 추가는 C3의 "약화 작업" 확인을 거친다.
- `card`: 지금처럼 바인딩 켜짐. `identity`·`address`: 어디든 허용하되, 그 항목이 처음 쓰이는 origin이면 audit `first_use_origin` + (5단계) 앱 알림. 막지 않는다.
- 사용 예(README·스킬): `keyharbor vault import-env openai <file>` → `keyharbor run --env openai -- <cmd>`; MCP 서버 설정을 `keyharbor run --env … -- <server>`로 감싸 평문 키를 설정 파일에 두지 않기.
- 테스트: env 항목 run 기본 허용; origin 없는 secret의 browser_fill 거부; origin 등록 후 허용; identity 첫 사용 origin 기록.

## 추가: 항목 만료 주기 (R-37: "적당히 만료주기까지 설정 가능하게, 기본은 무한으로")
- 항목 속성 `expires_at: Option<RFC3339 date>`(기본 None = 무한). vault 컨텍스트 새 마이그레이션(`vault_items.expires_at TEXT NULL`), 메타 MAC 입력에 포함.
- 설정 `default_item_ttl`(config 컨텍스트, 기본 없음; 예 `90d`) — 새 항목에 `now + ttl` 자동 적용.
- CLI: `vault add/edit --expires YYYY-MM-DD | --expires-in <N>d | --no-expiry`; `vault list`에 만료 열(`expires 2027-01-31`, `expired`), `--json`에 `expires_at`, `expired`.
- 만료 후: 값 보존, 에이전트 사용(browser fill/type, run) 거부 `denied: <item> (expired)` + audit. MCP `vault_list`에 `expires_at`·`expired`.
- 만료일 연장·제거는 C3 "약화 작업"(사람 확인), 단축·신규 설정은 확인 없음.
- 테스트: 기본 무한; 만료 전 허용·후 거부; 연장은 비TTY에서 거부; default_item_ttl 적용; DB에서 expires_at 변조 시 tampered.

## 추가: 설명 필드 (에이전트가 언제 무엇을 쓸지 알 수 있게)
- 항목 `description`, 필드별 `description`(FieldSpec 기본 설명 + 항목별 덮어쓰기), 세션 `description`. 각 500자 제한(초과 시 오류). 평문 비밀이 아니므로 평문 컬럼에 두되 **메타 MAC 입력에 포함**(변조 시 tampered).
- CLI: `vault add/edit --description <text>`, `--field-description <field>=<text>`, `session start/edit --description`. `vault list --json`과 MCP `vault_list`·`session_list`에 노출.
- 설명 텍스트도 마스커를 통과시킨다(사용자가 실수로 값을 적은 경우 대비). 설명에 비밀 값이 들어 있으면 `add/edit`에서 거부.
- 테스트: 설명 왕복, 500자 초과 거부, DB 변조 시 tampered, 설명에 같은 항목의 비밀 값이 있으면 거부.
- **결정(사용자 미응답, lead가 정함)**: env 항목은 계층을 만들지 않는다. 이름 규칙(`myapp-dev`, `myapp-prod`)과 설명으로 구분한다. 계층은 필요가 확인되면 다시 본다(J 기록).

## 추가: `browser_login` MCP 도구
- 입력: `{session, item, submit?: bool = true}`. item은 `login` 종류여야 한다.
- 현재 포커스와 무관하게, 최상위 또는 동일 origin frame에서 username 칸(`autocomplete=username` → `type=email` → `name|id ~ user|email|login` 순)과 password 칸(C1 규칙과 같은 판정)을 찾는다. 두 칸 모두 guard의 origin·만료·정책·field-kind 검사를 그대로 거친다(새 우회 경로를 만들지 않는다: 내부적으로 기존 PEP 경로를 호출).
- `submit`이면 password 칸에서 Enter. 결과는 마스킹된 짧은 요약(`filled username, password; submitted`)과 이후 URL.
- 칸을 못 찾으면 `not found: username field`처럼 오류, 아무것도 입력하지 않는다.
- 테스트(실제 Chrome): 정상 로그인 폼, password 칸 없는 폼 거부, 다른 origin 페이지 거부, 응답에 값 없음.

## 추가: browser-use 연동 예제 (문서만)
- `docs/guides/browser-use.md`: browser-use의 `sensitive_data` 대신 KeyHarbor placeholder를 쓰는 방법, 또는 `keyharbor run --env` 로 API 키 주입. 코드 의존성은 추가하지 않는다. **결정(lead)**: 예제는 문서로만 둔다.

## 문서·기록
- 06: R-35(C3), R-36(안전 기본값), R-37(만료), R-38(설명 필드), R-39(browser_login), 위 두 결정의 J 항목.
- 03: §3.7–§3.9 한계, 터미널 확인의 약함, 저엔트로피 값.
- README·rebuild/README: run 기본 허용, 만료, 설명, browser_login, 약화 작업 확인.
