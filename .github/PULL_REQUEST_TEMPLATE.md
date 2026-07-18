## Work packet

- Packet/task ID:
- Requirements: `FR-*`, `NFR-*`, `ADR-*`, `E*-T*`
- Closes:

## Scope

Что входит в PR:

Что намеренно не входит:

## Изменение поведения и контрактов

Опишите наблюдаемое поведение до/после и затронутые public API.

- Schema impact: none / compatible / breaking + version/migration
- Ruleset impact: none / new immutable version + domain review
- Metric impact: none / new definition version
- Protocol impact: none / negotiated / breaking

## Проверка

Выполненные команды и результаты:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

Manual QA для UI (или `N/A`):

## Инженерные considerations

- Determinism:
- Security/resource bounds:
- Performance:
- Licensing/provenance:
- Failure/cancel/unsupported paths:

## Checklist

- [ ] PR даёт один связный reviewable результат.
- [ ] Acceptance criteria work packet выполнены наблюдаемо.
- [ ] Diagnostics имеют стабильные codes и object refs, где применимо.
- [ ] Нет silent fallback/downgrade и запрещённых dependency edges.
- [ ] Tests покрывают happy path и существенные failure paths.
- [ ] Docs, fixtures, migration и changelog синхронизированы.
- [ ] Golden outputs не обновлены только ради зелёного CI.
- [ ] Назначены необходимые domain/architecture/security/license/UX reviewers.

## Ограничения и follow-up

Перечислите известные ограничения, риски и отдельные последующие packets.
