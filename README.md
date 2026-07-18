# RoadSim

RoadSim — открытое кроссплатформенное desktop-приложение для проектирования
небольших городских дорожных сетей и воспроизводимой микроскопической симуляции
автомобилей, автобусов, трамваев и пешеходов.

Проект находится на ранней стадии M1. Репозиторий содержит собираемый Rust
workspace, минимальные точки входа `roadsim-app`/`roadsim-cli` и первые
backend-independent контракты Design Model: typed IDs, единицы, simulation ticks,
project metadata, CRS, reference lines, corridor cross-sections и atomic typed
commands с bounded undo/redo, а также первые junction/pedestrian/rail semantics.
Design Model также содержит минимальные traffic-control contracts для знаков,
разметки, стоп-линий и фиксированных сигнальных программ, а project root —
интервальный demand и воспроизводимые scenario/experiment definitions.
Project также хранит exact ruleset pin и hash-bound audited exceptions без
встроенной трактовки нормативных требований. `roadsim-app` уже запускает первый
нативный GPU/egui shell со статическим дорожным viewport; редактирование и
simulation backend ещё не подключены.

Прямой corridor с постоянным cross-section уже детерминированно компилируется в
отдельный immutable Compiled Simulation Network. Кривые и переменный профиль на
этом этапе отклоняются явной diagnostic, а не упрощаются молча.

Для тестирования backend boundary есть deterministic in-memory backend с явным
seed, capability preflight и lifecycle; desktop controls подключаются следующим
срезом. Это test backend, а не скрытая замена SUMO.

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
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --locked --no-deps
python3 scripts/ci/check_markdown_links.py --root .
```

Правила обновления toolchain и зависимостей описаны в
[dependency policy](docs/dependency-policy.md). Изменения публичных schema,
ruleset, metrics и protocol следуют [versioned change policy](docs/change-policy.md)
и фиксируются в [CHANGELOG.md](CHANGELOG.md).

CI, трехплатформенная matrix, архитектурный dependency guard и supply-chain
artifacts описаны в [CI guide](docs/ci.md). Полный dependency/license/security
gate дополнительно требует закрепленные `cargo-deny` и `cargo-cyclonedx` из этого
руководства.

## Участие в разработке

RoadSim принимает ограниченные, проверяемые изменения, связанные с task ID и
требованиями. Перед началом работы прочитайте [CONTRIBUTING.md](CONTRIBUTING.md).
Уязвимости сообщаются по [SECURITY.md](SECURITY.md), а взаимодействие участников
регулируется [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Лицензия

Основной код RoadSim предоставляется по вашему выбору на условиях
[Apache License 2.0](LICENSE-APACHE) или [MIT License](LICENSE-MIT). Лицензии и
обязательства сторонних компонентов учитываются отдельно.
