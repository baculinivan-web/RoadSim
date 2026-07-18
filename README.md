# RoadSim

RoadSim — открытое кроссплатформенное desktop-приложение для проектирования
небольших городских дорожных сетей и воспроизводимой микроскопической симуляции
автомобилей, автобусов, трамваев и пешеходов.

Проект находится на стадии foundation. Сейчас репозиторий содержит только
собираемый Rust workspace и минимальные точки входа `roadsim-app` и
`roadsim-cli`; редактор и simulation backend ещё не реализованы.

## Source of truth

Перед изменением кода полностью прочитайте документы в следующем порядке:

1. [PROJECT_SPEC.md](docs/PROJECT_SPEC.md) — продуктовые требования и границы MVP;
2. [ARCHITECTURE.md](docs/ARCHITECTURE.md) — слои, контракты и ADR;
3. [IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md) — work packets и gates;
4. [AGENTS.md](AGENTS.md) — обязательные правила работы в репозитории.

## Локальная проверка

Корневой `rust-toolchain.toml` автоматически выбирает поддерживаемый Rust.

```text
cargo build --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

Правила обновления toolchain и зависимостей описаны в
[dependency policy](docs/dependency-policy.md). Изменения публичных schema,
ruleset, metrics и protocol следуют [versioned change policy](docs/change-policy.md)
и фиксируются в [CHANGELOG.md](CHANGELOG.md).

## Участие в разработке

RoadSim принимает ограниченные, проверяемые изменения, связанные с task ID и
требованиями. Перед началом работы прочитайте [CONTRIBUTING.md](CONTRIBUTING.md).
Уязвимости сообщаются по [SECURITY.md](SECURITY.md), а взаимодействие участников
регулируется [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Лицензия

Основной код RoadSim предоставляется по вашему выбору на условиях
[Apache License 2.0](LICENSE-APACHE) или [MIT License](LICENSE-MIT). Лицензии и
обязательства сторонних компонентов учитываются отдельно.
