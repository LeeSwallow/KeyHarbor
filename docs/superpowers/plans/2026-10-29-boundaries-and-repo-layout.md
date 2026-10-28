# 경계 정리와 저장소 재배치 계획 (#10, #11, #12)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. Steps use `- [ ]`.
> 규칙: [.claude/rules/writing-code.md](../../../.claude/rules/writing-code.md). 동작은 바꾸지 않는다. 기존 테스트의 이름과 단언을 그대로 두고, 테스트가 이 정리의 명세다.

**Goal:** 표면(MCP·CLI)이 유스케이스만 부르게 하고(#10), 감독자를 broker에서 떼어 내고(#11), 저장소 루트를 계층이 보이게 다시 놓는다(#12).

**언제:** 3단계 강화 브랜치와 디자인 계층화·확장 브랜치가 `main`에 병합된 직후, 4단계 전에 한다. 시작하기 전에 그 시점의 코드로 전체 분석(비즈니스 로직과 네트워크·I/O 인터페이스의 분리)을 한 번 더 하고, 새로 나온 항목은 이 계획에 Task로 더한다.

## 순서

```mermaid
flowchart LR
  A["전체 분석<br/>(새 리뷰어, 읽기 전용)"] --> B["#10 표면은<br/>유스케이스만"]
  B --> C["#11 crates/supervisor<br/>분리"]
  C --> D["#12 루트 재배치<br/>(파일 이동만)"]
  D --> E["경로·문서·CI<br/>일괄 갱신"]
```

경계부터 고치고 이동은 마지막에 한다. 이동 커밋에는 내용 변경을 섞지 않아야 `git log --follow`와 리뷰가 읽힌다.

## Task 0: 전체 분석

- 코드를 처음 보는 리뷰어 subagent(가장 강한 모델, 읽기 전용)에게 맡긴다.
- 대상 인터페이스: CLI, MCP stdio, control socket, CDP 파이프, Native Messaging, SQLite, Keychain, 자식 프로세스.
- 확인할 것:
  - 안쪽(domain·crypto·vault·core·guard)이 직렬화·전송·프로세스를 직접 다루는지
  - 판정이 어댑터에 새어 있는지
  - 같은 규칙이 두 벌 있는지
  - wire 타입이 안쪽 타입에 섞였는지
  - 확장 domain이 브라우저 API에 의존하는지
  - check-layers가 빠뜨린 규칙이 있는지
- 결과는 이 문서 끝 "분석 결과" 절에 표로 적고, 항목마다 Task 번호를 단다.

## Task 1 (#10): 표면은 유스케이스만

- [ ] 재현: `scripts/check-layers.sh`에 규칙 "`keyharbor-mcp`는 `keyharbor-sqlite`에 의존하지 않는다"와 "cli의 `*_cmd.rs`는 `SqliteStore`의 저장소 메서드를 부르지 않는다(조립부 `main.rs`·`compose.rs`만 연다)"를 먼저 넣고 실패를 본다.
- [ ] core에 빠진 유스케이스를 더한다: `session::list`, `audit::list`, 정책 규칙 읽기. MCP `session_*` 도구는 `core::session`을 부른다.
- [ ] 조립을 한 곳으로 모은다: `cli/src/compose.rs`가 저장소·금고·guard를 만들고, MCP 서버는 생성자로 받는다(`McpServer::new(deps)`). 요청마다 저장소를 열지 않는다.
- [ ] 검사 통과 후 커밋 `refactor: surfaces call use cases, one composition root`.

## Task 2 (#11): `crates/supervisor`

- [ ] check-layers에 "broker는 domain에만 의존한다"를 넣고 실패를 본다.
- [ ] `broker/src/supervisor/`, `control`, `runtime`의 프로세스 부분을 새 crate `keyharbor-supervisor`로 옮긴다. broker에는 CDP 어댑터(`cdp/`, `browser/`)와 포트 구현만 남긴다.
- [ ] 실제 Chrome 테스트까지 통과하면 커밋 `refactor: supervisor crate composes broker and guard`.
- [ ] ADR-0014 crate 지도, 02 §2.4, `rebuild/README.md`를 고친다.

## Task 3 (#12): 루트 재배치

`git mv`만 쓰는 이동 커밋 하나, 경로를 고치는 커밋 하나로 나눈다.

| 지금 | 다음 |
|---|---|
| `rebuild/Cargo.toml`, `rebuild/Cargo.lock`, `rebuild/deny.toml`, `rebuild/crates/*` | `Cargo.toml`, `Cargo.lock`, `deny.toml`, `crates/*` |
| `rebuild/README.md` | `crates/README.md` |
| 루트 `Cargo.toml`·`Cargo.lock`·`src/`·`tests/` (프로토타입) | `prototype/` |
| `design/` | `design-system/` |
| `docs/architecture-proposal.md`, `implementation-roadmap.md`, `open-source-research.md`, `openkeylet-integration.md`, `prototype.md` | `docs/history/` |
| `ROADMAP.md`, `.sticky/roadmap.md`, `docs/implementation-roadmap.md` | `ROADMAP.md` 하나가 정본, `.sticky/roadmap.md`는 같은 체크리스트 |

- [ ] 이동 커밋: `chore: move workspace to the repository root`.
- [ ] 경로 커밋에서 함께 고친다:
  - `.github/workflows/*`의 `working-directory`, `cd rebuild`
  - `scripts/*.sh`
  - `.goreleaser.yaml`
  - `AGENTS.md`의 "끝내기 전에"
  - `.claude/rules/*`, `CONTRIBUTING.md`, README와 `site/`의 빌드 안내
  - docs 링크 전부
  - 확장 빌드가 가져오는 디자인 경로, design-check가 읽는 경로
  - 커밋 이름: `chore: update paths after the move`
- [ ] 프로토타입 CI job은 `prototype/`에서 돌게 한다. 프로토타입을 지울지는 정하지 않는다(사용자에게 묻지 않고 유지).
- [ ] `sh scripts/check-docs.sh`로 깨진 링크가 0인지 확인한다.

## 끝나는 조건

```
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --locked && cargo deny check
sh scripts/check-docs.sh && sh scripts/test-scripts.sh && sh scripts/check-layers.sh && sh scripts/check-release-features.sh && sh scripts/check-design.sh
(cd integrations/browser-extension && npm ci && npm run check)
```

- Chrome 실제 테스트 환경 변수를 켠 상태로 모두 통과해야 한다.
- 테스트 개수는 정리 전과 같거나 많아야 하고, 이름 목록을 전후로 비교한다.
- 이슈마다 PR 하나: #10 → #11 → #12 순서로 squash 병합한다.

## 분석 결과

전문: [2026-10-29-boundary-analysis.md](2026-10-29-boundary-analysis.md) (main @ d7953bc). Critical 0, Important 19, 평문 유출 경로 없음.

| 갈래 | 이슈 | PR 순서 |
|---|---|---|
| 표면(MCP·CLI)에 있는 판정·유스케이스, `vault_list`의 MAC 검증 누락, 조립 위치 | #10 (범위 확장) | 1 |
| 여러 벌인 domain 어휘: placeholder, 요청자 이름, 설정 레지스트리, 상태 enum, TTL | #18 | 2 |
| supervisor 분리, control socket wire 타입, mcp의 broker 내부 의존 | #11 (범위 확장) | 3 |
| 안쪽 crate의 I/O: keystore crate, 파일·잠금, 시계, 난수, 브라우저 target 값 타입 | #19 | 4 |
| 확장 wire 타입 위치, 세션 이름 규칙 공유, 구조 테스트 | #20 | 5 |
| 루트 재배치 | #12 | 6 |

- 각 PR은 분석 보고서의 check-layers 규칙(A–N) 중 자기 몫을 먼저 넣고 실패를 본 뒤 고친다.
- 크기 기준을 넘는 파일·함수(`vault_cmd.rs`, `session_cmd::run` 등)는 그 코드를 옮기는 PR에서 같이 나눈다.
- 유지할 것: `PlaintextAccess`·`GuardedBrowser`, 검사 체인 공유, broker의 CDP 번역, SQLite 컨텍스트, 확장의 검증 한 곳.
