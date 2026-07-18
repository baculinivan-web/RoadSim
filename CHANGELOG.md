# Changelog

Здесь фиксируются заметные изменения RoadSim. Записи группируются по версии, а
ещё не выпущенные изменения находятся в разделе `Unreleased`. Изменения public
schema, ruleset, metric definitions и protocol всегда указывают старую и новую
версию, compatibility и migration impact.

## Unreleased

### Added

- Инициализирован Rust workspace с точками входа desktop app и headless CLI.
- Зафиксированы Rust toolchain, MSRV и dependency policy.
- Добавлены OSS contribution/security/conduct документы и dual license
  `Apache-2.0 OR MIT`.
- Source-of-truth документы размещены в `docs/`; добавлен ADR template.
- Добавлена единая change policy для versioned contracts.
- Добавлены work-packet/PR templates и declarative label catalog.
- Добавлены реальные CI gates для format, Clippy, tests, rustdoc/Markdown links,
  dependency/source/license/advisory policy и трехплатформенной build/smoke matrix.
- Добавлен автоматизированный forbidden-dependency graph guard с regression
  fixtures, включая намеренно запрещенную связь domain → editor UI.
- Добавлена генерация CycloneDX SBOM и объединенного Rust/native license inventory
  как GitHub Actions artifact.

### Changed

Нет.

### Fixed

Нет.

### Security

- Включены deny-by-default checks неизвестных Cargo sources, RustSec advisories и
  лицензионной совместимости; GitHub Actions закреплены полными commit SHA.
