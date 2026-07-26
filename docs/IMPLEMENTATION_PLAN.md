# План реализации RoadSim

> Статус: исполнимый план для команды и frontier-model coding agents
> Версия документа: 0.1.0
> Связанные документы: [PROJECT_SPEC.md](PROJECT_SPEC.md), [ARCHITECTURE.md](ARCHITECTURE.md)

## 1. Назначение плана

Этот документ переводит требования и архитектуру RoadSim в последовательность эпиков, задач, проверяемых результатов и PR. Он не заменяет `PROJECT_SPEC.md` и `ARCHITECTURE.md`:

- `PROJECT_SPEC.md` отвечает, какой продукт и поведение нужны;
- `ARCHITECTURE.md` задает границы и технические инварианты;
- этот файл определяет порядок, зависимости и критерии завершения.

План организован не по календарным неделям, а по **контрольным точкам**. Оценки времени до появления baseline ненадежны; команда планирует capacity после декомпозиции work packets. Ранний вертикальный срез имеет приоритет над параллельной реализацией всех подсистем.

## 2. Правила работы для ИИ-агентов

### 2.1. Обязательный порядок перед изменением

Каждый coding agent должен:

1. прочитать три source-of-truth документа и ближайшие `AGENTS.md`;
2. определить связанные `FR-*`, `NFR-*`, `ADR-*`, epic/task ID;
3. исследовать существующий код, схемы, fixtures и незакоммиченные изменения;
4. сформулировать минимальный вертикальный результат и список файлов;
5. проверить, не требует ли решение нового ADR, schema migration или domain-owner review;
6. сначала добавить/уточнить acceptance test для исправления или поведения, если это практически возможно;
7. внести ограниченное изменение и выполнить релевантные проверки;
8. обновить документацию/fixtures/changelog в том же PR.

### 2.2. Запреты

Агент не должен:

- менять публичный формат, metric definition, ruleset semantics или backend contract «по ходу» без явной фиксации;
- превращать SUMO/OpenDRIVE/UI objects в source of truth;
- молча упрощать неподдерживаемую модель;
- придумывать численные нормативные ограничения или трактовку ГОСТ;
- обновлять golden outputs только для получения зеленого CI;
- добавлять `unsafe`, сетевой доступ, telemetry или native plugin API без отдельного review;
- смешивать механический рефакторинг и изменение поведения в одном PR;
- исправлять несвязанные пользовательские изменения;
- объявлять задачу завершенной без failure/cancel/unsupported path;
- использовать wall clock, unordered iteration или незафиксированный RNG в model behavior.

### 2.3. Формат work packet

Перед передачей задачи агенту maintainer создает issue/work packet:

```yaml
id: WP-<epic>-<number>
title: <один проверяемый результат>
source_requirements: [FR-..., NFR-..., ADR-...]
scope:
  in: [<точно входит>]
  out: [<точно не входит>]
dependencies: [<task/PR/schema>]
affected_areas: [<crates, schemas, fixtures>]
deliverables: [<code/tests/docs/data>]
acceptance:
  - <наблюдаемый критерий>
test_commands: [<команды или CI jobs>]
risks: [<determinism/security/licensing/performance>]
reviewers: [domain, architecture, security, UX]
```

Не следует назначать одному агенту «реализовать редактор» или «сделать симуляцию». Хороший packet дает один связный результат, обычно помещается в один reviewable PR и не требует скрытых решений.

### 2.4. Отчет агента

В завершении агент сообщает:

- какой результат готов;
- какие requirements/acceptance выполнены;
- измененные публичные contracts;
- выполненные тесты и их результаты;
- оставшиеся ограничения/риски;
- нужны ли migration, manual QA, domain review или follow-up.

## 3. Стратегия поставки

Работа идет через вертикальные срезы:

```text
сохранить простую модель
  → показать ее в viewport
  → отредактировать через command
  → скомпилировать в CSN
  → проверить rule
  → скомпилировать в SUMO
  → запустить worker
  → показать frame
  → записать metric/Parquet
  → сравнить variant A/B
```

Каждый срез должен использовать будущие границы, но минимальную функциональность. Заглушки допустимы только как явно помеченные test doubles; они не должны создавать ложное ощущение поддерживаемой функции.

## 4. Контрольные точки

| Milestone | Результат | Exit gate |
|---|---|---|
| M0 — Foundation | собираемый OSS workspace и CI | один commit проходит gates на трех ОС |
| M1 — Persistent Domain | Design Model, commands, `.roadsim` v1 | модель создается, undo и round-trip подтверждены |
| M2 — Editable Viewport | интерактивный 2D-редактор базовой дороги | пользователь рисует и сохраняет коридор/полосы |
| M3 — Compiled Intersection | CSN, перекресток и первые rules | четырехсторонний узел компилируется с diagnostics |
| M4 — First Simulation | SUMO worker и собственная визуализация | авто и пешеход проходят эталонный перекресток |
| M5 — Multimodal Scenario | demand, signals, bus и tram happy path | основные UC-03…05 работают или честно блокируются |
| M6 — Comparison Workflow | variants, experiments, metrics, Parquet | A/B сравнение воспроизводимо через UI и CLI |
| M7 — Interchange & Extensibility | OpenDRIVE subset, GIS, preview SDK boundaries | round-trip/loss report и isolated imports |
| M8 — MVP Release Candidate | hardening, packages, docs, pilot | выполнены 12 критериев приемки MVP |

Milestone закрывается только демонстрацией артефакта и автоматизированным exit gate. Процент выполненных задач не заменяет работающий сценарий.

## 5. Рекомендуемая структура репозитория

Структура соответствует `ARCHITECTURE.md`; создавать все crates заранее не обязательно.

```text
roadsim/
├── Cargo.toml
├── rust-toolchain.toml
├── README.md
├── CONTRIBUTING.md
├── SECURITY.md
├── LICENSE-APACHE
├── LICENSE-MIT
├── docs/
│   ├── PROJECT_SPEC.md
│   ├── ARCHITECTURE.md
│   ├── IMPLEMENTATION_PLAN.md
│   ├── adr/
│   ├── formats/
│   └── metrics/
├── crates/
│   ├── roadsim-types/
│   ├── roadsim-domain/
│   ├── roadsim-commands/
│   ├── roadsim-geometry/
│   ├── roadsim-compiler/
│   ├── roadsim-compiled-network/
│   ├── roadsim-rules-*/
│   ├── roadsim-backend-api/
│   ├── roadsim-backend-sumo/
│   ├── roadsim-worker-protocol/
│   ├── roadsim-storage/
│   ├── roadsim-results/
│   ├── roadsim-import-export/
│   ├── roadsim-renderer/
│   ├── roadsim-editor-ui/
│   ├── roadsim-application/
│   ├── roadsim-cli/
│   └── roadsim-app/
├── workers/
│   ├── sumo-worker/
│   └── gdal-worker/
├── schemas/
├── rulesets/
├── fixtures/
├── benchmarks/
├── examples/
├── packaging/
├── python/
├── wit/
└── .github/workflows/
```

## 6. Общая Definition of Ready

Задача готова к реализации, если:

- есть task ID, цель и связанные требования;
- перечислены границы in/out;
- зависимости merged или предоставлен стабильный mock/contract;
- acceptance наблюдаем и не сформулирован как «код написан»;
- понятны fixtures/test data и права на них;
- неизвестные архитектурные решения закрыты ADR/spike;
- для нормативной задачи есть подтвержденная трактовка domain owner;
- для внешней библиотеки зафиксирована версия и лицензионная оценка.

## 7. Общая Definition of Done

Любая задача завершена только если:

1. Код собран и отформатирован, lints не ухудшены.
2. Unit/integration/contract tests покрывают happy path и существенные failure paths.
3. Для исправления есть regression test.
4. Публичные API, schema, errors и units документированы.
5. Determinism, security, cancellation и resource bounds рассмотрены.
6. Нет silent fallback; unsupported behavior диагностируется.
7. Fixtures, golden data и docs обновлены осознанно.
8. Performance горячего пути измерен либо подтвержден как несущественный.
9. Изменения форматов имеют migration/compatibility test и changelog.
10. Изменения ruleset одобрены domain reviewer и отражены в coverage manifest.
11. Изменения worker/protocol прошли crash/timeout/version mismatch tests.
12. PR ограничен задачей, имеет понятное описание и review evidence.

