# ADR-0008. `rebuild/` 워크스페이스 위에 구현한다

| 항목 | 값 |
|---|---|
| 상태 | Proposed |
| 날짜 | 2026-10-28 |
| 관련 요구 | — |

## 맥락

프로토타입 `src/`는 단일 crate에 세션·감독·MCP가 몰려 있다. `rebuild/`는 domain·sqlite·cli로 나뉜 워크스페이스이고, `rebuild/ARCHITECTURE.md`의 의존 방향이 이번 설계와 같다.

## 결정

`rebuild/`를 기준으로 프로토타입 기능을 이식하고 새 crate(`kh-vault`, `kh-broker`, `kh-core`, `kh-mcp`, `kh-app`)를 더한다. 프로토타입은 이식이 끝날 때까지 유지한다. **[설계자 판단]** — 사용자가 뒤집을 수 있다.

## 결과

- 이식 비용이 든다. 기존 테스트 시나리오를 재현해 회귀를 막는다.
- 이식 완료 후 프로토타입 정리 방식은 사용자에게 묻는다.

## 이후

2026-10-29 (#12): 결정은 그대로이고 자리만 옮겼다. `rebuild/`의 워크스페이스는 저장소 루트(`Cargo.toml`, [`crates/`](../../../crates/README.md))로, `rebuild/ARCHITECTURE.md`는 [`crates/ARCHITECTURE.md`](../../../crates/ARCHITECTURE.md)로, 프로토타입 `src/`·`tests/`는 `prototype/`으로 갔다. 이 문서의 `rebuild/`는 당시 이름이다.
