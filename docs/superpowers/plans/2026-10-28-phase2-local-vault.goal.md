# GOAL: KeyHarbor 2단계 — 로컬 암호화 금고 구현

## 목표
`rebuild/` 워크스페이스에 세 도메인(자격 증명·결제·개인정보) 로컬 암호화 금고와 `keyharbor vault` CLI를 구현한다. 구현 계획 `docs/superpowers/plans/2026-10-28-phase2-local-vault.md`의 Task 1–10을 **순서대로 전부** 끝내고, 모든 검증을 통과하면 완료다.

## 먼저 읽을 것 (이 순서로)
1. `docs/superpowers/plans/2026-10-28-phase2-local-vault.md` — 작업 단위, 인터페이스, 테스트. **이 문서가 실행 기준이다.**
2. `docs/design/07-cryptography-design.md` — 키 계층·레코드 바이트 배치·AD의 정본
3. `docs/design/02-system-architecture.md` §2.7(도메인·필드 스키마·데이터 모델)
4. `docs/design/03-security-whitepaper.md` §3.4(Keychain)·§3.12(CLI 평문 없음)
5. `docs/design/adr/0011-domain-split.md`, `0012-crypto-scheme.md`
6. 기존 코드: `rebuild/crates/{domain,sqlite,cli}/src/*.rs` — 문체·오류 처리·테스트 방식을 그대로 따른다

## 절대 규칙
- 암호 알고리즘을 직접 구현하지 않는다. 계획의 Tech Stack에 있는 crate만 쓴다. 다른 crate를 추가하려면 이유를 커밋 메시지에 적는다.
- **평문 비밀을 stdout·stderr·로그·오류 메시지·SQLite 평문 컬럼에 쓰지 않는다.** CLI에 `show`·`reveal`·평문 export를 만들지 않는다.
- 비밀 값을 명령행 인자로 받지 않는다. 받는 경로는 에코 없는 프롬프트나 stdin JSON뿐이다.
- 키나 평문을 담는 타입은 `Zeroizing`으로 감싸고, `Debug` 출력은 `[REDACTED]`로 한다.
- Keychain 항목을 덮어쓰지 않는다. 키가 없을 때 새로 만들지 않는다.
- 테스트는 `KEYHARBOR_TEST_KEYSTORE_DIR`(파일 키 저장소)만 쓴다. 실제 Keychain에 접근하는 테스트는 `#[ignore]`로 둔다.
- 이 레포에서는 `moa:*` 스킬과 `moa-transition`을 쓰지 않는다.
- 설계 문서와 어긋나는 결정이 필요하면 코드를 바꾸기 전에 멈추고 보고한다. 설계를 조용히 바꾸지 않는다.

## 환경
- 매 셸 명령 앞에 `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"`를 둔다. cargo는 이 경로에만 있다.
- 작업 디렉터리는 `rebuild/`다. 시작 시 `cargo test --workspace`가 통과하는지 확인한다(기존 테스트 4개).

## Git
1. 레포에 커밋이 하나도 없다. 시작할 때 `main`에서 현재 파일 전체로 첫 커밋을 만든다. 메시지는 `chore: initial import of prototype, rebuild workspace, and design docs`다. `rebuild/target/`이 무시되는지 먼저 확인한다.
2. `feat/phase2-local-vault` 브랜치를 만들어 그 위에서 작업한다.
3. 계획의 각 Task 마지막 단계에서 커밋한다. push는 하지 않는다.

## 작업 방식 (Task마다 반복)
1. 실패하는 테스트를 먼저 쓴다. 계획에 적힌 테스트 이름과 단언을 그대로 쓴다.
2. 실행해서 **예상한 이유로** 실패하는지 확인한다.
3. 최소 구현을 한다.
4. 해당 crate 테스트, 그다음 `cargo test --workspace`를 실행한다.
5. `cargo fmt --all` 후 `cargo clippy --workspace --all-targets -- -D warnings`를 통과시킨다.
6. 커밋한다.
7. 가능하면 Task마다 새 검토 에이전트에게 diff와 계획의 해당 Task를 주고 검토를 받는다. 지적된 문제는 고친 뒤 다음 Task로 넘어간다.

## 사람이 필요한 지점 (여기서는 멈추고 요청한다)
- **Task 4 Step 4**와 **Task 10 Step 3**: 실제 macOS Keychain 확인 창이 뜰 수 있다. 명령을 준비해 두고, 사용자에게 실행과 "항상 허용" 클릭을 요청한다. 결과(창이 몇 번 떴는지, 재빌드 후 다시 떴는지)를 받아 기록한다. 사용자 응답 없이 이 단계를 건너뛰었다면, 건너뛰었다고 명시한다.
- 같은 테스트가 접근을 바꿔 세 번 시도해도 통과하지 않을 때
- 설계 문서끼리 모순을 발견했을 때

## 문서화 (작업과 함께)
- 구현하면서 실제로 정한 세부(정확한 crate 버전, API 선택, 스키마 변경)는 관련 설계 문서와 `docs/design/06-requirements-log.md` §6.3에 J-번호로 추가한다.
- Task 2에서 고정한 KAT hex 값을 `docs/design/07-cryptography-design.md` §7.11 아래 "부록: v1 KAT"로 옮긴다.
- 다이어그램을 바꾸면 Mermaid로 유지하고 렌더링이 되는지 확인한다.

## 완료 조건 (모두 참이어야 한다)
- [x] 계획의 Task 1–10 체크박스가 모두 완료다
- [x] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`가 모두 성공한다
- [x] `no_command_prints_plaintext`, `db_and_wal_never_contain_plaintext`, `export_is_age_and_contains_no_plaintext` 누출 테스트가 통과한다
- [x] `interrupted_root_rotation_recovers`가 failpoint 1·2·3 모두에서 통과한다
- [x] `vault show`가 존재하지 않는다
- [x] README와 설계 문서가 구현과 일치한다
- [x] Keychain 수동 확인 결과가 기록돼 있거나, 하지 못한 이유가 기록돼 있다 — 하지 못한 이유를 계획 파일 「실행 기록」과 03 §3.4에 기록

## 최종 보고 형식
1. 완료한 Task와 커밋 목록(해시·메시지)
2. 검증 명령과 결과 요약(테스트 개수, 실패 0)
3. 계획과 달라진 점과 그 이유
4. 사람이 확인해야 할 것(Keychain 실험 결과 등)
5. 다음 단계(1단계 브라우저 브로커 또는 3단계 치환·마스킹)를 시작하기 전에 결정할 것