## 8. Epic E00 — Управление проектом и OSS foundation

**Цель:** создать воспроизводимую основу, на которой агенты не расходятся в соглашениях.

| ID | Статус | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|---|
| E00-T01 | ✅ Готово | Создать mono-repo и Cargo workspace | нет | root manifests, минимальные crates | `cargo build/test` проходит на чистом clone |
| E00-T02 | ✅ Готово | Зафиксировать Rust toolchain и dependency policy | T01 | toolchain, lockfile policy, MSRV policy | CI использует ту же версию; обновление описано |
| E00-T03 | ✅ Готово | Добавить OSS документы | T01 | licenses, README, CONTRIBUTING, SECURITY, CoC | ссылки валидны; contribution path понятен |
| E00-T04 | ✅ Готово | Перенести source-of-truth docs и ADR template | T01 | `docs/`, ADR-0000 | doc link check проходит |
| E00-T05 | 🟡 Реализовано² | Настроить CI PR gates | T01–T03 | fmt, clippy, test, docs, deny/audit jobs | намеренно сломанный fixture блокирует PR |
| E00-T06 | 🟡 Реализовано² | Настроить cross-platform matrix | T05 | Windows/macOS/Linux jobs | hello app/CLI собираются на всех runners |
| E00-T07 | ✅ Готово¹ | Добавить issue/PR templates и labels | T03 | work packet template | PR требует requirement/task IDs |
| E00-T08 | ✅ Готово | Ввести changelog/schema/ruleset change policy | T04 | policies | sample breaking change проходит checklist |
| E00-T09 | ✅ Готово | Dependency graph guard | T01 | forbidden-dependency test/tool | UI dependency в domain демонстрационно отклоняется |
| E00-T10 | 🟡 Реализовано² | Базовый SBOM и license inventory | T05 | CI artifact | список включает Rust/native dependencies |

¹ Issue/PR templates и declarative label catalog готовы; labels применяются к
GitHub после настройки remote/hosting. Проверка CI pin из E00-T02 выполняется в
E00-T05.

² Workflow и локальные acceptance checks готовы: exact Rust 1.88.0 проходит
quality gates, forbidden fixture отклоняется, а SBOM/license inventory локально
сгенерирован. Статус меняется на `✅ Готово` только после первого green hosted run
на Ubuntu/Windows/macOS и скачивания CI artifact; текущий checkout не имеет Git
remote, поэтому это evidence пока невозможно получить. Milestone M0 остаётся
открытым только до этого внешнего подтверждения T05/T06/T10.

**Результат M0:** skeleton app и CLI запускаются; CI зелен на трех ОС; source of truth находится в репозитории.

## 9. Epic E01 — Базовые типы и Design Model

**Цель:** получить backend-independent семантическую модель с единицами и стабильными ID.

| ID | Статус | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|---|
| E01-T01 | ✅ Готово | Typed IDs и object references | E00 | `roadsim-types` | ID round-trip; type confusion не компилируется |
| E01-T02 | ✅ Готово | Единицы, ticks, tolerances | T01 | typed length/speed/time/angle | locale-independent serialization; invalid finite values rejected |
| E01-T03 | ✅ Готово | Project metadata и CRS descriptor | T01–T02 | project root types | CRS/unit/provenance обязательны |
| E01-T04 | ✅ Готово | Reference line primitives | T02 | line/arc/transition API | continuity/property tests на границах |
| E01-T05 | ✅ Готово | Corridor и cross-section model | T03–T04 | roads, profile sections, lane semantics | типовой подход описывается без meshes/backend IDs |
| E01-T06 | ✅ Готово | Junction, crossing, sidewalk, rail semantics | T05 | domain entities | ссылки типизированы; dangling refs диагностируются |
| E01-T07 | ✅ Готово | Traffic control model | T06 | signs/markings/signals/controllers | phase invariants и object refs тестируются |
| E01-T08 | ✅ Готово | Demand/scenario/experiment skeleton | T03 | domain types без engine logic | явные units/intervals/seed policy |
| E01-T09 | ✅ Готово | Rule configuration/exceptions model | T03 | pins/exceptions | stale hash semantics покрыта тестом |
| E01-T10 | Не начато | Semantic project hash | T01–T09 | canonical content hashing | UI/cache/order не меняют hash |

E01-T07 дополнен явным backend-independent `SignalMovementBinding`: signal group
ссылается на semantic movement через пару stable Design lane IDs. Catalog
проверяет group/lane refs и deterministic serde order; разрешение в compact CSN
movement и unbound/conflict diagnostics остаются E07-T07.

**Особое review:** геометрические типы и единицы должны быть одобрены до массового появления API. Иначе изменение распространяется на storage, renderer и backend.

## 10. Epic E02 — Команды, транзакции и варианты

**Цель:** все изменения модели проходят проверяемый command path.

| ID | Статус | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|---|
| E02-T01 | 🟡 Реализовано¹ | `ModelTransaction` и revision IDs | E01 | atomic mutation layer | failed transaction не меняет model/hash |
| E02-T02 | ✅ Готово | Command trait/envelope/diagnostics | T01 | command API | код ошибки и affected IDs стабильны |
| E02-T03 | ✅ Готово | Create/update/delete commands | T02 | базовый command set | structural validation на commit |
| E02-T04 | Не начато | Geometry intent commands | E01-T04, T02 | create/split/move corridor | один drag commit = одна команда |
| E02-T05 | 🟡 Реализовано² | Undo/redo и inverse commands | T03–T04 | bounded command history | apply+inverse восстанавливает semantic hash |
| E02-T06 | Не начато | Proposed command/diff | T05 | preview API для auto/AI/fix | preview не меняет model; apply undoable |
| E02-T07 | Не начато | Fragment copy/paste format | T03 | versioned fragment | IDs remapped, external refs diagnosed |
| E02-T08 | Не начато | Variant spike и ADR-Q01 | E01-T10, T05 | benchmark/prototype/ADR | выбранный механизм поддерживает A/B без неявного общего mutable state |
| E02-T09 | Не начато | Variant implementation | T08 | variants API | изменение B не меняет A; hashes трассируются |

¹ Atomic project/revision behavior и failure paths реализованы. Проверка
semantic hash добавится после E01-T10; до этого E02-T01 не отмечается полностью
готовой.

² Bounded state-bound history, inverse для corridor CRUD, stable-ID restore и
property test восстановления semantic project реализованы. Полное закрытие T05
ожидает geometry intent commands T04 и semantic hash E01-T10.

## 11. Epic E03 — `.roadsim` storage v1

**Цель:** безопасный, версионируемый и атомарный проектный файл.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E03-T01 | JSON schemas и manifest v1 | E01 | `schemas/roadsim-project/v1` | examples проходят schema validation |
| E03-T02 | Canonical serializer/deserializer | T01, E01-T10 | deterministic model JSON | одинаковая модель дает одинаковый canonical hash |
| E03-T03 | Safe ZIP reader | T01 | bounded staged reader | malicious corpus: traversal/bomb/duplicates rejected |
| E03-T04 | Atomic writer | T02–T03 | temp/flush/rename writer | simulated interruption сохраняет старый файл |
| E03-T05 | Autosave/recovery | T04, E02 | recovery artifacts | crash fixture восстанавливается без silent overwrite |
| E03-T06 | Migration framework | T01–T04 | migration registry/report | v1→v1 idempotent; unknown major rejected |
| E03-T07 | Corpus and round-trip suite | T02–T06 | fixtures | semantic hash сохраняется после open/save |
| E03-T08 | CLI `inspect` and `validate-container` | T03 | JSON output/exit codes | пригодно для CI и не загружает results без запроса |

**M1 exit:** CLI создает минимальный проект, применяет команды, undo, сохраняет `.roadsim`, повторно открывает с тем же semantic hash.

