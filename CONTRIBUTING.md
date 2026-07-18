# Участие в разработке RoadSim

Спасибо за интерес к RoadSim. Проект принимает небольшие, проверяемые изменения,
которые сохраняют продуктовые и архитектурные границы.

## Перед началом работы

1. Полностью прочитайте `docs/PROJECT_SPEC.md`, `docs/ARCHITECTURE.md`,
   `docs/IMPLEMENTATION_PLAN.md` и применимые `AGENTS.md`.
2. Выберите один work packet с task ID, связанными `FR/NFR/ADR`, наблюдаемым
   acceptance criterion и явными границами `in/out`.
3. Проверьте существующий код, fixtures и локальные незакоммиченные изменения.
4. До изменения публичного API, schema, protocol, ruleset или metric definition
   выясните, нужен ли ADR, migration либо domain/security/license review.

Для versioned contracts применяйте checklist из `docs/change-policy.md`.

Не определяйте нормативную трактовку самостоятельно. Rule, численный предел и
область применимости требуют подтверждённых source metadata и domain owner.

## Разработка

- Используйте корневой Rust toolchain и не обходите workspace dependency policy.
- Сначала добавьте regression/acceptance test, когда это практически возможно.
- Не смешивайте изменение поведения с unrelated рефакторингом или форматированием.
- Обрабатывайте invalid input, unsupported behavior, cancellation и failure.
- Не добавляйте telemetry, сеть, новый `unsafe` или native dependency без review.
- Обновляйте docs, fixtures, schema/migration и changelog вместе с кодом.

Минимальные локальные проверки:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --workspace --locked
```

Дополнительные property, corpus, contract, golden, fuzz-smoke и benchmark jobs
выбираются по изменяемой области согласно `docs/IMPLEMENTATION_PLAN.md`.

## Pull request

PR должен содержать:

- work packet и связанные `FR/NFR/ADR/E*-T*`;
- scope и намеренно не реализованные части;
- изменение поведения и публичных контрактов;
- schema/ruleset/metric/protocol impact;
- выполненные тесты и результаты, а для UI — manual QA;
- determinism, performance, security и license considerations;
- известные ограничения и follow-up.

Один PR должен давать один связный reviewable результат. Golden outputs нельзя
обновлять только ради зелёного CI.

## Лицензирование вклада

Отправляя вклад, вы соглашаетесь лицензировать его на условиях проекта:
`Apache-2.0 OR MIT`. Добавляйте только код и данные, которые вы вправе
распространять; для fixtures указывайте происхождение, лицензию и units.
