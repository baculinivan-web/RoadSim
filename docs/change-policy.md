# Политика versioned changes и changelog

> Связанные требования: `E00-T08`, `NFR-011`, `NFR-012`, `NFR-052`, `FR-026`,
> `FR-053`, `FR-055`; архитектурные решения `ADR-006`, `ADR-007`, `ADR-009`,
> `ADR-014`.

## Общие правила

- Один PR меняет один связный контракт или поведение.
- Breaking product decision сначала получает ADR и синхронное изменение
  `PROJECT_SPEC.md`, `ARCHITECTURE.md` и `IMPLEMENTATION_PLAN.md` при необходимости.
- Stable ID, удалённый из требования, schema, metric или rule, не переиспользуется.
- Unknown major version отклоняется стабильным diagnostic code; частичное чтение
  обязательных данных запрещено.
- Silent migration, ruleset update, metric reinterpretation и capability downgrade
  запрещены.
- Любое заметное изменение получает запись в `CHANGELOG.md` в том же PR.

`CHANGELOG.md` описывает эффект для пользователей и интеграторов, а не перечень
файлов. До релиза запись находится в `Unreleased`; при релизе она переносится под
immutable version и дату без переписывания истории предыдущих версий.

## Project/container schema

Breaking schema change требует новой major version. PR обязан включать:

- task/ADR с причиной и рассмотренными совместимыми вариантами;
- обновлённые schema и format docs;
- явную migration из каждой поддерживаемой версии и migration report;
- идемпотентность миграции и сохранение исходного artifact/backup;
- old/new/malformed corpus fixtures и compatibility tests;
- round-trip и semantic-hash evidence;
- понятную ошибку для неизвестной major version;
- security/resource-limit review для parser и migration;
- changelog с loss, unsupported и rollback information.

Additive optional field может быть minor change только если старый reader безопасно
его игнорирует по заранее спроектированному extension mechanism, а новый reader
имеет однозначный default. Новое обязательное поле не считается minor change.

## Ruleset

Опубликованный ruleset immutable. Изменение metadata, applicability, tolerance,
severity, evidence или check создаёт новую exact version и coverage diff. PR обязан
иметь официальный source metadata, amendment/clause, validity, domain owner и
positive/boundary/negative/not-applicable fixtures. Проект сохраняет прежний pin до
явной миграции; отсутствие реализованного rule никогда не превращается в `pass`.

## Metric definition и results schema

Metric definition включает stable `metric_id`, version, формулу, raw inputs,
units, nullability, aggregation, sampling и tolerance. Изменение любого поля,
способного изменить значение или интерпретацию, создаёт новую definition version.
Старые results продолжают ссылаться на старую версию и не пересчитываются молча.

PR включает independently verified fixture, schema metadata, compatibility test и
объяснение влияния на A/B comparison. Несовместимые definitions блокируют сравнение
либо требуют явного документированного преобразования.

## Worker/backend protocol

Protocol change обновляет schema version и обе стороны контракта под одним
contract owner. Обязательны handshake version/capability tests, mismatch/unknown
message, size limits, timeout, cancellation, sequence/idempotency и crash paths.
Minor negotiation разрешена только для явно optional capability; неизвестная
обязательная capability блокирует run до запуска worker.

Backend capability ID и его семантика стабильны. Удаление, переименование или
ослабление поведения является breaking change и требует migration/compatibility
решения. Unsupported feature никогда не удаляется из модели и не упрощается молча.

## Проверочный пример breaking change

Предложение «сделать обязательным новое поле `engineering_crs` в project schema»
проходит review только при отмеченных пунктах:

- [ ] есть новый major schema ID и ADR с причиной;
- [ ] определено преобразование старых проектов без догадки о CRS;
- [ ] неоднозначный старый проект получает diagnostic, а не silent default;
- [ ] исходный `.roadsim` сохраняется, migration создаёт новый artifact/report;
- [ ] есть old, migrated, ambiguous и unknown-major fixtures;
- [ ] round-trip сохраняет semantic hash для однозначного случая;
- [ ] format docs, corpus index и `CHANGELOG.md` обновлены;
- [ ] domain, architecture и security reviews назначены.

Если хотя бы один применимый пункт не выполнен, breaking change не готов к merge.
