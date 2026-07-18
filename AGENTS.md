# AGENTS.md — правила работы над RoadSim

## 1. Область действия

Этот файл действует для всего репозитория RoadSim. Более вложенный `AGENTS.md` может уточнять локальные команды и ограничения, но не может отменять продуктовые требования, архитектурные инварианты, безопасность или требования к нормативной проверке.

RoadSim — OSS desktop-приложение для проектирования небольших городских дорожных сетей и микроскопической симуляции автомобилей, автобусов, трамваев и пешеходов.

## 2. Source of truth

Перед любой задачей полностью прочитайте:

1. `docs/PROJECT_SPEC.md` — продукт, пользователи, требования `FR-*`/`NFR-*`, MVP и критерии успеха;
2. `docs/ARCHITECTURE.md` — слои, контракты и решения `ADR-*`;
3. `docs/IMPLEMENTATION_PLAN.md` — эпики `E*-T*`, зависимости, PR gates и Definition of Done;
4. ближайшие к изменяемому коду вложенные `AGENTS.md`;
5. связанные ADR, схемы, metric definitions и fixtures.

При противоречии действует порядок: продуктовая спецификация → архитектура → план реализации → ADR/локальная документация → код. Не исправляйте противоречие догадкой: зафиксируйте его в issue или отдельном ADR.

## 3. Базовый технологический стек

- Rust stable и Cargo workspace;
- `winit` для оконного lifecycle;
- `wgpu` для GPU-rendering;
- `egui` для инструментального UI;
- собственные Design Model и Compiled Simulation Network;
- SUMO/libsumo в отдельном worker-процессе как первый backend;
- Arrow/Parquet для результатов;
- PROJ и изолированный GDAL worker для GIS;
- Wasmtime Component Model/WIT для будущих sandboxed-плагинов;
- Python SDK для автоматизации и аналитики, не для горячего timestep.

Не заменяйте согласованный стек без утвержденного ADR.

## 4. Неприкосновенные архитектурные границы

1. Design Model — единственный редактируемый источник истины.
2. SUMO XML и OpenDRIVE являются производными/interchange-форматами, а не внутренней базой.
3. Design Model и Compiled Simulation Network представлены разными типами и слоями.
4. UI изменяет модель только через типизированные domain commands.
5. Renderer получает read-only snapshots и не определяет инженерную геометрию.
6. `domain` не зависит от UI, GPU, SUMO, filesystem или async runtime.
7. libsumo types, указатели и идентификаторы не выходят за границу SUMO worker/adapter.
8. Backend обязан объявлять capabilities; неподдерживаемая функция блокируется до запуска.
9. Silent downgrade, удаление объекта или незаметное упрощение модели запрещены.
10. Плагины и AI возвращают proposed commands/diffs и не изменяют модель напрямую.

## 5. Порядок выполнения задачи

Перед изменением:

- определите связанный work packet, `FR/NFR`, `ADR` и epic/task ID;
- проверьте существующий код, схемы, fixtures и незакоммиченные изменения;
- сформулируйте минимальный наблюдаемый результат и границы `in/out`;
- выясните, требуется ли ADR, migration, domain, security или license review;
- по возможности сначала добавьте failing/regression test.

Во время изменения:

- делайте один связный результат за PR;
- сохраняйте публичные границы и stable IDs;
- обрабатывайте happy path, invalid input, unsupported, cancellation и failure;
- не блокируйте UI thread длительными операциями;
- используйте bounded queues, sizes, timeouts и memory limits;
- добавляйте object references и стабильные diagnostic codes;
- не меняйте unrelated пользовательский код или форматирование.

После изменения:

- выполните релевантные unit, property, contract, integration и scenario tests;
- проверьте format/lints/docs links;
- обновите schema, migration, fixtures, changelog и документацию при необходимости;
- сообщите выполненные acceptance criteria, тесты, ограничения и follow-up.

## 6. Команды и изменения модели

- Команды отражают domain intent: `CreateRoad`, `SplitCorridor`, `AddCrossing`, `SetPhaseDuration`.
- Один пользовательский жест должен commit-иться как одна логическая undoable-команда.
- Preview не меняет Design Model.
- Failed transaction не изменяет model revision или semantic hash.
- `apply(command)` + `apply(inverse)` должны восстанавливать semantic state.
- Undo/redo сохраняет UUID и корректно восстанавливает связи.
- Import, autofix и AI-операция формируют proposed command batch с предварительным diff.

## 7. Геометрия и единицы

- Design/geometric predicates используют локальную метрическую CRS и `f64`.
- Внутренние длины — метры, скорости — м/с, время симуляции — integer ticks, углы — радианы.
- Сериализованные единицы фиксируются схемой и не зависят от локали.
- Tolerance задается контекстом операции; глобальная «магическая точность» запрещена.
- Широта/долгота не используются непосредственно для offsets/intersections.
- Любой repair возвращает evidence и набор proposed changes.
- Near-degenerate input не должен вызывать panic, NaN или неограниченное выделение памяти.

## 8. Детерминизм

- Root seed обязателен и записывается в run manifest.
- Все RNG substreams выводятся из стабильного `(algorithm_version, root_seed, purpose, entity_id)`.
- Не используйте wall clock, случайный hash seed или порядок выполнения потоков в model behavior.
- Порядок, влияющий на результат, задается стабильным ключом.
- Parallel reductions имеют фиксированный порядок или документированный tolerance.
- Backend, adapter, ruleset, schemas, metric definitions и input hashes записываются в manifest.
- Golden output нельзя обновлять только ради зеленого CI; нужен объяснимый diff и review.

