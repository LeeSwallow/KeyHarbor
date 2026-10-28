# 문서를 쓰는 자리

작업하면서 아키텍처와 결정을 기술 문서로 남긴다. 그림을 넣는다([visualize](visualize.md)).

| 어디 | 무엇 | 수명 |
|---|---|---|
| `docs/design/01`–`09` | 지금 시스템이 어떻게 생겼나 (제품, 구조, 보안 백서, 로드맵, 암호, 등록 흐름, 앱 UX) | 코드가 바뀌면 같은 커밋에서 고친다 |
| `docs/design/06-requirements-log.md` | 사용자 결정(R-), 설계자 판단(J-) | 뒤집힐 때까지 남는다. 뒤집으면 새 번호로 쓰고 옛 항목에 링크 |
| `docs/design/adr/` | 되돌리기 어려운 구조 결정 하나에 한 장 | 대체되면 "대체됨"으로 표시만 한다 |
| `docs/superpowers/specs/YYYY-MM-DD-<주제>-design.md` | 구현 전에 합의한 설계 | 구현하면서 바뀐 것은 design 문서로 옮긴다 |
| `docs/superpowers/plans/YYYY-MM-DD-<주제>.md` | 정해진 것을 어떤 순서로 구현하나 | 작업이 끝나면 고치지 않는다 |
| `design-system/` | 디자인 시스템 (토큰, 컴포넌트, 아이콘, 쇼케이스) | 코드처럼 관리한다 |
| `integrations/<이름>/README.md` | 확장·외부 연동의 구조와 설치 | 코드와 함께 |
| `contracts/` | 언어 사이의 계약 fixture (예: `session-names.json`을 Rust와 TypeScript 테스트가 함께 읽는다) | 규칙이 바뀌면 같은 커밋에서. 두 쪽 테스트가 같이 깨져야 한다 |
| `.sticky/roadmap.md` | 단계 체크리스트 | 단계가 끝날 때 |
| `docs/history/` (`prototype.md` 등 옛 문서) | 프로토타입 시절 자료 | 읽기만 한다 |

- 사용자가 정한 것은 `[사용자 결정 R-nn]`, 내가 정한 것은 `[설계자 판단]` 또는 J- 번호로 표시해 구분한다.
- 계획과 다르게 구현했으면 06에 J 항목을 쓰고 코드에는 번호만 남긴다.
- 동작이 바뀌면 README, `crates/README.md`, 해당 design 절을 같은 커밋에서 고친다(AGENTS.md).
- `sh scripts/check-docs.sh`가 링크를 확인한다.