## 12. Epic E04 — Geometry kernel и производная геометрия

**Цель:** надежно вычислять дорожные границы, полосы и snapping для редактора/компилятора.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E04-T01 | Geometry context/tolerance policy | E01-T04 | documented API | tolerance зависит от operation/scale, не global magic |
| E04-T02 | Curve evaluation and stationing | T01 | position/tangent/curvature | analytic fixtures и property tests |
| E04-T03 | Offset/profile evaluation | T02, E01-T05 | lane boundaries | ширина/ориентация/continuity проверяются |
| E04-T04 | Intersections/robust predicates | T01–T03 | topology primitives | near-degenerate corpus не panic и детерминирован |
| E04-T05 | Tessellation API | T02–T03 | screen/render meshes | error bound зависит от zoom; инженерная геометрия не меняется |
| E04-T06 | Spatial index and picking queries | E01 | R-tree wrapper | stable tie-break и benchmark |
| E04-T07 | Snapping guides | T04–T06 | endpoint/tangent/intersection snaps | preview deterministic, commit exact |
| E04-T08 | Geometry fuzz/property suite | T02–T07 | corpus/fuzz targets | no panic/NaN/unbounded allocation |
| E04-T09 | Performance baseline | T03–T06 | Criterion benches | типовая сеть имеет recorded baseline |

Статус E04-T01…T04: 🟢 `roadsim-geometry` требует caller-owned context без
global epsilon, аналитически вычисляет line/arc и bounded composite-Simpson
transition, строит signed offsets/lane cross-sections и явно классифицирует
profile discontinuities. Segment predicates детерминированно различают crossing,
touch, overlap и degenerate input; derived overflow и offset singularity
возвращают стабильные коды. Аналитические regression/property fixtures покрывают
units, orientation, station boundaries и order independence. Tessellation,
spatial index, snapping, fuzz harness и performance baseline остаются T05…T09.

Статус E04-T05: 🟡 geometry API содержит exact cubic Bézier control polygon и
детерминированную adaptive tessellation с явными chord-error/depth/point limits;
endpoint preservation, tightening и limit failure покрыты regression tests.
Reference-line tessellation, zoom-derived renderer policy и cached road meshes
ещё не реализованы, поэтому полный T05 не заявляется.

## 13. Epic E05 — Desktop shell и renderer

**Цель:** нативное окно и GPU viewport, не смешивающие UI и доменную истину.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E05-T01 | `winit` app lifecycle | E00 | app opens/closes on three OS | clean shutdown и DPI events тестируются smoke |
| E05-T02 | `wgpu` device/surface lifecycle | T01 | renderer shell | resize/minimize/device-loss paths не crash |
| E05-T03 | `egui` integration | T01–T02 | panels + viewport | input routing и scaling 100–200% работают |
| E05-T04 | Camera/origin/grid | T02, E04 | 2D navigation | zoom-to-cursor, pan, fit selection |
| E05-T05 | Static road/marking render | E04-T05 | cached mesh passes | model revision invalidates correct cache |
| E05-T06 | Selection/diagnostic overlay | E04-T06, T05 | highlight/gizmos | ID mapping идет к Design UUID |
| E05-T07 | Instanced dynamic agents | T02 | vehicle/person batches | 5k agent benchmark и bounded buffers |
| E05-T08 | Screenshot/render regression harness | T03–T07 | controlled images | known scene stable within platform policy |

Статус E05-T01…T03: 🟡 реализован первый связный shell. На macOS наблюдаемо
создаются native window, Metal surface и два `egui` frame при DPI 200%, после
чего smoke cleanly exits. Resize/minimize и input routing обработаны кодом;
трёхплатформенное runtime evidence и принудительный device-loss test ещё нужны
до статуса `✅ Готово`. Viewport теперь читает immutable CSN встроенного валидного
Design Model и показывает fake-backend frame batches; camera/selection, cached
meshes и instanced/performance acceptance E05-T04…T08 ещё не реализованы.

## 14. Epic E06 — Editor UI и tools

**Цель:** пользователь создает базовую модель только через domain commands.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E06-T01 | Application message/job loop | E02, E05 | UI↔application channel | stale result by revision is discarded |
| E06-T02 | Project new/open/save/recovery UX | E03, T01 | file workflow | failure не заменяет активный project |
| E06-T03 | Object tree and property inspector | E01, T01 | typed inspectors | edits produce commands; mixed selection safe |
| E06-T04 | Tool state machine | E02-T04, T01 | select/draw/edit lifecycle | cancel leaves model unchanged |
| E06-T05 | Draw corridor tool | E04-T07, T04 | interactive road creation | preview + one undoable commit |
| E06-T06 | Cross-section/lane editor | E01-T05, T03 | lane profile UI | invalid widths shown before commit |
| E06-T07 | Crossing/stop/control placement | E01-T06–07, T04 | placement tools | snapping and object refs preserved |
| E06-T08 | Undo/redo/history UI | E02-T05 | actions/hotkeys | selection updates predictably after undo |
| E06-T09 | Diagnostics panel shell | E02-T02 | grouped diagnostics | click selects/zooms object |
| E06-T10 | Long-job progress/cancel | T01 | nonblocking UX | artificial 5s job keeps UI responsive |
| E06-T11 | Accessibility/localization foundation | T03 | message keys, keyboard paths | colors not sole signal; Russian UI baseline possible |

**M2 exit:** пользователь рисует четырехподходный skeleton, задает полосы, размещает переход, undo/redo, сохраняет и открывает проект. Performance baseline зафиксирован на reference hardware.

Статус E06-T03/T08 (частично): 🟡 desktop shell редактирует demo Design Model
только через typed commands: object tree перечисляет реальные corridors,
инспектор меняет ширину полосы кнопочными шагами (один клик — одна undoable
команда), дороги добавляются/удаляются `CreateCorridor`/`DeleteCorridor`, а
undo/redo идут через `CommandHistory` с восстановлением semantic content hash
(покрыто тестом round-trip). Успешная команда перекомпилирует CSN и подменяет
simulation artifact только при неактивном run. Полные draw tools, tool state
machine и property inspector общего вида остаются E06-T04…T07.

## 15. Epic E07 — Компилятор CSN

**Цель:** получить детерминированный backend-independent snapshot.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E07-T01 | CSN schema/header/source map | E01, E04 | compiled types | нет UI/SUMO types; content hash стабилен |
| E07-T02 | Compile pipeline framework | T01 | staged compiler | stage failure не публикует partial CSN |
| E07-T03 | Normalize/evaluate corridors | E04, T02 | compiled lane arrays | golden straight/T roads |
| E07-T04 | Lane/pedestrian graph | T03 | adjacency/reachability | disconnected refs диагностируются |
| E07-T05 | Junction movement inference | T03–T04 | movements/connectors | ambiguity blocks compile with object refs |
| E07-T06 | Movement curves and conflicts | E04, T05 | conflict zones/matrix | known movement pairs match fixtures |
| E07-T07 | Signals/stops binding | E01-T07, T05 | compiled controls | unbound/conflicting groups rejected |
| E07-T08 | Spatial/lookup indices | T03–T07 | compact indices | referential integrity test 100% |
| E07-T09 | Capability requirements | T04–T07 | requirement manifest | feature use maps to stable capability IDs |
| E07-T10 | Incremental compile prototype | T02–T09 | cache/invalidation | output semantically equals full compile |
| E07-T11 | Compiler diagnostics/source map | T02–T09 | user diagnostics | ≥95% fixture errors have Design object ref; remainder classified global |
| E07-T12 | Compiler benchmarks | T03–T10 | baseline | MVP intersection ≤2s acceptance target |

Статус E07-T01…T03/T09: 🟡 опубликован минимальный immutable CSN contract и
атомарный compiler slice для straight corridor с одним постоянным cross-section.
Compact lane arrays, source map, stable SHA-256 content hash и первые capability
IDs покрыты unit/regression/property tests. Arc/transition и переменный профиль
явно блокируются diagnostics с Design object refs; полноценная curve evaluation,
movement curves/conflicts и весь capability manifest остаются следующими задачами.

