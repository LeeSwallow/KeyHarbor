Closes #

## Why

<!-- 무엇을 왜 바꾸는지 한두 문장. -->

## How I checked

<!-- 실행한 것: 추가한 테스트, 수동 확인. AI 에이전트가 일부를 썼다면 직접 읽고 확인한 것을 적어 주세요. 비밀 값은 붙여 넣지 마세요. -->

## Checklist

- [ ] 제목이 릴리스 노트 한 줄로 읽히는 Conventional Commit이다 ("feat: import .env files", not "fix bug")
- [ ] 종류 라벨 하나: feature, enhancement, bug, breaking change, docs, plugin, chore, security
- [ ] `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` 통과
- [ ] 비밀 값이 출력·로그·오류·DB로 나가지 않는다 (누출 테스트 유지 또는 추가)
- [ ] 동작이 바뀌었다면 README, `crates/README.md`, docs/design(필요하면 ADR)을 함께 고쳤다
- [ ] 이슈 없는 작은 수정이면 no-issue 라벨
