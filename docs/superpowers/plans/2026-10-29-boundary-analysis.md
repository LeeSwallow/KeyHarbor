# 비즈니스 로직과 인터페이스 분리 전건 분석 (main @ d7953bc)

기준: `.claude/rules/writing-code.md`, ADR-0014/0015/0016, `scripts/check-layers.sh`.
경로는 모두 `rebuild/crates/` 기준이다(확장은 `integrations/browser-extension/src/`). 줄 번호는 HEAD 기준이다.
Critical은 없다. 평문 비밀이 새는 경로는 찾지 못했다.

## 인터페이스별 요약

| 인터페이스 | 어댑터 위치 | 안쪽 누수 | 판정이 바깥에 있음 | 조립 |
|---|---|---|---|---|
| CLI (clap) | cli/src/*_cmd.rs | — | 설정 레지스트리, 정책 변경, 항목 add/edit의 순서, session create | 명령마다 `SqliteStore::open`/`keystore_from_env` |
| MCP stdio (rmcp) | mcp/src | — | 세션 PDP 호출, vault_list, wait_for_resume, 에이전트 이름 검사 | 요청마다 store를 연다(10곳) |
| Control socket | broker/src/control.rs, ops.rs | `BrowserPort`의 `Value` target/result | 에이전트 이름 검사(3중), `submit` 기본값(2중) | broker supervisor 안 |
| CDP pipe | broker/src/cdp, browser | 없음(Element/Focus로 번역) | — | supervisor |
| Native Messaging | 확장 domain/protocol.ts | wire 타입이 TS `domain/`에 있음 | 세션 이름 규칙이 Rust와 다름 | adapters/{chrome,firefox}/background.ts |
| SQLite (Diesel) | sqlite/src/contexts | `ItemRow` 문자열 필드 | audit 켜짐 판정, handoff 취소 조건 | StoreFactory |
| Keychain | **vault/src/keystore.rs (안쪽 crate)** | core가 security-framework를 링크함 | env로 저장소를 고름 | `keystore_from_env` |
| 자식 프로세스 (`run`) | cli/src/run_cmd.rs | **guard가 파일을 읽음** | 에이전트 이름 검사 | run_cmd |
| supervisor argv | broker/src/runtime.rs ↔ cli Supervise | 계약이 두 군데에 있음 | — | — |
| design-serve HTTP | design-check/src/serve.rs | 해당 없음(dev tool) | — | bin |

## 발견 사항

| 발견 | 심각도 | 위치 | 무엇이 문제인가 (규칙) | 고칠 방법 | Task |
|---|---|---|---|---|---|
| 1 | Important | mcp/src/tools/vault.rs:15-19, 29-60 | `vault_list`가 core `Vault::list`를 거치지 않고 `VaultStore::item_list`를 직접 읽는다. 만료 판정(`expired`)이 core/src/vault/summary.rs:57-59와 두 벌이고, **메타데이터 MAC을 검증하지 않아** `tampered`를 모른 채 origins·run_allowed·description을 에이전트에게 준다. CLI `vault list`(cli/src/vault_cmd.rs:398)는 검증된 경로를 쓴다. 같은 규칙이 CLI와 MCP에 따로 있다 | 키 없이 값을 뺀 `Vault::list_metadata()`(MAC 검증 포함)나, 금고를 여는 `core::vault::list`를 두고 MCP가 그것만 부르게 한다. JSON 모양은 MCP 쪽 mapper 하나로 | #10 (확장) |
| 2 | Important | domain/src/vault.rs:419(비공개 `PLACEHOLDER_OPEN`), guard/src/browser.rs:24, guard/src/pep/mask/mod.rs:51, guard/src/login.rs:42, core/src/vault/resolve.rs:105, mcp/src/tools/vault.rs:42 | placeholder 문법 `{{secret:item.field}}`을 여섯 곳에서 따로 만들거나 찾는다(불필요한 반복) | domain에 `impl Display for PlaceholderRef`와 `pub const PLACEHOLDER_OPEN`을 두고 나머지는 그것만 쓴다. check-layers에 리터럴 금지 규칙을 넣는다 | NEW |
| 3 | Important | mcp/src/lib.rs:31-36, broker/src/ops.rs:57-66, cli/src/run_cmd.rs:62-65 | "요청자 에이전트 이름은 `*`가 될 수 없다"가 세 군데 있다. `parse_agent`(domain/src/policy.rs:72)는 `*`를 받아들이므로 호출하는 쪽마다 덧붙인다 | domain에 요청자용 newtype `AgentName::parse`(`*` 거부)를 두고 규칙용 `parse_agent`와 나눈다. 세 곳은 그것만 부른다 | NEW |
| 4 | Important | mcp/src/server.rs:35-41, 91; mcp/src/tools/session.rs:100, 134-137, 286-287 | 세션 수준 PDP(`OpenSession`·`CreateSession`·`RawCdp`)를 어느 작업에서 물을지를 MCP 표면이 정한다. control socket 쪽(broker/src/ops.rs:120-145)은 세션 정책을 보지 않는다. 그래서 `session_stop`과 `session_wait_auth`는 정책 검사가 없고, 그것이 의도인지 코드로 알 수 없다 | `guard::session::authorize(store, agent, op)`처럼 작업에서 Action으로 가는 표를 guard에 하나 두고, core 세션 유스케이스 앞이나 supervisor의 ops 진입점에서 부른다. MCP는 결과만 옮긴다 | #10 (확장) |
| 5 | Important | mcp/src/tools/session.rs:200-268 | `session_wait_for_resume`의 폴링, 상태 해석(Resumed인데 Running이 아니면 실패, Pending인데 supervisor가 죽음), 시간 한도 `1..=300`과 기본값 `120`이 표면에 있다. 매직 넘버이고 설정으로 빠져 있지 않다 | `core::session::wait_for_resume(store, runtime, id, request, deadline) -> ResumeOutcome`. 한도는 settings나 이름 붙은 상수로. MCP는 async sleep 어댑터만 남긴다 | #10 |
| 6 | Important | mcp/src/tools/session.rs:96-108, cli/src/session_cmd.rs:117-128 | session create가 `store.create`와 `set_description` 두 번의 저장소 호출로 CLI와 MCP에 같은 모양으로 두 벌 있다(원자적이지 않다) | `core::session::create(store, name, description)` 하나로 모은다(포트에 `create_with_description`을 두는 것도 방법) | #10 |
| 7 | Important | cli/src/audit_cmd.rs:72-114 | 설정 레지스트리(키, 기본값, 검증 함수)가 CLI에 있다. 같은 기본값이 sqlite/src/contexts/config/repo.rs:19(`unwrap_or("true") == "true"`)에도 있고, 의미가 세 곳에서 다르다. domain/src/ports/audit.rs:6은 "`false`면 끔, 나머지는 켬", sqlite는 "`true`만 켬", core/src/weaken.rs:107은 "`true`가 아니면 약화"다 | domain(또는 core)에 `SETTINGS: &[SettingSpec]` 레지스트리를 둔다(KindSpec와 같은 방식). sqlite는 문자열만 저장하고 켜짐 판정은 레지스트리의 `audit_enabled(value)` 하나가 한다 | NEW |
| 8 | Important | cli/src/policy_cmd.rs:54-141 | 정책 set/rm 유스케이스(약화 확인, 저장, `PolicyChanged` 기록)가 CLI에 있다. 앞으로 관리 앱이나 MCP가 정책을 바꾸면 이 순서를 다시 써야 한다 | `core::policy::{set, remove}(store, confirm, rule)`을 두고 CLI는 인자 파싱과 출력만 한다 | #10 (확장) |
| 9 | Important | cli/src/vault_cmd.rs:316, 359, 386 | 항목 add/edit의 순서(설명에 값이 섞였는지 `describe::refuse_values`, 약화 계산, `weaken::require`, 그다음 `vault.edit`)가 표면에 있다. guard→core 방향 때문에 core에 넣을 수 없어서 CLI가 조립했다 | guard에 `items::add/edit(vault, confirm, …)`를 두거나, core 유스케이스가 `ValueLeakCheck` 포트를 받게 한다 | #10 (확장) |
| 10 | Important | vault/src/keystore.rs:1-160, vault/Cargo.toml:18-19 | Keychain 어댑터(`security-framework`)와 `FileKeyStore`(std::fs), env 기반 선택(`keystore_from_env`, `std::env::var_os`, `eprintln!`)이 **안쪽 crate** `keyharbor-vault`에 있다. core와 guard가 security-framework를 링크한다(`cargo tree -i security-framework`: vault→core→guard). 규칙 표는 keystore를 Frameworks & Drivers로 둔다 | `crates/keystore`(또는 adapters/keystore)로 옮기고 `keyharbor-vault`에는 레코드·키 함수만 남긴다. `keystore_from_env`는 CLI의 composition root로 옮긴다. 이름은 규칙대로 `KeychainKeyStore`로(지금은 `MacKeychain`) | NEW (#11과 같이 하면 좋다) |
| 11 | Important | guard/src/run.rs:126-130 | guard가 `std::fs::read_to_string`으로 `--env-file`을 직접 읽는다(안쪽 crate의 파일 I/O) | `Request.files`를 `&[(String /*표시용 이름*/, Zeroizing<String>)]`로 바꾸고 CLI run_cmd가 파일을 읽는다 | NEW |
| 12 | Important | core/src/transfer.rs:82-160(`export_age`/`import_age`가 `&Path`를 받음, 138-147 OpenOptions 0600, 153 fs::read) | core 유스케이스가 파일을 만들고 읽는다. 문서 형식(serde `ExportOut`/`ExportIn`)은 07 §7.7이 정한 우리 형식이라 core에 있어도 되지만 파일은 바깥 일이다 | `export_age(..) -> Zeroizing<Vec<u8>>`, `import_age(.., bytes: &[u8])`. 0600·create_new 쓰기는 CLI(또는 작은 file 어댑터)로 | NEW |
| 13 | Important | core/src/rotate.rs:18-40, domain/src/ports/vault.rs:56-58(`fn root(&self) -> &Path`) | 키 교체 잠금을 core가 `std::fs::File::lock`로 직접 잡는다. 그 때문에 포트가 데이터 디렉터리 경로를 드러낸다 | `trait RotationLock { fn lock_rotation(&self) -> Result<Guard> }` 포트를 두고 sqlite나 broker 쪽이 구현한다. `VaultStore::root`는 지운다 | NEW |
| 14 | Important | domain/src/ports/browser.rs:6, 10, 24-38, 108-113 | `BrowserPort`·`PageInput`이 target을 `serde_json::Value`(`{"ref":N}`/`{"selector":css}`)로 받고, 결과를 "control protocol's shape" JSON으로 돌려준다. 안쪽 port에 wire 모양이 드러난다. guard/src/browser.rs:225-245는 `target.get("selector")`로 wire를 파싱하고, guard/src/browser.rs:129-133과 guard/src/login.rs:43, 51은 응답 JSON 모양을 만든다. J-43("포트의 응답 타입")은 결과 쪽만 정당화하고 target은 정당화하지 못한다. J-43 본문과 ADR-0014:67은 아직 domain→`getrandom`이라고 쓰는데, 지금은 `uuid`다 | domain에 `enum Target { Ref(u64), Selector(String) }`를 두고 broker/src/ops.rs의 `parse`에서 번역한다. guard의 응답은 `FillOutcome { origin_changed: bool }` 같은 값으로 바꾸고 JSON은 ops가 만든다. 결과 `Value`는 guard가 통째로 마스킹하므로 당분간 남겨도 된다(남기는 이유를 J-43에 다시 쓴다) | NEW |
| 15 | Important | broker/src/ops.rs:36-111(파서), mcp/src/server.rs:87-92 및 mcp/src/tools/browser.rs 전체(`json!({"op":…})`), broker/src/runtime.rs:96 | control socket 프로토콜에 wire 타입이 없다. 클라이언트(MCP, runtime)는 `json!`으로 만들고 서버(ops.rs)는 손으로 파싱한다. 기본값도 두 벌이다(`submit` 기본 true: mcp/src/tools/browser.rs:185, broker/src/ops.rs:99) | `broker::control::wire`에 `#[derive(Serialize, Deserialize)] enum ControlRequest { Navigate{url}, … }`와 `ControlResponse`를 두고, 클라이언트와 서버가 이 하나만 쓴다. `ops::parse`는 serde가 대신한다 | #11 |
| 16 | Important | broker/src/supervisor/mod.rs:59-103, 106-157; broker/Cargo.toml(guard 의존) | (#11 확인) 세션 상태 기계(예약 유효성 67-72, Starting→Running/Failed 129-140, record_exit)와 Guard·CdpBrowser·Ops 조립(144-152)이 CDP 어댑터 crate 안에 있다. mcp/src/scope.rs:7, 17은 자기 lock을 잡으려고 `broker::supervisor::acquire_lock`과 `paths::mcp_lock_path`를 쓴다 | 계획대로 `crates/supervisor`로 옮긴다. 이때 상태 전이는 `core::session::{mark_running, mark_exited}`로 올리고, lock·paths는 supervisor crate의 공개면으로 만든다 | #11 |
| 17 | Important | 확장 domain/protocol.ts:1-64, domain/cookie.ts:26-38 | Native Messaging wire 타입(`ReceiveSiteWire`, `HostReplyWire`)과 CDP wire(`CdpCookieParam`)가 `src/domain/`에 있다. 규칙은 wire를 어댑터에 둔다. 검증이 한 파일에 모여 있는 것은 좋다 | `src/adapters/native-host/protocol.ts`(또는 `src/wire/`)로 옮긴다. domain에는 `HostReply`, `SiteCookie`, 번역 규칙(`toCookieParam`의 판단부: 범위·파티션)만 남긴다 | NEW |
| 18 | Important | 확장 domain/handoff.ts:8 ↔ domain/src/lib.rs:28-35 | 세션 이름 규칙이 확장(`^[a-z0-9][a-z0-9_-]{0,63}$`)과 Rust `SessionName::parse`(1-120바이트, 제어 문자만 금지)에서 다르다. Rust 호스트가 생기면 같은 개념에 규칙이 둘이 된다. "Work" 같은 이름은 Rust에서는 되고 확장에서는 막힌다. `ttl_seconds` 범위(handoff.ts:6-7)도 Rust 쪽 짝이 아직 없다 | Native Messaging 명세(docs/design)에 규칙 하나를 적고 Rust 호스트를 판정 기준으로 둔다. 확장은 UX용 사전 검사로 같은 규칙을 쓰거나 세션 ID를 보낸다. 범위 검사는 Rust 호스트 도입 때 domain `Grant` TTL로 함께 정한다 | NEW (4단계 전) |
| 19 | Important | vault/src/keys.rs:13, domain/src/lib.rs:9, 102, 121(`Uuid::new_v4`) | 난수가 crypto 밖에서 만들어진다. 키 ID(vault_id·kek_id·dk_id)는 암호 형식(AD)에 들어가는 값이다. 규칙("난수는 keyharbor-crypto 한 곳")과 ADR-0016에 어긋나고, check-layers는 `getrandom`만 grep해서 못 잡는다 | `keyharbor_crypto::random::id16()`을 두고 vault의 키 ID는 그것으로 만든다. domain의 SessionId/Generation/RequestId 생성은 비밀이 아니므로, crypto를 쓰거나 그대로 두되 예외로 ADR-0016에 적는다 | NEW |
| 20 | Minor | core/src/session/lifecycle.rs:16-17, 39, 69, 98, 136, 152; core/src/lib.rs:31-55; guard/src/guard.rs:75-79 | 안쪽 crate가 시계(`SystemTime::now`, `Instant::now`, `thread::sleep`)와 파일 시스템(`Path::is_file`)을 직접 쓴다. `unix_seconds`가 core와 guard에 두 벌 있다. 시작·종료 대기 15초가 설정으로 빠져 있지 않다 | domain에 `trait Clock { fn now_unix(&self) -> u64; }` 포트를 두고 composition root에서 주입한다. 브라우저 실행 파일 확인은 `SessionRuntime::start`(broker)로 옮긴다 | NEW |
| 21 | Minor | cli/src/expiry_args.rs:32-35 ↔ core/src/vault/items.rs:101-106 | "TTL + 오늘 = 만료일" 계산이 CLI와 core에 두 벌 있다 | `ExpiryChoice::In(Ttl)`을 더하고 core만 `today()`를 부른다 | #10 |
| 22 | Minor | cli/src/vault_cmd.rs:117-127, 374-378, 432-437 | `KindArg`/`StatusArg`/`DomainArg`에서 domain으로 가는 종류별 `match`가 CLI에 있다. 규칙("CLI·MCP에 종류별 match를 더하지 않는다")에 어긋나서 새 종류를 더하면 CLI도 고쳐야 한다 | clap `value_parser`로 `ItemKind::parse`·`KINDS`를 직접 쓴다(가능한 값 목록은 레지스트리에서) | NEW |
| 23 | Minor | sqlite/src/contexts/session/repo.rs:171-173 | "Stopping/Stopped/Failed로 가면 대기 중인 handoff를 취소한다"는 조건이 어댑터에 있다(원자성 때문에 실행은 어댑터에서 하는 게 맞다) | 조건은 `State::cancels_handoff()`로 domain에 두고 repo는 그것을 부른다 | NEW |
| 24 | Minor | domain/src/ports/vault.rs:19-39(`ItemRow.kind/status/source: String`, `expires_at: Option<String>`) | 포트 레코드가 저장 모양(문자열)이라 core(summary.rs:66, 86-87)와 MCP(vault.rs:33)가 각각 파싱한다. MAC이 저장된 형식에 서명하므로 레코드 포트 자체는 정당하다 | 표면이 `ItemRow`를 못 보게 하면(발견 1, check-layers 규칙 A) 충분하다. 정당화 한 줄을 `ItemRow` 문서에 남긴다 | #10 |
| 25 | Minor | cli/src/session_cmd.rs:134, core/src/session/mod.rs(`display_state -> &'static str`), mcp/src/tools/session.rs:150, 192, 245 | 상태를 문자열로 비교하거나 만든다(`state != "stopped"`, `"running"` 리터럴) | `enum DisplayState { Stored(State), RecoveryRequired }`를 쓰고 `as_str`은 한 곳에만 둔다 | #10 |
| 26 | Minor | broker/src/runtime.rs:69-82 ↔ cli/src/session_cmd.rs:81-90, 240-254 | supervisor 프로세스 argv 계약(`--data-dir`, `__supervise id generation browser --url --cdp`)이 두 crate에 따로 있다 | #11에서 supervisor crate가 `SuperviseArgs`(build_argv/parse) 하나를 갖게 한다 | #11 |
| 27 | Minor | cli/src/run_cmd.rs:69-70 | 같은 명령에서 `SqliteStore::open`을 두 번 한다(guard 검사용과 Vault용) | composition root에서 한 번 열고 `Vault::store()`나 공유 핸들을 쓴다 | #10 |
| 28 | Minor | 확장 adapters/webext/popup.ts:15, adapters/webext/background.ts:9 | background가 popup에 주는 응답은 검증하지 않고 캐스트한다. 오류 응답 `{ok:false,error:"bad_command"}`를 `Result` 생성자를 거치지 않고 손으로 쓴다 | `app/commands.ts`에 `parseCommandReply`를 두고, `err("bad_command")`를 쓴다 | NEW |
| 29 | Minor | design-check/src/lib.rs:20-23, serve.rs:59-71 | design-serve가 `design/`이 아니라 **저장소 루트 전체**(.git 포함)를 loopback에 내보낸다. `file_for`는 `..`만 막고 저장소 안 심볼릭 링크는 따라간다. 또 검사 라이브러리 crate가 HTTP 서버까지 가지고 있다 | 루트를 `design/`과 확장 빌드 산출물로 좁히고, canonicalize 후 접두사를 검사한다. 서버는 bin 쪽 모듈로 옮겨도 된다 | NEW (#12 때 경로와 같이) |
| 30 | Minor | cli/src/main.rs:35 | `--agent` 도움말이 "used by policies in a later phase"인데 이미 쓰인다(mcp/src/server.rs:33-41) | 문구를 고친다 | NEW |
| 31 | Minor | docs/design/adr/0014-hexagonal-guard-layer.md:65, 67; scripts/check-layers.sh:31-37 | ADR은 "암호 원시 crate는 vault만"이라고 쓰는데 실제는 crypto다(ADR-0016). 허용 표가 실제보다 넓다(mcp→vault, cli→crypto는 쓰지 않는데 허용) | ADR 문구를 고치고, 표를 실제 의존으로 좁힌다 | #10 |

### 7. 크기와 구조 (리뷰 기준선 초과)

| 종류 | 위치 | 값 | 판단 |
|---|---|---|---|
| 파일 > 400 (테스트 제외) | cli/src/vault_cmd.rs | 452 | 인자 정의, 입력 수집(collect_fields·read_passphrase), 출력 mapper, 실행이 섞여 있다. `vault_cmd/{args,input,output,mod}.rs`로 나눈다 (#10) |
| | domain/src/vault.rs | 441 | ItemName·FieldName·Origin·Kind 레지스트리·placeholder가 한 파일에 있다. `vault/{names,origin,kind,placeholder}.rs`로 (NEW) |
| | guard/src/guard.rs | 404 | 기준선 위. 거부 기록 rate limit(`Denials`)을 `audit.rs`나 `denials.rs`로 떼어 낸다 (NEW) |
| | guard/src/browser.rs | 1066 (그중 테스트 약 750) | 가짜 브라우저를 `test_support.rs`로 옮긴다 (Minor) |
| 함수 > 50 | cli/src/vault_cmd.rs:294 `run` | 158 | 명령마다 함수 하나로 (#10) |
| | cli/src/session_cmd.rs:116 `run` | 147 | 같음 (#10) |
| | core/src/session/lifecycle.rs:28 `open` | 84 | 대기 루프를 `await_running`으로 뗀다 |
| | core/src/transfer.rs:151 `import_age`(75), :82 `export_age`(65) | | 발견 12의 바이트 API로 바꿀 때 같이 나눈다 |
| | core/src/rotate.rs:68(66), :145(55); core/src/vault/developer.rs:49(59); core/src/vault/open.rs:64(54); core/src/vault/items.rs:119(53) | | 기준선 근처. 리뷰 때 본다 |
| | broker/src/browser/snapshot.rs:113(71), frames.rs:37(67), cdp/client.rs:38(58), ops.rs:50(56), supervisor/mod.rs:107(56), supervisor/launch.rs:62(54) | | ops.rs `parse`는 발견 15의 serde로 없어진다 |
| | cli/src/run_cmd.rs:61(62), policy_cmd.rs:54(54) | | #10 |
| | design-check/src/rules/icons.rs:60(66), model.rs:59(54) | | dev tool, Minor |
| 폴더 > 8 직속 파일 | broker/src/browser | 10 | `input/`(input·frames·login)과 `page/`(navigate·navigation·wait·tabs·snapshot·screenshot)로 나눈다 |
| | design-check/src/rules | 11 | 레지스트리 패턴이라 예외 한 줄을 남긴다 |
| | domain/src/ports | 9 (mod 포함) | 경계선. 그대로 둔다 |
| | cli/tests | 15 + common | `tests/{vault,session,browser,mcp}_*.rs` 접두사가 이미 있다. 묶을지는 #12에서 정한다 |
| 파일 하나짜리 폴더 | cli/src/adapters/(tty_confirm.rs 하나) | | 규칙 위반이다. `cli/src/tty_confirm.rs`로 내린다 |
| 섞인 책임 | cli/src/audit_cmd.rs | audit 목록 + 설정 레지스트리 + settings 명령 | 발견 7 이후 `settings_cmd.rs`로 뗀다 |
| grab-bag 이름 | 없음 | `models.rs`는 컨텍스트 폴더 안 schema·mappers·repo 묶음이라 규칙이 허용한다 | — |

### 8. 테스트

| 심각도 | 위치 | 내용 | Task |
|---|---|---|---|
| Minor | core/src/lib.rs:57-120(`Env`, `env`, `init`, `reopen`) ↔ guard/src/test_support.rs:20-40 | 같은 테스트 금고 조립(SqliteStore + FileKeyStore)이 두 벌 있다. `keyharbor-sqlite`의 `test-support` feature나 dev 전용 crate 하나로 모은다 | NEW |
| Minor | mcp crate | 단위 테스트가 없고 모든 검증이 cli/tests/mcp_stdio.rs·mcp_scope.rs(바이너리 경유)에 있다. #10에서 MCP가 생성자 주입을 받으면 도구 단위 테스트를 mcp/src 안에 둘 수 있다 | #10 |
| Minor | scripts/test-scripts.sh:41 | check-layers를 현재 트리에서 한 번 돌리기만 한다. 규칙 문서("위반 예시로 실패하는 것을 확인한다")와 달리 실패 fixture가 없다 | NEW |
| — | cli/tests/* | crate를 가로지르는 시나리오(실제 Chrome 포함)가 cli/tests에 있고 `KEYHARBOR_TEST_CHROME`가 없으면 건너뛴다. 위치가 맞다 | — |

## check-layers.sh에 더할 규칙

```sh
# A (#10) 표면은 저장소를 직접 열지 않는다
#   Cargo: mcp의 normal deps에서 keyharbor-sqlite를 금지 (allowed: domain core guard broker)
#   grep:  grep -nE 'SqliteStore|SqliteStores|keystore_from_env|ProcessRuntime::new' cli/src/*_cmd.rs mcp/src
#          → cli/src/main.rs·cli/src/compose.rs만 허용
#   grep:  grep -nE '\.(item_list|item_get|list|create|set_description|policy_rules|policy_set|policy_remove|list_audit|setting_(get|set)|handoff_status)\(' mcp/src cli/src/*_cmd.rs
# B (#11) broker는 domain에만 의존 (allowed broker="domain"), 새 supervisor="domain core guard broker"
# C 안쪽 crate에 I/O·시계·출력이 없다 (outside_tests 재사용)
for c in domain crypto vault core guard; do
  outside_tests 'std::(fs|process|net|env)\b|\bFile::|Command::new|SystemTime::now|Instant::now|thread::sleep|println!|eprintln!|\btokio::' \
    | grep "^\./$c/src/"
done   # 지금: vault/keystore.rs, core/lib.rs:31, core/transfer.rs, core/rotate.rs, core/session/lifecycle.rs, guard/run.rs:128, guard/guard.rs:75
# D 안쪽 crate의 Cargo normal deps 금지 목록
#   domain|crypto|vault|core|guard: tokio rmcp clap diesel* libsqlite3-sys security-framework libc rpassword base64
# E domain의 serde_json 허용 목록 (발견 14 이후 0)
grep -rln 'serde_json' domain/src | grep -v '^domain/src/ports/browser.rs$'
# F 난수는 crypto만 (ADR-0016)
grep -rnE 'new_v4\(|OsRng|rand::|getrandom' --include='*.rs' . | grep -v '^\./crypto/' | grep -v '/tests/'
#   + Cargo: uuid의 "v4" feature는 crypto(또는 허용한 domain)만
# G 어댑터 기술은 자기 crate에만
#   security-framework → keystore(지금은 vault)만, rmcp → mcp만, clap → cli만, tldts는 TS
# H placeholder 문법은 domain 한 곳
grep -rnE '\{\{(\{\{)?secret:' --include='*.rs' . | grep -v '^\./domain/src/vault\.rs' \
  | grep -v '/tests/' | grep -vE '#\[tool\(|description =|^\S+:\s*//' # (cfg(test) 블록 제외에는 outside_tests 사용)
# I 요청자 에이전트 이름 규칙은 domain 한 곳
grep -rn 'reserved for rules that apply to every agent\|== ANY\b' --include='*.rs' . | grep -vE '^\./(domain|guard/src/pdp)/'
# J 세션 수준 PDP 호출은 guard/core만
grep -rnE 'Action::(OpenSession|CreateSession|RawCdp)' mcp cli
# K control 프로토콜 wire는 한 파일 (발견 15 이후)
grep -rnE '"op"\s*[:\]]' --include='*.rs' mcp cli broker | grep -v 'broker/src/control/wire.rs' | grep -v '/tests/'
# L 허용 표를 실제 사용으로 좁힌다: mcp에서 vault, cli에서 crypto를 뺀다
# M 자기 검사: scripts/test-scripts.sh가 임시 트리에 위반 fixture(예: core/src/x.rs에 std::fs::read)를 만들어
#   check-layers가 실패하는지 본다 (CHECK_LAYERS_ROOT 환경 변수로 루트를 바꿀 수 있게)
# N 확장 (npm, vitest test/architecture.test.ts): src/domain/**/*.ts에서
#   /\b(chrome|browser)\./, /globalThis/, /from "\.\.\/(adapters|app|ui|ports)/ 를 금지,
#   src/app은 adapters·ui를 import하지 않음, *Wire 타입 선언은 adapters/(또는 wire/)에만
```

## 잘 된 것 (유지할 것)

1. **평문 capability와 decorator.** `PlaintextAccess::issue` 호출 위치를 check-layers 규칙 4가 막는다. 브라우저 쪽 평문은 `GuardedBrowser`(guard/src/browser.rs)를 지나야만 하고, raw CDP는 broker 밖으로 나가지 않는다(broker/src/lib.rs:7-8, check-layers 규칙 5). 표면은 값을 볼 길이 없다.
2. **검사 사슬 하나를 두 경로가 함께 쓴다.** `pdp::checks::{BROWSER, PROCESS}`(guard/src/pdp/checks/mod.rs:46-63)를 브라우저 입력과 `keyharbor run`(guard/src/run.rs:89)이 같이 쓴다. 검사 하나는 파일 하나다. ADR-0014가 지적한 "run 검사가 CLI에 따로 있다" 문제가 실제로 해결됐다.
3. **CDP를 우리 말로 번역한다.** broker가 CDP JSON을 `Element`·`Focus`·`LoginFields`·`Origin`으로 바꿔서 넘긴다(domain/src/ports/browser.rs:46-104). 판정(`Element::is_password`, `takes_text`)은 domain에 있다. `FrameInspector`와 `PageInput`을 나눈 것도 ISP를 따른다.
4. **SQLite 경계.** 바운디드 컨텍스트마다 schema·models·mappers·repo가 있고 서로 가져다 쓰지 않는다(check-layers 규칙 6). 상태 전이 허용은 domain `State::permits`를 부르고(sqlite/src/contexts/session/repo.rs:157), generation CAS는 SQL 조건으로 원자적으로 한다. 운영 코드에는 원시 SQL이 없다.
5. **확장의 포트 구조.** `src/domain`은 브라우저 API를 import하지 않는다(외부 의존은 `tldts`뿐). `CookieWire`는 webext 어댑터 안에서 `SiteCookie`로 번역된다(adapters/webext/cookies.ts:14). Chrome과 Firefox는 cookie query 한 줄만 다르다(adapters/chrome/cookies.ts, adapters/firefox/cookies.ts). 각 background.ts가 10줄짜리 composition root다. popup 명령 검증은 `parseCommand` 하나, 호스트 응답 검증은 `parseHostReply` 하나로 모여 있다.

(덧붙여 core/src/weaken.rs의 `WeakeningOp` 표와 `Confirm` 포트 덕분에 CLI의 약화 확인은 `weaken::require(&TtyConfirm, …)` 한 줄이다. 이 모양을 발견 8·9의 유스케이스로 옮길 때도 유지한다.)

## 부록 A. #10 위치 확인 (정확한 줄)

`SqliteStore::open` 직접 호출. 전달받은 줄 번호(session.rs:74,95, server.rs:37, session_cmd.rs:106, audit_cmd.rs:42)는 함수 시작 줄에 가깝고, 실제 호출 줄은 아래와 같다.
- mcp: server.rs:37, scope.rs:14, tools/session.rs:78, 107, 145, 167, 191, 224, 288, tools/vault.rs:29 (전달 목록에 없던 것: **scope.rs:14, vault.rs:29**, session.rs의 145·167·191·224·288)
- cli: session_cmd.rs:111-112(supervisor 조립), 120, 132, 157, 176, 187, 193, 204, 213, 231, 238; audit_cmd.rs:45, 118; policy_cmd.rs:55; vault_cmd.rs:291, 297; run_cmd.rs:69-70
- `keystore_from_env`: vault_cmd.rs:291, 297, session_cmd.rs:112, run_cmd.rs:70 / `ProcessRuntime::new`: mcp/src/lib.rs:42, cli/src/session_cmd.rs:104

## 부록 B. 조립 위치 (질문 4)

| 바이너리/프로세스 | 지금 조립하는 곳 | 하나인가 |
|---|---|---|
| `keyharbor` CLI | 명령 파일마다 따로(vault_cmd.rs:290-298 `open`, session_cmd.rs:103-114 `runtime`/`supervisor_adapters`, run_cmd.rs:69-70, policy_cmd.rs:55, audit_cmd.rs:45/118). main.rs는 data dir만 정한다 | 아니다 |
| `keyharbor mcp` | mcp/src/lib.rs:41-50이 `Inner`(runtime·경로)를 만들지만 store는 요청마다 연다(위 10곳). Vault는 열지 않는다(그래서 발견 1이 생겼다) | 아니다 |
| supervisor (`__supervise`) | 어댑터 선택은 cli/src/session_cmd.rs:107-114, Guard·CdpBrowser·Ops 조립은 broker/src/supervisor/mod.rs:144-152(어댑터 crate 안) | 둘로 나뉜다 (#11) |
| 키 저장소 선택 | vault/src/keystore.rs:145-160 `keystore_from_env`(라이브러리 안에서 env를 읽는다) | composition root 밖이다 (발견 10) |
| 확장 | adapters/chrome/background.ts, adapters/firefox/background.ts, 각 popup.ts | 그렇다 |
| design-serve | design-check/src/bin/design-serve.rs | 그렇다 |