Статус E07-T04: 🟢 CSN schema v2 содержит compact directed lane adjacency и
pedestrian graph с полным lane/walking-area/sidewalk/crossing source mapping.
Junction approaches создают только coarse inter-corridor reachability без
преждевременной turn geometry; crossings создают двунаправленные pedestrian
links. Car/bus и pedestrian demand с disconnected endpoints блокируется до
backend compile стабильной object-linked diagnostic. Unit, regression и chain
property tests покрывают direction, transitive reachability и invalid graph IDs.

Статус E07-T05: 🟢 CSN schema v3 содержит детерминированно индексированные
semantic lane-to-lane movements и требует backing edge в coarse lane graph для
каждого movement. Compiler поддерживает однозначные one-lane merge/diverge и
блокирует несколько target lanes одного corridor для одной source lane кодом
`compiler.movement.ambiguous` со ссылками на junction, corridors и lanes. Явный
lane-assignment intent остаётся будущим расширением movement inference.

Статус E07-T06: 🟢 CSN schema v4 содержит exact cubic curve для каждого
movement и sparse symmetric conflict matrix. Compiler использует явные geometry/
tessellation/resource options, ограничивает total derived points и segment-pair
tests, а для crossing/overlap centerlines строит width-expanded conflict AABB.
Перпендикулярный четырёхподходный fixture проверяет известную пару, стабильность
между revisions и failure при исчерпании лимитов. Swept-area proximity без
пересечения centerlines и priority/yield не заявлены этой стадией; последнее
остаётся E10-T04.

Статус E07-T07: 🟢 CSN schema v5 содержит travel-oriented per-lane stop
positions, signal groups с compact movement IDs, authored fixed-time
programs/phases/states и active controller programs. Compiler публикует
`signals.fixed_time` только для полного control snapshot и до backend compile
блокирует unbound/unresolved groups, duplicate movement ownership, несколько
controllers одного junction и одновременно зелёные geometry-conflicting
movements стабильными object-linked diagnostics. SUMO TLS mapping и runtime
signal batches остаются E10-T05/E11-T05.

## 16. Epic E08 — Нормативный движок RU

**Цель:** versioned, testable rules without false compliance claims.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E08-T01 | Rules API/finding/evidence | E01, E07-T01 | typed contracts | finding stable-sortable и serializable |
| E08-T02 | Ruleset metadata/registry/pinning | T01, E03 | RU artifact format | exact version открывается; missing pin blocks evaluation |
| E08-T03 | SemanticRuleView | E01, E07 | read-only view | rule не получает mutable model/UI state |
| E08-T04 | Applicability and coverage engine | T01–T03 | pass/fail/not-evaluated model | отсутствие rule не превращается в pass |
| E08-T05 | Exception/staleness engine | E01-T09, T04 | audited exceptions | relevant change marks exception stale |
| E08-T06 | Autofix proposal bridge | E02-T06, T01 | proposed commands | fix previewed, validated, undoable |
| E08-T07 | Domain review workflow | E00, T02 | template/checklist/owners | rule PR не merge без source/clause/tests/reviewer |
| E08-T08 | Первый геометрический rule pack | E04, T07 | reviewed rules/fixtures | positive/boundary/negative/N/A для каждого rule |
| E08-T09 | Crossing/stop-line/signal rules | E07-T07, T07 | reviewed rules | evidence показывает objects/value/limit/unit |
| E08-T10 | Marking/sign consistency rules | E01-T07, T07 | reviewed rules | coverage matrix обновляется автоматически |
| E08-T11 | Diagnostics UI integration | E06-T09, T04–T10 | findings panel/actions | click/highlight/explain/autofix/exception flows |
| E08-T12 | Coverage report export | T04, T08–T10 | machine+human report | перечислены implemented/partial/manual/not evaluated |

**Обязательная зависимость:** E08-T08…T10 нельзя брать coding agent без подтвержденных source metadata и domain owner. Агент может писать инфраструктуру и tests по предоставленной трактовке, но не определять норму.

**M3 exit:** эталонный перекресток компилируется в CSN, показывает conflict diagnostics и первые нормативные findings с evidence/coverage.

## 17. Epic E09 — Backend API и worker protocol

**Цель:** стабильная граница симуляции до интеграции SUMO.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E09-T01 | Backend capabilities/errors/lifecycle | E07-T09 | `roadsim-backend-api` | compile-time/runtime/unsupported errors distinct |
| E09-T02 | In-memory fake backend | T01 | contract test double | lifecycle/pause/cancel/frame tests |
| E09-T03 | RunConfig and deterministic seed derivation | T01, E01-T08 | run contract | known vectors stable; wall clock excluded |
| E09-T04 | Worker protocol spike/ADR-Q02-Q03 | T01 | schema/IPC decision | Windows/Unix prototype, size/version limits |
| E09-T05 | Protocol messages/handshake | T04 | versioned schema | mismatch and unknown capability tested |
| E09-T06 | Local endpoint authentication | T04–T05 | nonce/token flow | unrelated process cannot reuse stale endpoint |
| E09-T07 | Client/server lifecycle and watchdog | T05–T06 | worker harness | crash/hang/timeout/cancel tests |
| E09-T08 | Frame/metric batching and backpressure | T05 | batch transport | visual drop allowed; metric loss impossible |
| E09-T09 | Common backend contract suite | T01–T08 | reusable tests | fake backend passes all cases |
| E09-T10 | Resource/workdir management | T07 | bounded run directories | cleanup/recovery/incomplete marker tested |

Статус E09-T01…T03/T09: 🟡 стабилизирован первый object-safe async contract и
in-memory fake backend. Handshake, capability preflight, compile/runtime/
unsupported errors, pause/resume/cancel, terminal semantics, bounded idempotency
keys и deterministic frame batches покрыты contract/property tests; root seed
substreams закреплены known vectors. Production worker adapter, accepted batch
transport, periodic watchdog policy, resource directories и
общий suite для внешних backend остаются невыполненными.

Статус E09-T04…T07: 🟡 добавлен macOS-tested cross-platform child-process
prototype из ADR-018 (`proposed`): inherited stdin/stdout, versioned bounded JSON
control frames, one-time token, correlation, capability rejection и
crash/hang/timeout/cancel harness. Acceptance остаётся частичным до Windows/Linux
CI evidence, architecture/security review и принятия transport ADR. State/metric
production transport/benchmark, accepted backpressure decision и
workdir/resource isolation не реализованы.

Статус E09-T08: 🟡 ADR-019 (`proposed`) добавляет исполняемый dual-pipe baseline:
SoA visual frames используют latest-wins с observable drop counter, versioned
metrics и terminal events идут через отдельную bounded reliable queue с
backpressure. macOS child-process contract test доказывает 32→1 visual drop и
12/12 ordered metrics без потерь. Arrow/shared-memory выбор, benchmark и
Windows/Linux CI evidence остаются до принятия ADR-Q09.

Статус E09-T10: 🟡 добавлен bounded `RunDirectoryManager`: отдельный generated
workdir передаётся child process как `current_dir`, append-only schema v1 journal
проверяет lifecycle, а startup recovery переводит прерванные `Starting/Running`
в `Incomplete`. Retention автоматически удаляет только старейшие
`Completed/Cancelled`; `Failed/Incomplete` сохраняются и блокируют capacity без
silent cleanup. CPU/memory/disk-byte OS quotas и platform sandbox остаются для
E10/E16.

Preview вертикального пути E11: desktop наблюдаемо выполняет встроенный Design
Model → CSN → fake backend → 18-agent frame overlay и предоставляет
Start/Pause/Resume/Stop с unit lifecycle tests. Agent frames содержат метрический
footprint 4,5 × 1,8 м и viewport рисует ориентированный прямоугольник. Native
macOS smoke за два GPU frame подтверждает
`simulation_state=running tick=0 agents=18`. Это ранняя
проверка границ, а не закрытие E11-T01…T03: production orchestration всё ещё
зависит от E10 worker/state batches, frame adapter не instanced и SUMO отсутствует.

