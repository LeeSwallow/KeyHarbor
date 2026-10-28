# AGENTS.md

코딩 에이전트가 이 저장소를 바꾸기 전에 알아야 할 것. 사람용 설명은 [CONTRIBUTING.md](CONTRIBUTING.md)에 더 길게 있다.

## 이것은 무엇인가

KeyHarbor는 AI 에이전트를 위한 로컬 금고·브라우저 세션 기반이다. 릴리스되는 제품은 `rebuild/` Rust 워크스페이스의 `keyharbor` CLI이고, 저장소 루트 crate는 프로토타입이다([docs/prototype.md](docs/prototype.md)). 원칙은 [VISION.md](VISION.md), 설계는 [docs/design/](docs/design/README.md)에 있다.

## 끝내기 전에

```sh
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"   # 이 기계에서 cargo 위치
cd rebuild && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --locked
sh scripts/check-docs.sh && sh scripts/test-scripts.sh && sh scripts/check-layers.sh
```

모두 통과해야 한다. 테스트가 명세다: 동작을 바꾸려는 경우에만 테스트를 바꾸고, 그렇다고 말한다.

## 규칙

- **평문 비밀은 밖으로 나가지 않는다.** CLI 출력, 로그, 오류 메시지, SQLite 평문 컬럼, MCP 응답에 비밀 값을 쓰지 않는다. CLI에 `show` 같은 평문 출력 명령을 만들지 않는다(docs/design/03 §3.12). 비밀 값을 명령행 인자로 받지 않는다. 누출 테스트(`no_command_prints_plaintext` 등)를 지운다면 그것은 버그다.
- **암호 알고리즘을 직접 구현하지 않는다.** 형식과 키 계층은 [docs/design/07](docs/design/07-cryptography-design.md)이 정본이고, 바꾸면 KAT와 문서를 함께 바꾼다.
- **설계와 함께 바꾼다.** 동작이 바뀌면 README, `rebuild/README.md`, docs/design의 해당 절, 필요하면 ADR(`docs/design/adr/`)과 결정 기록(`docs/design/06`)을 같은 커밋에 갱신한다.
- **계층을 지킨다.** crate 사이 의존과 평문 경로 규칙은 [ADR-0014](docs/design/adr/0014-hexagonal-guard-layer.md)와 `scripts/check-layers.sh`에 있다. 평문 값이 필요하면 `keyharbor-guard`를 거친다.
- **의존성은 이유가 있어야 한다.** `rebuild/deny.toml`이 허용하는 라이선스여야 하고 `cargo deny check`를 통과해야 한다.
- **커밋**은 Conventional Commits, 한 커밋에 한 변경. AI 도구를 `Co-Authored-By`나 "Generated with" 줄로 적지 않는다 — 커밋하는 사람이 저자다. `git config core.hooksPath .githooks`로 훅을 켜 두면 그런 줄을 지워 준다.
- **먼저 묻는다**: push, `main` 병합, 태그, 브랜치 삭제, 실제 macOS Keychain에 쓰는 명령, 이 기계 밖에 영향을 주는 모든 일.
- 이 저장소에서는 `moa:*` 스킬을 쓰지 않는다.