## 9. ГОСТ from day one

- Coding agent не определяет и не «уточняет» нормативное требование самостоятельно.
- Для rule обязательны: официальный source metadata, редакция/изменения, пункт, область применимости, единицы, tolerance, severity, evidence и domain owner.
- Каждый rule имеет positive, boundary, negative и not-applicable fixtures.
- Отсутствие finding не означает соответствие непроверенным требованиям.
- Coverage различает `implemented`, `partial`, `manual` и `not_evaluated`.
- Autofix — только reviewed proposed command с preview и undo.
- Полные тексты стандартов не помещаются в OSS-репозиторий без подтвержденных прав.
- Ruleset закрепляется exact version и не обновляется незаметно.

## 10. Форматы и совместимость

- `.roadsim` — недоверенный ZIP-контейнер: проверяйте path traversal, duplicates, symlinks, zip bombs, размеры и hashes.
- Чтение выполняется в staging; активный проект заменяется только после полной проверки.
- Запись — temporary file, flush и atomic rename.
- Не сериализуйте layout Rust-структур как публичный формат.
- Breaking schema change требует major version, migration, corpus test и changelog.
- OpenDRIVE importer/exporter сопровождается loss/unsupported report.
- Metric definition version неизменяема; старые результаты нельзя молча интерпретировать по новой формуле.
- Partial/cancelled run никогда не маркируется `Completed`.

## 11. Worker и IPC

- Один SUMO worker владеет одной активной libsumo session, пока не доказана иная безопасная модель.
- Worker запускается с отдельным каталогом и минимальными правами.
- Протокол имеет version handshake, request/session IDs, sequence numbers, лимиты сообщений, timeouts и watchdog.
- Не открывайте сетевой listener по умолчанию.
- Запускайте процессы через массив аргументов, без shell interpolation.
- Состояние агентов передается пакетами/SoA; IPC на каждого агента в каждом tick запрещен.
- Visual frames могут пропускаться под backpressure; metrics и terminal events теряться не могут.
- Worker crash оставляет editor живым, project state неизменным, run — `Failed`/`Incomplete`.

## 12. Безопасность

Считайте недоверенными `.roadsim`, ZIP, JSON, XML, OpenDRIVE, GIS, assets, rulesets, plugins и worker output.

- XML: без DTD, external entities и сетевого доступа.
- Parsers: depth/count/size/time limits и fuzz corpus.
- Пути: только нормализованные относительные пути внутри staging/workdir.
- Плагины: deny-by-default capabilities, fuel/time/memory limits.
- Секреты и полные пользовательские пути не попадают в логи/diagnostic bundles.
- Новый `unsafe` требует изоляции, safety comment, tests и отдельного review.
- Новая зависимость требует необходимости, версии, license и vulnerability review.
- Telemetry и отправка пользовательских данных отсутствуют без отдельной opt-in политики.

## 13. Тестирование

Минимально выбирайте подходящий уровень:

- unit tests — value types, commands, curves, rules;
- property tests — geometry, serialization, undo/redo;
- golden/snapshot — CSN, diagnostics, interchange, metrics;
- backend contract — lifecycle/capabilities/errors;
- integration — app/worker/storage/Parquet;
- scenario — инженерное поведение;
- fuzz/security — parsers, migrations, protocol;
- performance — compile/render/IPC/result writing.

Обязательные свойства:

- save/open сохраняет semantic hash;
- full compile и incremental compile семантически эквивалентны;
- unsupported capability выявляется до run;
- одинаковые inputs/seed/version дают accepted deterministic result;
- import failure, rule и renderer не изменяют Design Model;
- finding order стабилен;
- worker crash не повреждает проект;
- incomplete run не публикуется как завершенный.

## 14. PR и Definition of Done

В PR укажите:

- work packet и связанные `FR/NFR/ADR/E*-T*`;
- scope и намеренно не реализованные части;
- изменение поведения и публичных контрактов;
- schema/ruleset/metric/protocol impact;
- выполненные команды тестирования и результаты;
- manual QA для UI;
- performance/security/determinism considerations;
- follow-up и известные ограничения.

Задача завершена, когда:

- acceptance criteria наблюдаемо выполнены;
- happy/failure/cancel/unsupported paths покрыты;
- format/lints/tests зеленые;
- diagnostics имеют стабильный код и object references;
- docs, fixtures, migration и changelog синхронизированы;
- нет скрытой зависимости от UI, SUMO или платформы;
- нет silent fallback;
- соответствующий domain/architecture/security review получен.

## 15. Рекомендованные команды проверки

Конкретные команды уточняются после создания репозитория и локальными `AGENTS.md`. Базовый набор:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test -p <affected-crate>
cargo deny check
```

Для changes в geometry, storage, worker, rules или results дополнительно запускайте соответствующие property, corpus, contract, golden, fuzz-smoke и benchmark jobs, указанные в `docs/IMPLEMENTATION_PLAN.md`.

## 16. Когда остановиться и запросить решение

Остановитесь и создайте вопрос/ADR, если:

- требуется изменить MUST-требование или границу MVP;
- есть два несовместимых способа изменить публичный API/schema;
- нормативный источник или трактовка не подтверждены;
- импорт требует необъявленной потери данных;
- действие расширяет права worker/plugin, добавляет сеть/telemetry или меняет license obligations;
- невозможно сохранить данные/детерминизм без breaking change;
- вы обнаружили пользовательские изменения, которые нельзя безопасно обойти.

Не останавливайтесь из-за локальной неясности, которую можно разрешить чтением кода, tests, schemas или официальной документации зависимости.