## 18. Epic E10 — SUMO/libsumo adapter

**Цель:** первый production backend без утечки SUMO semantics в продуктовую модель.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E10-T01 | Pin SUMO version/build strategy | E00, E09 | build matrix/license note | exact engine version reported in handshake |
| E10-T02 | Minimal worker with libsumo lifecycle | T01, E09-T07 | start/step/close | worker crash isolated; editor/CLI survives |
| E10-T03 | CSN→SUMO road/lane compiler | E07, T02 | typed export bundle | straight-road fixture runs; mapping preserved |
| E10-T04 | Junction/connectors/priority compiler | E07-T05–06, T03 | junction export | known turn paths and yields run |
| E10-T05 | Signals compiler | E07-T07, T04 | TLS export | phase timeline matches fixture |
| E10-T06 | Vehicle demand/routes compiler | E01-T08, T03–T04 | routes/flows | unreachable demand blocks before start |
| E10-T07 | Pedestrian demand/crossings compiler | E07-T04, T04 | person flows | crossing scenario produces expected events |
| E10-T08 | Batch state collector | E09-T08, T02 | Arrow/frame batches | no per-agent IPC; IDs map to Design origin |
| E10-T09 | SUMO diagnostics translation | T03–T08 | structured errors | backend XML/log errors map where possible |
| E10-T10 | SUMO contract/golden suite | T03–T09 | pinned fixtures | repeated seed accepted deterministic |
| E10-T11 | Packaging spike on three OS | T01–T02 | packages/report | clean-machine worker starts or limitation documented |
| E10-T12 | License distribution review | T01, T11 | approved packaging decision | EPL notices/source obligations recorded |

Статус E10-T01: 🟢 exact SUMO `1.27.1` source tag/commit, headless build matrix и
pending license boundary закреплены machine-readable manifest с CI regression
guard. Worker protocol v3 сообщает exact engine name/version/build revision,
блокирует mismatch до session, а macOS arm64 smoke подтверждает runtime version
из libsumo. Clean-machine artifacts остаются отдельной задачей E10-T11.

Статус E10-T02: 🟢 добавлен отдельный `sumo-worker`, versioned native C ABI и
явный protocol v3 lifecycle `OpenSession → StepSession → CloseSession`.
Cross-process ABI fixture доказывает ordered 3+2 steps, close и изоляцию native
abort; missing/mismatched engine блокируется до session. Production C++ bridge
вызывает libsumo `getVersion/start/step/close`; opt-in test на macOS arm64
генерирует минимальную сеть, запускает exact headless `1.27.1`, выполняет пять
tick и завершает recoverable run как `Completed`. Platform packaging и
clean-machine matrix не приписываются T02 и остаются E10-T11.

Статус E10-T03: 🟢 `roadsim-backend-sumo` детерминированно переводит straight
CSN lanes в typed SUMO plain-network bundle без filesystem/libsumo dependency.
Bundle сохраняет `CompiledLaneId → LaneOrigin → SUMO edge/lane` mapping, требует
явную скорость и блокирует неподдерживаемые lane uses с Design object evidence.
Exact `netconvert 1.27.1` принимает export, после чего opt-in worker smoke
выполняет пять tick на сгенерированной CSN-дороге. Junction topology, demand и
routes намеренно остаются E10-T06; junction topology добавлена E10-T04.

Статус E10-T04: 🟢 `roadsim-backend-sumo` экспортирует compiled junction
movements как полный explicit SUMO connection table: одно movement — ровно одна
`<connection>`, узел с movements получает `type="priority"`, а
`SUMO_NETCONVERT_INPUT_ARGUMENTS` отключает turnarounds и эвристические связи.
`SumoConnectionMapping` сохраняет `CompiledMovementId → JunctionId → SUMO
edge/lane`. Неполный набор movements, разорванные endpoints и один узел с двумя
junction ID блокируются object-linked diagnostics. ADR-022 фиксирует, что
приоритет не выдумывается backend: RoadSim не авторизует junction priority, а
right-of-way между уже зафиксированными связями считает pinned `netconvert
1.27.1`. Pedestrian graph и traffic controls явно отклоняются кодами
`backend.sumo.pedestrian_network.unsupported` и
`backend.sumo.traffic_controls.unsupported` и остаются E10-T07/E10-T05.

Статус E10-T05: 🟢 активная fixed-time программа контроллера экспортируется как
один `<tlLogic type="static">` в `roadsim.tll.xml`: узел получает
`type="traffic_light"`, связи — `tl`/`linkIndex` в compact movement order, а
`SumoSignalMapping` сохраняет `SignalControllerId → SignalProgramId → link
index → CompiledMovementId`. Authored порядок фаз, длительности и per-group
indication сохраняются один к одному; `Green` экспортируется как major `G`,
потому что compiler уже блокирует одновременно зелёные конфликтующие movements.
Movement сигнализированного узла без группы отклоняется
(`backend.sumo.signal_movement.unbound`), контроллер неизвестного узла —
`backend.sumo.signal_junction.unknown`. Authored `intergreen > 0` намеренно не
экспортируется: распределение clearance между amber и all-red требует
подтверждённой трактовки domain owner (ADR-022, п. 10) и блокируется кодом
`backend.sumo.signal_intergreen.unsupported`. Stop positions остаются
неподдержанными отдельным кодом.

Статус E10-T06: 🟢 добавлен отдельный compiled demand contract
(`CompiledDemandTable`, schema v1): спрос — состояние сценария, поэтому он не
входит в CSN и одна и та же сеть запускается с разными профилями без
перекомпиляции топологии. `compile_demand` резолвит authored corridor endpoints
в единственную boundary lane (source без предшественника, sink без
преемника), блокирует неизвестный профиль, неоднозначный endpoint,
недостижимую пару и нецелевой mode до backend. Экспортер переводит каждый
authored interval ровно в один `<flow>` с явными `begin`/`end`/`vehsPerHour` —
arrival process и rate не выдумываются, — и сохраняет
`DemandFlowId → interval index → SUMO flow/edges`. Маршрут между boundary edges
пока считает SUMO по exported connection table; authored full routes остаются
следующим шагом. Opt-in smoke на exact SUMO `1.27.1` прогоняет authored car
через exported junction и требует непустой visual frame — это исполняемое
доказательство M4 для автомобильного сценария.

Статус E10-T07: ⛔ заблокирована. SUMO выражает пешехода как `<personFlow>` со
стадиями `walk` между edges, а authored demand задан между walking areas. В
Design Model нет ни одного typed reference между `WalkingArea` и `Sidewalk`
(crossing связывает две walking areas, sidewalk знает только corridor/side/
station), поэтому построить endpoint для `walk` можно лишь геометрической
догадкой, меняющей маршруты и метрики UC-03. Пробел зафиксирован в ADR-023;
задача ждёт follow-up к E01-T06, который добавит связь «walking area ↔
pedestrian edge». До этого экспорт продолжает отклонять непустой pedestrian
graph и pedestrian demand явными кодами.

Статус E10 (client slice, T02→UI): 🟢 `roadsim-backend-sumo-client` реализует
versioned backend contract поверх worker IPC: compile материализует typed
export bundle в bounded run directory (journal Created→Running→Completed/
Failed) и вызывает pinned `netconvert`; start поднимает worker с handshake по
exact engine identity; события переводят latest-wins visual frames в backend
frames без изобретения агентов (отсутствующий кадр — пустой batch на
достигнутом tick). Pause/Resume — клиентское степание, cancel идёт через
worker; contract suite выполняется против protocol worker-stub без SUMO.
Desktop app включает этот backend переменными `ROADSIM_SUMO_WORKER`/
`ROADSIM_NETCONVERT` (+ `ROADSIM_SUMO_BRIDGE`); неполная конфигурация — ошибка,
а не тихий fallback.

