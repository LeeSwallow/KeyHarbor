# KeyHarbor: 재구현

이 디렉터리는 기존 프로토타입과 별도의 데이터 경로를 쓰는 새 Rust 워크스페이스다. 아직 재구현 중이며 브라우저 실행, 인증 인계, 금고 및 MCP 연동은 구현되지 않았다. 기존 데이터를 자동으로 가져오거나 변경하지 않는다.

```sh
cargo test --workspace
cargo run -p keyharbor -- create work
cargo run -p keyharbor -- list
```

기본 데이터 경로는 `~/.keyharbor-rebuild`이며 `--data-dir` 또는 `KEYHARBOR_HOME`으로 변경할 수 있다. SQLite 메타데이터는 암호화되지 않는다. 이름에 비밀을 입력하지 않는다.
