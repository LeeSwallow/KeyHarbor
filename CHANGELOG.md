# Changelog

중요한 변경을 모두 적는다. 형식은 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)를 따르고, GitHub 릴리스 노트는 PR 라벨로 만들어진다.

## [Unreleased]

### Added

- 로컬 암호화 금고와 `keyharbor vault` CLI: `init`, `add`, `edit`, `rm`, `list`, `import-env`, `export`·`import`(age 형식), `rotate-root`, `rotate-domain`.
- 자격 증명·결제·개인정보 세 도메인과 항목 종류 6가지(`login`, `secret`, `env`, `card`, `identity`, `address`).
- XChaCha20-Poly1305 필드 암호화, 도메인 키 envelope, HKDF 키 분리, 항목 메타데이터 HMAC, macOS Keychain 루트 키.
- 중단에 안전한 키 교체와 동시 쓰기 보호.

### Security

- CLI는 평문 비밀을 출력하지 않으며, 비밀 값을 명령행 인자로 받지 않는다.