Статус E10-T08: 🟡 native bridge ABI v2 собирает vehicle positions/headings и
footprints одним bounded вызовом после `StepSession`; worker публикует
отсортированный protocol-v3 SoA frame через отдельный latest-wins writer, не
блокируя control pipe. ABI fixture и exact SUMO 1.27.1 smoke проверяют compact
agent IDs и размер 4,5 × 1,8 м. Полное Design-demand→agent source mapping,
person/signal/queue batches, benchmark и Arrow/shared-memory решение остаются
следующими частями E10-T06…T08/E11-T03.

Для задач T03–T07 обязательны `unsupported_feature` fixtures. Экспорт не должен удалять неподдерживаемые сущности.

## 19. Epic E11 — Simulation UX и динамический renderer

**Цель:** полный путь compile → run → frames → control в desktop.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E11-T01 | Run orchestration state machine | E09, E10 | application service | все terminal paths и повторный запуск протестированы |
| E11-T02 | Simulation control panel | T01, E06 | compile/start/pause/cancel UI | invalid transition disabled/diagnosed |
| E11-T03 | Frame snapshot adapter | E10-T08, E05-T07 | GPU-ready SoA | bounded allocations и ID mapping |
| E11-T04 | Playback/interpolation | T03 | smooth visualization | interpolation не меняет metrics/tick labels |
| E11-T05 | Signal/queue overlays | E10-T05, T03 | visual state | states align with frame tick |
| E11-T06 | Worker failure/restart UX | E09-T07, T01 | recovery dialog/bundle | project stays editable after forced crash |
| E11-T07 | Performance instrumentation | T01–T05 | frame/IPC/run metrics | dropped visual frames visible in diagnostics |

**M4 exit:** на эталонном регулируемом перекрестке движутся автомобили и пешеходы, сигналы отображаются собственным renderer, UI pause/cancel работает, worker crash не теряет проект.

Статус E11-T01: 🟢 добавлен отдельный `roadsim-application` с backend-agnostic
`RunOrchestrator`: `Idle → Prepared → Running ⇄ Paused → Completed/Cancelled/
Failed`, restart через `Reset` возвращает к тому же prepared artifact. Каждый
принятый запрос возвращает ровно один `RunIntent`, который выполняет caller, —
state machine не делает I/O и тестируется без движка и GPU. Cancel только
запрашивается: run завершает backend, поэтому медленный cancel не выглядит
завершённым. Все три terminal outcome, повторный запуск, незапрошенное
изменение состояния движком и каждый invalid transition покрыты тестами;
отказ — стабильный код, а не panic, и не меняет состояние.

Статус E11-T02: 🟡 desktop shell берёт enablement кнопок Start/Pause/Resume/
Stop из `RunOrchestrator::accepts`, поэтому UI не может предложить переход,
который run отклонит, а отказ показывается стабильным кодом
`application.run.invalid_transition`. Панель показывает активный backend
(fake или SUMO worker), выбранный из окружения без тихого fallback.
Полноценная compile/progress-панель и diagnostics остаются в T02/E06-T09.

Статус E11-T03: 🟢 `FrameSnapshotAdapter` переводит backend frame в GPU-ready
SoA с переиспользуемыми буферами: установившийся run не выделяет память на
кадр, а превышение явного bound и non-finite состояние отклоняются стабильным
кодом, оставляя предыдущий snapshot целым. Backend agent ID и compact lane ID
сохраняются без переиндексации, поэтому instance остаётся связанным с моделью;
`f64 → f32` сужение выполняется только на границе рендера. Полный маппинг до
Design-объекта конкретного автомобиля зависит от per-agent demand origin и
остаётся с E10-T06/E10-T08.

## 20. Epic E12 — Спрос, автобусы, трамвай и сценарии

**Цель:** покрыть мультимодальные сценарии MVP и честно ограничить неподдерживаемые.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E12-T01 | Demand profile editor | E01-T08, E06 | interval/turning UI | units/interval overlap/total flow validated |
| E12-T02 | Arrival distributions and seed streams | E09-T03, T01 | deterministic generator | known seed vectors and statistical sanity tests |
| E12-T03 | Vehicle type distributions | T01–T02 | typed configs | invalid/unsupported parameters rejected |
| E12-T04 | Pedestrian zones/flows editor | E07-T04, T01 | demand UI/compiler | reachability shown before run |
| E12-T05 | Bus route/stop/dwell domain | E01, T03 | model + UI | dwell distribution and stop position valid |
| E12-T06 | Bus SUMO mapping and metrics | E10, T05 | backend support | UC-04 golden scenario passes |
| E12-T07 | Tram MVP scope matrix | PROJECT_SPEC | supported configurations document | domain+SUMO capabilities explicitly mapped |
| E12-T08 | Basic tram domain/compiler/backend | T07, E07, E10 | happy path | UC-05 passes for supported fixture; others block |
| E12-T09 | Signal program editor | E01-T07, E06 | phase/timeline UI | conflict/intergreen diagnostics before run |
| E12-T10 | Scenario editor | T01–T09 | duration/warmup/outputs/seed | serialized scenario creates repeatable RunConfig |
| E12-T11 | Golden behavior suite | T04–T10 | UC-03…05 fixtures | expected event/metric tolerances domain-approved |

**M5 exit:** car/pedestrian/bus and restricted tram scenarios are runnable; any out-of-scope combination is detected before worker start.

## 21. Epic E13 — Результаты, метрики и сравнение

**Цель:** инженерно определенные, воспроизводимые и сопоставимые результаты.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E13-T01 | Results schema registry | E09, PROJECT_SPEC | Arrow schemas v1 | fields/units/nullability/version documented |
| E13-T02 | Run manifest | T01, E09-T03, E10 | `run_manifest.json` | inputs/versions/hashes/seeds/status complete |
| E13-T03 | Streaming Arrow recorder | T01, E10-T08 | RecordBatch pipeline | bounded memory/backpressure/cancel tested |
| E13-T04 | Atomic Parquet writer | T03 | datasets | incomplete file not listed as completed |
| E13-T05 | Metric definition registry | T01 | versioned docs/code | travel time/delay/queue/throughput/stops formalized |
| E13-T06 | Raw→derived metric pipeline | T03–T05 | aggregations | fixture values independently verified |
| E13-T07 | Timeline/charts UI | T06, E06 | analysis panels | units/aggregation/run shown, no misleading default |
| E13-T08 | Experiment/replication scheduler | E02-T09, E11 | bounded batch runs | resource limit, cancel and partial status work |
| E13-T09 | A/B comparison statistics | T05–T08 | comparison model/UI | paired seeds, uncertainty and incompatible definitions handled |
| E13-T10 | CLI validate/compile/run/experiment/export | E03, E07, E11, T04 | stable commands/JSON/exit codes | UI и CLI manifests equivalent |
| E13-T11 | HTML report | T09–T10, E08-T12 | portable report | provenance, warnings, coverage, metrics included |
| E13-T12 | Python/Arrow smoke consumer | T04 | example notebook/script tests | Parquet читается и units metadata проверяется |

**M6 exit:** variant A/B запускается с paired seeds через UI и CLI; Parquet и отчет содержат проверяемый manifest и одинаковые metric definitions.

## 22. Epic E14 — OpenDRIVE и GIS

**Цель:** безопасный interchange с явной потерей информации.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E14-T01 | OpenDRIVE 1.9 subset matrix | E01, E07 | support document | каждая сущность имеет lossless/normalized/approx/unsupported |
| E14-T02 | Secure XML parser + IR | T01 | ImportModel | DTD/external entities/oversize rejected |
| E14-T03 | OpenDRIVE importer | T02, E02-T06 | proposed command batch | import failure не меняет project |
| E14-T04 | OpenDRIVE exporter | T01, E01 | xodr + loss report | object IDs и approximations перечислены |
| E14-T05 | Round-trip corpus | T03–T04 | fixtures | supported subset сохраняет semantic equivalence |
| E14-T06 | PROJ coordinate service | E01-T03 | typed transforms | axis/unit/CRS provenance fixtures |
| E14-T07 | GeoJSON importer | T06, E02-T06 | vector import | ambiguous CRS requires explicit choice |
| E14-T08 | GDAL worker spike | E09 patterns, T06 | isolated process | network VFS off; limits/crash tested |
| E14-T09 | GeoPackage import through worker | T08 | normalized batches | project mutation only after preview/apply |
| E14-T10 | Import/export UX | T03–T09, E06 | mapping/loss/preview panels | user must acknowledge lossy export |

## 23. Epic E15 — Extensibility foundations

**Цель:** подготовить стабильные границы, не задерживая MVP из-за публичного SDK.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E15-T01 | Headless application service boundary | E13-T10 | internal service API | CLI не вызывает UI crates |
| E15-T02 | Python SDK prototype | T01 | subprocess client | validate/run/read results example работает |
| E15-T03 | WIT world design | stable domain/results | draft `validator/importer/metric` worlds | no mutable model/raw host access |
| E15-T04 | Wasmtime sandbox spike | T03 | capability/fuel/memory prototype | malicious loop/memory/fs requests contained |
| E15-T05 | Plugin manifest/signature design | T03–T04 | draft schema/threat model | permissions visible and deny-by-default |
| E15-T06 | Deterministic plugin host services | E09-T03, T04 | RNG/time policy | plugin cannot read wall clock in deterministic mode |

Tasks T03–T06 являются preview/post-MVP и не делают API stable. Публикация stable SDK требует отдельного milestone и compatibility policy.

## 24. Epic E16 — Hardening, CI и release engineering

**Цель:** превратить интеграционный прототип в надежный OSS MVP.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E16-T01 | Полная трехплатформенная CI matrix | M6 | jobs/artifacts | clean runner passes required suite |
| E16-T02 | Malicious input/fuzz program | E03, E14, E09 | targets/corpus | no known crash/path escape/unbounded allocation |
| E16-T03 | Worker chaos tests | E09–E11 | kill/hang/corrupt/version cases | editor/CLI exit states and cleanup correct |
| E16-T04 | Determinism matrix | E10, E12, E13 | documented report | repeatability defined by OS/backend/version |
| E16-T05 | Performance budgets | M6 | reference hardware report | NFR-001…006 measured; regressions gated |
| E16-T06 | Memory/resource soak | M6 | long-run report | no unbounded growth in editor/worker/recorder |
| E16-T07 | Accessibility/UX acceptance | M6 | checklist/pilot findings | key workflows keyboard/scaling/error comprehension |
| E16-T08 | Packaging/installers | E10-T11, T01 | signed artifacts | install/run/uninstall smoke on three OS |
| E16-T09 | SBOM, licenses, notices | E00, E10-T12 | release bundle | all shipped components covered |
| E16-T10 | Migration/corpus support | E03 | compatibility suite | all supported project versions open/migrate |
| E16-T11 | User docs/tutorial/examples | M6 | documentation site/content | UC-01/A-B completed from docs on clean install |
| E16-T12 | Diagnostic bundle/redaction | E11, E13 | opt-in bundle | secrets/paths previewed and redacted |
| E16-T13 | Release checklist and rollback | T01–T12 | release runbook | RC can be reproduced and revoked |

## 25. Epic E17 — Validation и пилот

**Цель:** доказать полезность и отсутствие критических модельных ошибок.

| ID | Задача | Зависимости | Deliverable | Acceptance criteria |
|---|---|---|---|---|
| E17-T01 | Утвердить reference intersections | M5 | legally shareable fixtures | покрывают T, 4-way, signal/yield, bus/crossing |
| E17-T02 | Независимая проверка geometry/CSN | T01 | review report | source map, movements, conflicts подтверждены |
| E17-T03 | Сравнение SUMO artifacts с ручным baseline | T01 | validation report | существенные расхождения объяснены |
| E17-T04 | Проверка metric calculations | E13, T01 | independent calculations | значения совпадают в tolerance |
| E17-T05 | Нормативный review RU coverage | E08, T01 | signed coverage report | источники/пункты/ограничения/непокрытое перечислены |
| E17-T06 | Usability pilot | E16-T11, T01 | observations/metrics | success metrics из PROJECT_SPEC собраны |
| E17-T07 | Reproducibility exchange | M6, T01 | second-machine report | `.roadsim`+manifest воспроизводит accepted metrics |
| E17-T08 | RC defect triage | T02–T07 | blocker list | zero open blocker/critical; high have decisions |
| E17-T09 | MVP acceptance audit | все | signed checklist | все 12 критериев PROJECT_SPEC §12 подтверждены |

**M8 exit:** release candidate принят по E17-T09, опубликованы known limitations, нормативный coverage и compatibility matrix.

## 26. Рекомендуемый порядок первых PR

Порядок специально минимизирует конфликт агентов и раннюю фиксацию неправильных API.

1. **PR-001:** repository skeleton, toolchain, licenses, source-of-truth docs.
2. **PR-002:** CI fmt/clippy/test/docs + contribution templates.
3. **PR-003:** typed IDs, units, ticks и basic diagnostics.
4. **PR-004:** minimal Project/CRS metadata + canonical hash spike.
5. **PR-005:** reference line primitives с property tests.
6. **PR-006:** corridor/cross-section/lane Design Model.
7. **PR-007:** transaction/command API + create/update commands.
8. **PR-008:** undo/redo invariants.
9. **PR-009:** `.roadsim` manifest/schema + canonical JSON.
10. **PR-010:** safe ZIP staged reader/writer + malicious fixtures.
11. **PR-011:** minimal CLI create/inspect/round-trip.
12. **PR-012:** `winit/wgpu/egui` shell.
13. **PR-013:** curve tessellation, camera и static corridor render.
14. **PR-014:** application message loop + selection/picking.
15. **PR-015:** draw corridor tool through commands.
16. **PR-016:** CSN header/pipeline + straight lane compilation.
17. **PR-017:** junction movement/connectors/conflicts fixtures.
18. **PR-018:** rules API/coverage semantics + one synthetic rule.
19. **PR-019:** backend API/fake backend/common contract tests.
20. **PR-020:** IPC handshake/lifecycle/worker harness.
21. **PR-021:** minimal libsumo worker and pinned build.
22. **PR-022:** straight network/demand export and state batch.
23. **PR-023:** dynamic agents renderer + run controls.
24. **PR-024:** pedestrian crossing and signal compiler.
25. **PR-025:** result manifest + first Arrow/Parquet trip table.

После PR-025 проводится архитектурная ретроспектива. До нее не следует параллельно строить полный ruleset, tram, plugin SDK или сложную аналитику.

## 27. Допустимая параллельность

После стабилизации contracts можно параллелить:

- geometry fixtures/benchmarks и desktop shell;
- storage hardening и editor UI;
- rules infrastructure и compiler stages;
- worker protocol и result schemas;
- documentation/examples и security corpus.

Нельзя независимо реализовывать без общего владельца:

- Design Model schema и `.roadsim` schema;
- CSN и SUMO compiler;
- metric definition и UI comparison;
- rules applicability и нормативные checks;
- protocol schema и обе его стороны;
- command semantics и editor tools.

Для таких пар назначается один contract owner; остальные агенты работают по merged interface или test double.

## 28. Контроль зависимостей

Критический путь MVP:

```text
E00
 → E01 → E02 → E03
       ↘ E04 → E05 → E06
              ↘ E07 → E08
                     ↘ E09 → E10 → E11
                              ↘ E12 → E13
                                       ↘ E16 → E17
```

E14 и E15 не должны блокировать M6. Для MVP обязателен OpenDRIVE subset из FR-060, но тяжелый GIS/GDAL и stable SDK допускается завершать после основного simulation/comparison path. Если ресурс ограничен, сначала T01–T05 OpenDRIVE, затем GDAL/plugin preview.

## 29. Тестовые наборы и владельцы

| Набор | Содержимое | Владелец | Gate |
|---|---|---|---|
| `fixtures/geometry` | кривые, offsets, вырожденные случаи | geometry owner | каждый PR geometry |
| `fixtures/projects` | schema corpus по версиям | storage owner | migration/release |
| `fixtures/rules` | positive/boundary/negative/N/A | domain + rules owner | каждый rule PR |
| `fixtures/golden-scenarios` | micro behavior/metrics | simulation + domain | nightly/release |
| `fixtures/opendrive` | законно распространяемый subset corpus | interchange owner | importer/exporter |
| `fixtures/malicious` | ZIP/XML/protocol/plugin cases | security owner | nightly/release |
| `benchmarks` | compile/render/IPC/results | performance owner | trend/release |

Fixture должен иметь README: происхождение, лицензия, units, expected result и процедура обновления.

## 30. Acceptance criteria по вертикальным сценариям

### VS-01. Минимальный проект

- CLI создает `.roadsim` с CRS и пустой моделью;
- desktop открывает его и показывает сетку;
- повторное сохранение сохраняет semantic hash;
- неизвестная major schema отклоняется понятным кодом.

### VS-02. Редактируемая дорога

- пользователь рисует reference line и добавляет полосы;
- viewport обновляется через derived mesh;
- undo удаляет результат одним действием;
- self-intersection или отрицательная ширина дает object diagnostic.

### VS-03. Перекресток и правило

- четыре подхода соединяются movements;
- ambiguous movement blocks CSN;
- conflict zone и signal binding визуализируются;
- минимум одно reviewed rule дает evidence и safe autofix preview.

### VS-04. Первый run

- capability check проходит до worker start;
- libsumo worker запускает pinned artifact;
- dynamic batch отображается без per-agent IPC;
- forced worker kill переводит run в Failed и сохраняет editor state.

### VS-05. Инженерное сравнение

- варианты A/B имеют разные semantic hashes и общий demand;
- paired replications используют записанные seeds;
- metric definitions/units совпадают;
- отчет и Parquet позволяют независимо повторить сравнение.

## 31. Риски и меры снижения

| ID | Риск | Вероятность/влияние | Ранний индикатор | Мера |
|---|---|---|---|---|
| R-01 | Геометрические offsets/перекрестки сложнее ожиданий | высокая/критическое | много special cases, нестабильные predicates | ограничить MVP templates; property/fuzz; geometry owner; не смешивать render truth |
| R-02 | Design Model слишком зависит от SUMO | средняя/критическое | SUMO IDs/options в domain | dependency guard; backend capability/loss report; ADR review |
| R-03 | libsumo packaging нестабилен по ОС | высокая/высокое | ручные локальные установки | spike до M4; pinned builds; worker isolation; publish matrix |
| R-04 | Ошибочная трактовка ГОСТ | средняя/критическое | rule без clause/domain fixture | domain owner gate; coverage semantics; no invented limits |
| R-05 | Лицензии ГОСТ/SUMO/GDAL/assets | средняя/высокое | тексты/данные попадают в repo | license inventory; metadata-only rules; legal review |
| R-06 | UI/egui не масштабируется для CAD workflow | средняя/высокое | tool/input/docking hacks | UX prototype M2; isolate UI; ADR до deep coupling |
| R-07 | IPC становится bottleneck | средняя/высокое | frame drops/CPU serialization | batch SoA/Arrow; benchmark; droppable visual queue; shared-memory spike only if needed |
| R-08 | Недетерминизм SUMO/платформ | средняя/высокое | golden flakes | pinned versions; semantic tolerances; manifest; separate platform baselines |
| R-09 | Траектории переполняют диск/RAM | высокая/среднее | large files/backpressure | opt-in/sampling/ROI; streaming Parquet; quotas |
| R-10 | Crate fragmentation тормозит работу | средняя/среднее | boilerplate/cyclic pressure | start consolidated; split by boundary evidence |
| R-11 | ИИ-агенты создают несовместимые abstractions | высокая/высокое | параллельные duplicate APIs | work packets; contract owner; small PR; source IDs; architecture review |
| R-12 | Golden tests закрепляют неверное поведение | средняя/критическое | snapshots обновляются часто | independent/domain review; diff report; manual update gate |
| R-13 | OpenDRIVE обещает ложный round-trip | высокая/среднее | lost extensions/geometry drift | subset matrix; loss report; block unknown critical content |
| R-14 | Plugin API стабилизирован слишком рано | средняя/высокое | domain leaks into WIT | defer stable SDK; draft worlds; no compatibility promise before post-MVP |
| R-15 | Scope creep к полной AnyLogic parity | высокая/критическое | новые modes до VS-05 | enforce MVP non-goals; milestone gate; separate roadmap |
| R-16 | Native dependencies увеличивают attack surface | средняя/высокое | parser crashes/CVEs | workers, limits, fuzzing, SBOM, update policy |

Риски пересматриваются на каждом milestone. Риск без владельца и следующего действия считается blocker для соответствующего gate.

## 32. Контрольные ревью

### Gate G0 — Foundation review

После M0: лицензии, CI, dependency policy, docs authority.

### Gate G1 — Domain freeze v1

Перед storage/editor expansion: IDs, units, CRS, reference line, commands. Freeze означает versioned change process, а не запрет исправлений.

### Gate G2 — Geometry/editor usability

После M2: дорожный инженер проходит базовый workflow; измерены render/compile budgets; принимается решение по egui docking/tool architecture.

### Gate G3 — CSN/backend boundary

Перед libsumo mapping: source map, capabilities, lifecycle, unsupported policy и determinism review.

### Gate G4 — Simulation validity

После M5: golden behavior, SUMO mapping, metric raw sources и ограничения tram/bus/pedestrian.

### Gate G5 — Data/reproducibility

После M6: schemas, manifests, paired seeds, Parquet, migration policy.

### Gate G6 — Security/release

Перед RC: threat model, fuzz/chaos, licenses, installers, SBOM, rules coverage, known limitations.

## 33. PR policy

Каждый PR должен:

- иметь один основной task/work packet;
- перечислять `FR/NFR/ADR` и acceptance;
- описывать пользовательское/архитектурное поведение до и после;
- отмечать schema/ruleset/metric/protocol changes;
- содержать test evidence и manual QA, если UI;
- иметь screenshot/video только как дополнение, не вместо теста;
- не превышать разумный review scope; крупное изменение разбивается на contract → implementation → integration;
- не включать generated/binary artifacts без причины и license metadata.

Рекомендуемый трехшаговый шаблон для сложной функции:

1. **Contract PR:** types/schema/errors/tests с fake implementation.
2. **Implementation PR:** subsystem implementation без UI polish.
3. **Integration PR:** application/UI/docs/end-to-end fixture.

## 34. Управление изменениями source of truth

- Новое продуктовое требование получает новый `FR/NFR`; ID не переиспользуется.
- Архитектурный trade-off оформляется ADR и связывается с задачей.
- Schema breaking change обновляет format docs, migration, corpus и changelog.
- Metric change получает новую definition version; старые results не переинтерпретируются молча.
- Ruleset change создает новую immutable версию и coverage diff.
- Изменение milestone scope фиксируется в этом плане с причиной и влиянием на MVP acceptance.

## 35. План после MVP

После M8 новые линии запускаются только с отдельными validation baselines:

1. native Rust simulation backend;
2. stable Wasm plugin SDK и publisher trust model;
3. stable Python SDK/service mode;
4. калибровка по реальным потокам;
5. actuated/adaptive signals;
6. велосипеды, парковка и расширенный общественный транспорт;
7. сети больше 500 × 500 м и распределенные experiments;
8. AI-assisted import, разметка и объяснение findings;
9. дополнительные юрисдикции/rulesets;
10. 3D viewer при сохранении 2D engineering truth.

Native backend не считается «готовым», пока не пройдет общий backend contract suite, golden scenarios, calibration comparison и independent validation.

## 36. Итоговая готовность MVP

Перед публикацией maintainer формирует evidence index со ссылками на:

- каждый критерий `PROJECT_SPEC.md §12`;
- CI runs трех ОС;
- ruleset coverage и domain review;
- golden scenario results;
- performance/determinism/security reports;
- schema/migration compatibility;
- license/SBOM/third-party notices;
- clean-install user walkthrough;
- список known limitations и unsupported features.

Без evidence criterion считается невыполненным. Решение «ship with exception» допустимо только для некритического требования, если исключение публично, имеет владельца, срок и не противоречит MUST-границе MVP.
