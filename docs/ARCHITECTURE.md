# Архитектура RoadSim

> Статус: целевая архитектура и ограничения реализации
> Версия документа: 0.1.0
> Связанные документы: [PROJECT_SPEC.md](PROJECT_SPEC.md), [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md)

## 1. Назначение и архитектурные драйверы

Документ определяет, **как** реализуется продукт из `PROJECT_SPEC.md`. Он обязателен для людей и coding agents: публичные границы, зависимости и инварианты нельзя обходить ради локального упрощения.

Ключевые драйверы:

- кроссплатформенный Rust-native desktop;
- отзывчивый CAD-подобный 2D-редактор;
- независимая от backend предметная модель;
- воспроизводимая микросимуляция через заменяемый API;
- SUMO/libsumo как первый backend в отдельном процессе;
- нормативная проверка как самостоятельный versioned subsystem;
- открытые форматы проектов и результатов;
- безопасные недоверенные файлы и будущие плагины;
- возможность заменить backend и расширить масштаб без переписывания UI.

## 2. Архитектурные решения

| ID | Решение | Статус |
|---|---|---|
| ADR-001 | Основной язык и workspace — stable Rust | принято |
| ADR-002 | Desktop shell: `winit`; GPU: `wgpu`; инструментальный UI: `egui` | принято |
| ADR-003 | Design Model и Compiled Simulation Network — разные типы и crates | принято |
| ADR-004 | UI меняет модель только через типизированные команды | принято |
| ADR-005 | SUMO интегрируется отдельным worker-процессом, владеющим libsumo | принято |
| ADR-006 | Backend реализует versioned `SimulationBackend` contract | принято |
| ADR-007 | `.roadsim` — ZIP-контейнер с manifest, schema и защитными лимитами | принято |
| ADR-008 | OpenDRIVE — interchange, не внутренний формат | принято |
| ADR-009 | Результаты — Arrow in-memory/IPC и Parquet on disk | принято |
| ADR-010 | Геометрия работает в локальной метрической CRS; преобразования через PROJ | принято |
| ADR-011 | Тяжелый/недоверенный GIS import выполняется GDAL worker | принято |
| ADR-012 | Публичные плагины — Wasmtime Component Model + WIT + capabilities | принято после MVP |
| ADR-013 | Python SDK управляет headless API и анализом, но не вызывается на каждый tick | принято после MVP |
| ADR-014 | Детерминизм — часть контрактов данных, scheduler и tests | принято |
| ADR-015 | Основной код — Apache-2.0 OR MIT; лицензионный review дистрибуции обязателен | предварительно |

Новые решения оформляются отдельными ADR в `docs/adr/NNNN-title.md`: context, options, decision, consequences, migration.

## 3. Системный контекст

```text
                 ┌──────────────────────┐
 GIS/OpenDRIVE ─▶│                      │◀─ нормативные пакеты
 полевые данные ─▶│  RoadSim Desktop     │◀─ плагины Wasm
                 │  + Headless CLI      │
                 └──────┬───────┬───────┘
                        │ IPC   │ files
                 ┌──────▼───┐  └────────▶ Arrow/Parquet/Reports
                 │ SUMO     │
                 │ worker   │
                 └────┬─────┘
                      ▼
                   libsumo
```

RoadSim Desktop — локальное приложение. Headless CLI использует те же application/domain crates, а не альтернативную реализацию. Worker-процессы считаются недоверенными относительно стабильности: их падение не должно разрушать editor state.

## 4. Слои и правило зависимостей

```text
roadsim-app / roadsim-cli
        │
        ├── editor-ui ─── renderer
        │       │
        ├── application (commands, jobs, orchestration)
        │       │
        ├── domain (Design Model, scenarios, rules contracts)
        │       │
        ├── compiler ─── compiled-network
        │       │                 │
        ├── backend-api ◀─────────┘
        │       │
        ├── backend-sumo-client ── IPC ── sumo-worker/libsumo
        │
        └── storage / results / import-export / plugins
```

Правила зависимостей:

1. `domain` не зависит от UI, wgpu, SUMO, filesystem или async runtime.
2. `compiled-network` не зависит от Design Model mutable types и backend implementation.
3. `backend-api` зависит от versioned wire/domain DTO, но не от desktop UI.
4. `backend-sumo-client` не экспортирует libsumo types.
5. `editor-ui` не меняет модель напрямую; он отправляет application commands.
6. `renderer` принимает read-only render snapshots, а не mutex на Design Model.
7. import/export никогда не становится обходным путем для записи невалидной модели.
8. rules engine работает с semantic view Design Model/CSN и не зависит от пикселей.

Циклические зависимости запрещены и проверяются workspace tooling/обзором graph.

## 5. Рекомендуемая структура workspace

```text
roadsim/
├── Cargo.toml
├── rust-toolchain.toml
├── LICENSE-APACHE
├── LICENSE-MIT
├── README.md
├── CONTRIBUTING.md
├── SECURITY.md
├── CODE_OF_CONDUCT.md
├── docs/
│   ├── PROJECT_SPEC.md
│   ├── ARCHITECTURE.md
│   ├── IMPLEMENTATION_PLAN.md
│   ├── adr/
│   ├── formats/
│   ├── metrics/
│   └── performance-baseline.md
├── crates/
│   ├── roadsim-types/
│   ├── roadsim-domain/
│   ├── roadsim-commands/
│   ├── roadsim-geometry/
│   ├── roadsim-topology/
│   ├── roadsim-compiler/
│   ├── roadsim-compiled-network/
│   ├── roadsim-rules-api/
│   ├── roadsim-rules-engine/
│   ├── roadsim-rules-ru/
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
├── wit/
├── python/
│   └── roadsim-sdk/
├── rulesets/
│   └── ru/
├── schemas/
│   ├── roadsim-project/
│   ├── worker-protocol/
│   └── results/
├── fixtures/
│   ├── projects/
│   ├── geometry/
│   ├── rules/
│   ├── opendrive/
│   ├── malicious/
│   └── golden-scenarios/
├── benchmarks/
├── examples/
├── packaging/
└── .github/workflows/
```

На старте допускается объединить мелкие crates, если границы сохранены модулями. Разделение выполняется при появлении независимого API, различного dependency weight или необходимости запретить зависимость. Не следует создавать десятки пустых crates до вертикального среза.

## 6. Модель идентичности, версий и ошибок

### 6.1. Идентификаторы

- все persistable сущности получают typed UUID newtype (`RoadId`, `LaneId`, `ScenarioId`);
- UUID не кодирует тип, время, положение или backend;
- compiled entities получают компактный index ID и обязательный `OriginRef` на один/несколько Design UUID;
- backend IDs генерируются детерминированно из compact IDs и таблицы mapping;
- UI selection и findings всегда ссылаются на Design UUID.

### 6.2. Версии

Различаются:

- application version;
- `.roadsim` container version;
- Design Model schema version;
- CSN schema version;
- worker protocol version;
- backend adapter version и engine version;
- ruleset version;
- results schema version;
- plugin WIT world version.

Их нельзя заменять одним `version`. Major несовместимость отклоняется на handshake/open; minor capability согласуется явно.

### 6.3. Ошибки

Публичная ошибка имеет структуру:

```rust
struct Diagnostic {
    code: DiagnosticCode,
    severity: Severity,
    message_key: String,
    args: BTreeMap<String, Value>,
    object_refs: Vec<ObjectRef>,
    source_span: Option<SourceSpan>,
    hint_keys: Vec<String>,
    cause_chain: Vec<SanitizedCause>,
}
```

`code` стабилен и пригоден для CLI/тестов. Локализованный текст не используется для логики. Внутренние ошибки дополняются correlation ID; секреты и полные пути очищаются перед diagnostic bundle.

## 7. Design Model

### 7.1. Инварианты

Design Model — источник истины для редактируемого проекта. Она:

- семантическая, параметрическая и сериализуемая;
- не содержит GPU handles, widget state, libsumo objects или кэшируемых meshes;
- допускает временно неполное состояние только внутри editor transaction;
- после commit обязана пройти structural validation;
- хранит явные единицы, CRS, provenance и stable IDs;
- отделяет authoring data от derived caches.

### 7.2. Агрегаты

Предлагаемые корневые агрегаты:

```text
Project
├── Metadata + CoordinateReference
├── DesignCatalog
│   ├── Corridors / Junctions
│   ├── WalkingAreas / Crossings
│   ├── RailAlignments
│   ├── TrafficControlDevices
│   └── Detectors / References
├── Variants
├── DemandProfiles
├── SignalPrograms
├── Scenarios / Experiments
├── RuleConfiguration / Exceptions
└── ResultReferences
```

Variant должен быть определен как immutable base revision + command/change set либо как content-addressed snapshot с deduplication. Конкретный механизм выбирается ADR после прототипа; API не должен требовать полного копирования модели.

### 7.3. Геометрия

- reference line состоит из типизированных сегментов с диапазоном station `s`;
- профиль задает элементы и ширины как piecewise functions по `s`;
- производные границы полос вычисляются согласованным offset algorithm;
- `f64` используется в authoring/geometric predicates;
- tolerance задается контекстом операции, не глобальной магической константой;
- robust predicates и snap rounding применяются в топологических операциях;
- любой автоматический repair возвращает список изменений и evidence.

Минимальные библиотеки: `glam` для math types, `geo` для общих операций, `rstar` для spatial index. Критичные кривые, offset и topology должны иметь собственный контролируемый слой, чтобы семантика не зависела от смены библиотеки.

### 7.4. Команды и undo/redo

```rust
trait Command {
    fn validate(&self, ctx: &CommandContext) -> Result<(), Vec<Diagnostic>>;
    fn apply(&self, tx: &mut ModelTransaction) -> Result<CommandOutcome, CommandError>;
}

struct CommandOutcome {
    changed: BTreeSet<ObjectId>,
    created: Vec<ObjectId>,
    deleted: Vec<Tombstone>,
    inverse: InverseCommand,
    invalidated_caches: CacheMask,
}
```

Команды имеют domain intent (`SplitCorridor`, `AddCrossing`, `SetPhaseDuration`), а не UI intent (`DragPoint`). Drag может создавать preview, но commit — одна команда. Undo восстанавливает ID и связи. Command log не считается долгосрочным event store до отдельного ADR.

## 8. Компилятор и Compiled Simulation Network

### 8.1. Pipeline

```text
Design snapshot
  → normalize and structural validation
  → evaluate geometry
  → build semantic lane/walk/rail elements
  → infer/validate junction movements
  → create connectors and movement curves
  → compute conflicts and priority relations
  → bind signals, stops and demand zones
  → build spatial/lookup indices
  → backend-independent validation
  → immutable CSN + SourceMap + diagnostics
```

Каждая стадия принимает immutable input и возвращает typed output. Ошибка стадии не должна оставлять частично обновленный shared state. Pipeline может кэшировать стадии по content hash, но полная и инкрементальная компиляции обязаны давать эквивалентный CSN.

### 8.2. Состав CSN

```text
CompiledNetwork
├── header: schema, source revision, coordinate frame, content hash
├── lane arrays: geometry, width, speed, permissions, adjacency
├── pedestrian arrays: links, areas, crossings
├── rail arrays
├── junctions and movement connectors
├── conflict zones and priority matrix
├── traffic controls and signal bindings
├── stops and detector bindings
├── spatial indices
├── source map
└── capability requirements
```

Горячие данные хранятся преимущественно в SoA и индексируются compact IDs. Строки и UUID вынесены из горячих массивов. CSN read-only после создания и передается через `Arc`; mutability существует только в runtime state backend.

### 8.3. Capability requirements

Compiler маркирует необходимые возможности, например:

```text
pedestrians.basic
transit.bus_stops
rail.tram.basic
signals.fixed_time
junction.priority
```

Backend до запуска сравнивает их со своим capability manifest. Неподдерживаемая возможность — структурированная ошибка, а не silent downgrade.

## 9. Нормативная подсистема

### 9.1. Компоненты

- `roadsim-rules-api`: typed contracts, finding, evidence, coverage;
- `roadsim-rules-engine`: выбор применимых правил, исполнение, cache, exceptions;
- `roadsim-rules-ru`: reviewed built-in predicates/checks;
- `rulesets/ru`: metadata, parameters, localized explanations, fixtures;
- `RuleRegistry`: immutable registry по ruleset version.

Полностью произвольный expression language в MVP не требуется: он повышает риск неправильной трактовки и создает второй язык. Простые таблицы/параметры могут быть declarative; сложные checks — type-safe Rust functions с review. Метаданные и tests обязательны в обоих случаях.

### 9.2. Выполнение

Rules engine получает `SemanticRuleView`, построенный из Design Model и, при необходимости, CSN. Он не имеет права менять модель. Autofix формирует обычную proposed command. Findings сортируются детерминированно по `rule_id/object_id/location`.

### 9.3. Исключения

Exception связывается с:

- `rule_id` и exact ruleset version/range;
- object UUID;
- hash релевантных полей/evidence;
- автором, временем, основанием и сроком пересмотра.

Изменение релевантных данных переводит exception в `stale`. Suppression без аудита запрещен.

## 10. SimulationBackend API

### 10.1. Контракт

Публичный API должен оставаться небольшим и не копировать TraCI:

```rust
#[async_trait]
pub trait SimulationBackend: Send + Sync {
    async fn handshake(&self, client: ClientHello) -> Result<BackendHello>;
    async fn compile(
        &self,
        network: Arc<CompiledNetwork>,
        scenario: ScenarioSnapshot,
        options: CompileOptions,
    ) -> Result<BackendArtifact, BackendError>;
    async fn start(
        &self,
        artifact: BackendArtifact,
        run: RunConfig,
    ) -> Result<Box<dyn SimulationSession>, BackendError>;
}

#[async_trait]
pub trait SimulationSession: Send {
    fn metadata(&self) -> &RunMetadata;
    async fn control(&mut self, command: ControlCommand) -> Result<Ack, BackendError>;
    async fn next_event(&mut self) -> Result<BackendEvent, BackendError>;
    async fn cancel(&mut self) -> Result<RunSummary, BackendError>;
}
```

`BackendEvent`: state transition, progress, frame batch, metric batch, diagnostic, log event, completed. Управление имеет sequence number и idempotency key. API не обещает, что каждый backend может pause/checkpoint; это capabilities.

### 10.2. RunConfig

Обязательные поля:

- scenario/content hashes;
- duration, warm-up, tick/step, output sampling interval;
- root seed и algorithm version;
- requested outputs и metric definitions;
- resource limits;
- deterministic mode;
- backend-specific options в namespaced, schema-validated extension, не влияющей на общие поля.

### 10.3. Lifecycle

```text
Created → Compiling → Ready → Running ↔ Paused
                         ├──────────────→ Completed
                         ├──────────────→ Failed
                         └──────────────→ Cancelling → Cancelled
```

Переходы проверяются state machine. После terminal state session не возвращает новые data batches. Finish требует flush результатов и подтвержденный manifest; crash оставляет `incomplete` marker.

## 11. SUMO/libsumo worker

### 11.1. Почему отдельный процесс

- изоляция C++ crash/ABI;
- независимый lifecycle и несколько replication;
- явная лицензионная/packaging граница;
- ограничение CPU/memory/workdir;
- возможность заменить worker native Rust backend;
- UI не зависит от libsumo headers/types.

### 11.2. Внутренняя структура

```text
roadsim-backend-sumo (Rust client)
        │ framed local IPC
        ▼
sumo-worker
├── protocol adapter
├── CSN → SUMO compiler
├── backend ID source map
├── libsumo lifecycle owner
├── batch subscription/state collector
├── result translator
└── watchdog/diagnostics
```

Worker — единственный владелец libsumo instance. Один process обслуживает один активный run, если документированная версия libsumo не гарантирует безопасную многосессионность. Параллельные replication запускаются отдельными процессами через bounded scheduler.

### 11.3. IPC

Транспорт выбирается кроссплатформенно: Unix domain sockets на Unix и named pipes на Windows либо переносимая локальная socket abstraction. Протокол:

- length-delimited frames;
- schema/version handshake;
- request ID, session ID, sequence number;
- max message size и timeouts;
- heartbeat/watchdog;
- control messages — компактный schema-first формат (например Protobuf);
- большие кадры — Arrow IPC record batches или shared memory после отдельного ADR;
- backpressure: UI может пропускать промежуточные visual frames, но не final metrics/events.

Сетевой listener по умолчанию отсутствует. Worker принимает только заранее созданный локальный endpoint и одноразовый token.

### 11.4. SUMO compiler

Компилятор создает временный bundle в run workdir: network, routes/demand, signals, configuration и mapping. Bundle content-addressed и включается hash в manifest. XML генерируется typed writer, значения валидируются, external entities запрещены.

Loss/unsupported report обязателен. Маппинг сохраняет:

```text
Design UUID ↔ CSN ID ↔ SUMO edge/lane/person/tls ID
```

### 11.5. State collection

Запросы к libsumo группируются; subscriptions применяются после benchmark. Worker формирует frame batch, например:

```text
tick
vehicle_id[] / x[] / y[] / heading[] / speed[] / class[]
person_id[] / x[] / y[] / heading[] / speed[]
signal_group_id[] / state[]
queue_sample[]
```

Render sampling отделен от simulation step. UI может отображать интерполяцию между полученными кадрами, не меняя метрики.

## 12. Desktop shell, UI и rendering

### 12.1. Потоки

- main thread: `winit` event loop и platform window requirements;
- UI/render coordination: `egui` + `wgpu` submission в рамках event loop;
- application job runtime: compile/import/save/backend I/O;
- worker processes: SUMO/GDAL;
- bounded CPU pool: geometry tessellation, rules, analytics.

Ни один background task не получает mutable reference на UI. Результаты возвращаются versioned messages; устаревший результат, рассчитанный для другой model revision, отбрасывается.

### 12.2. UI state

UI state отделен от project state: раскладка панелей, камера, hover, временный tool preview не сериализуются в Design Model. Допустима отдельная user/workspace settings schema.

### 12.3. Renderer

- roads/markings: cached meshes по model revision;
- vehicles/pedestrians: instanced rendering и compact GPU buffers;
- signs/symbols/text: atlas/vector layer;
- selection/snap/diagnostics: overlay passes;
- picking: spatial index на CPU, GPU picking только при доказанной необходимости;
- origin rebasing для больших координат;
- device loss восстанавливает GPU resources из CPU caches.

Renderer не определяет геометрическую истину. Tessellation может быть приближенной для экрана; инженерные измерения выполняются geometry layer.

## 13. Формат `.roadsim`

### 13.1. Логическая структура

```text
project.roadsim (ZIP)
├── manifest.json
├── model/model.json
├── demand/*.json|arrow
├── scenarios/*.json
├── standards/pins.json
├── exceptions/*.json
├── assets/<content-hash>.*
├── results/index.json
├── results/<run-id>/...        # optional or external references
└── cache/...                   # optional, disposable
```

`manifest.json` содержит container/schema versions, project UUID, timestamps, required features, entry hashes, compression, application provenance и optional signatures. Архивные пути нормализованы и относительны.

### 13.2. Инварианты чтения

- сначала central directory и manifest, затем лимиты/версии;
- запрещены absolute paths, `..`, symlink/hardlink и duplicate normalized names;
- лимиты на число entries, compressed/uncompressed size, ratio и nesting;
- hash обязательных entries проверяется до десериализации;
- JSON depth/string/array limits;
- unknown optional fields сохраняются только по явно спроектированному extension mechanism;
- проект открывается в staging; application state заменяется только после полной проверки.

### 13.3. Запись и миграции

- запись в sibling temporary file;
- flush/fsync по поддерживаемой платформе;
- atomic rename;
- backup/recovery policy;
- canonical serialization для hashes/golden tests;
- миграция создает новый artifact и migration report;
- cache/results могут храниться отдельно при больших объемах.

JSON выбран для authoring/interoperability, но большие массивы demand/results могут использовать Arrow. Внутренний Rust struct layout не является file format.

## 14. OpenDRIVE и другие import/export

OpenDRIVE 1.9 поддерживается как versioned subset. Для каждой сущности действует status:

- lossless round-trip;
- imported with documented normalization;
- exported with documented approximation;
- unsupported and blocking.

Importer строит промежуточную `ImportModel`, выполняет validation/CRS decision и только затем генерирует command batch в Design Model. Unknown extensions не исполняются. Exporter создает loss report с object IDs.

SUMO XML — только backend artifact/export. GeoJSON подходит для легких векторных импортов; GeoPackage и растр идут через GDAL worker.

## 15. GIS: PROJ и GDAL worker

### 15.1. Coordinate model

Project хранит:

- исходную CRS (authority/code или WKT при необходимости);
- локальную метрическую engineering CRS;
- origin/transform и axis-order decision;
- unit и vertical datum status;
- provenance преобразования.

Design/CSN используют локальные метры. Широта/долгота не участвуют напрямую в offsets/intersections.

### 15.2. GDAL worker

GDAL изолируется из-за размера, нативных зависимостей, сложных форматов и attack surface. Worker:

- получает read-only input и explicit import options;
- пишет только в temporary workdir;
- запрещает сетевые virtual filesystems по умолчанию;
- имеет time/memory/output limits;
- возвращает нормализованные Arrow/GeoJSON batches + diagnostics;
- не меняет проект напрямую.

## 16. Результаты: Arrow и Parquet

### 16.1. Схемы

Минимальные datasets:

```text
vehicle_trajectory(run_id, tick, agent_id, x_m, y_m, speed_mps, lane_id)
vehicle_trip(run_id, agent_id, depart_s, arrive_s, travel_time_s, delay_s, stops)
pedestrian_trajectory(...)
lane_interval(run_id, lane_id, t0_s, t1_s, flow, mean_speed_mps, queue_max_m)
junction_interval(...)
signal_state(run_id, tick, group_id, state)
conflict_event(...)
simulation_summary(run_id, metric_id, value, unit, aggregation)
```

Каждое поле имеет nullability, unit, dictionary policy и semantic definition. `metric_id` ссылается на versioned документ `docs/metrics`.

### 16.2. Запись

- Arrow RecordBatch — внутренняя потоковая граница;
- Parquet — долговременное хранение;
- partitioning по experiment/variant/scenario/run/table;
- временный файл + atomic finalize;
- row group и compression выбираются benchmark;
- trajectory recording выключена по умолчанию либо sampling/ROI ограничены;
- schema metadata хранит units, coordinate frame и version.

`run_manifest.json` фиксирует hashes, versions, seeds, host compatibility info, lifecycle, warnings и datasets. Derived metrics содержат definition version и input dataset hashes.

## 17. Детерминизм

### 17.1. Источники недетерминизма

- RNG и seed derivation;
- iteration order collections;
- floating-point/parallel reductions;
- backend version/flags;
- wall clock и thread scheduling;
- unordered IPC arrival;
- изменение demand/rules/cache;
- platform math libraries.

### 17.2. Политика

- root seed — обязательный `u64`; substream получается из stable hash `(algorithm_version, root_seed, purpose, entity_id)`;
- RNG algorithm/version фиксируется в manifest;
- simulation time — integer tick; float seconds только представление;
- state-changing events сортируются stable key;
- parallel compute не может менять порядок применения;
- reductions используют фиксированное partition/order или tolerant acceptance;
- wall clock допускается для telemetry/progress, но не model behavior;
- backend artifact и capability manifest хэшируются;
- deterministic mode отклоняет неизвестные options.

Для SUMO заявляется воспроизводимость только в подтвержденной version/platform matrix. Native backend в будущем должен иметь более строгий contract.

### 17.3. Replay

MVP сохраняет inputs + manifest + outputs, но не обязан хранить event-sourced replay каждого tick. Debug event log может быть включен для golden scenarios. Visual replay из записанных trajectories не считается повторной симуляцией.

## 18. Плагины Wasmtime/WIT

Плагины вводятся после стабилизации domain API. Предлагаемые WIT worlds:

- `roadsim:validator` — read-only semantic snapshot → findings;
- `roadsim:importer` — byte/stream input → proposed model fragment;
- `roadsim:exporter` — snapshot → files/stream;
- `roadsim:metric` — result batches → derived batches;
- `roadsim:demand-generator` — scenario inputs + deterministic RNG service → demand;
- `roadsim:signal-controller` — только отдельный исследовательский API с budgets.

Capability grants: filesystem preopens, network allowlist, clock, random, memory, fuel, CPU deadline. По умолчанию ничего. Host валидирует весь plugin output как недоверенный. Plugin manifest содержит WIT version, permissions, publisher, checksum/signature и deterministic declaration.

Плагин не получает raw pointers, GPU device, libsumo handle или mutable Design Model. Вызов на каждого агента/tick запрещен для общего plugin API.

## 19. Python SDK и headless API

CLI и SDK используют один application service. Команды:

```text
roadsim validate <project> --ruleset RU-2026.07
roadsim compile <project> --variant A --backend sumo
roadsim run <project> --scenario AM --variant A --seed 42
roadsim experiment <project> --experiment compare-a-b
roadsim export <project> --format opendrive|sumo|parquet
roadsim inspect <project> --json
```

Exit codes и JSON output стабильны. Python SDK сначала может быть subprocess client к CLI; затем — клиент к локальному service/protocol. Он не связывается напрямую с internal Rust ABI и не внедряет Python interpreter в editor.

## 20. Безопасность

### 20.1. Trust boundaries

Недоверенными считаются `.roadsim`, ZIP/JSON/XML, GIS/OpenDRIVE, плагины, ruleset artifacts, worker output и imported assets. Доверенными — подписанный application binary и встроенные schema, но supply-chain риск остается.

### 20.2. Меры

- schema validation + semantic validation + resource limits;
- XML без DTD/external entities;
- path normalization и atomic staging;
- worker protocol allowlist, nonce/token, size/time limits;
- отсутствие shell interpolation; процессы запускаются через argument arrays;
- sandbox/OS restrictions по возможности без обещания одинакового механизма на всех ОС;
- Wasmtime fuel, epoch interruption, memory/table limits;
- dependency pinning, lockfile, `cargo audit`/`cargo deny`, SBOM;
- signed release artifacts/checksums;
- fuzzing parsers, migrations, geometry и protocol decoders;
- redaction diagnostic bundles;
- security disclosure policy.

Любая операция AI/автоматизации возвращает proposed commands; применение требует пользовательского consent. AI output не становится нормативным источником.

## 21. Наблюдаемость

- structured local logs с timestamp, level, component, event code, correlation/run ID;
- tracing spans: open/save, compile stages, worker handshake, run, export;
- метрики производительности локально доступны в diagnostics;
- log rotation и size limits;
- user-facing diagnostic bundle создается явно и проходит redaction preview;
- telemetry отсутствует/opt-in только после отдельной политики.

## 22. Стратегия тестирования

### 22.1. Пирамида

1. Unit tests: value types, commands, curves, rules, schema.
2. Property tests: geometry invariants, serialization round-trip, command undo/redo.
3. Snapshot/golden tests: CSN, diagnostics, import/export, metric definitions.
4. Contract tests: все `SimulationBackend` implementations на общем suite.
5. Integration tests: app ↔ worker, crash/cancel/timeouts, Parquet.
6. Scenario tests: инженерные golden scenarios.
7. UI smoke/e2e: ключевые workflows и screenshots в контролируемой среде.
8. Fuzz/security: `.roadsim`, XML, WIT/plugin outputs, IPC.
9. Performance: compile, render, IPC throughput, run/export.

### 22.2. Обязательные свойства

- `apply(command); apply(inverse)` восстанавливает semantic hash;
- save/open сохраняет semantic hash;
- full compile == incremental compile;
- одинаковые inputs/seed дают одинаковый accepted result;
- unsupported capability всегда обнаруживается до run;
- finding order стабилен;
- import failure не меняет project revision;
- worker crash не меняет Design Model;
- incomplete run не маркируется completed.

### 22.3. Golden scenarios

Минимум:

- straight free flow;
- car following and queue discharge;
- sharp turn speed behavior;
- priority/yield junction;
- pedestrian crossing yielding;
- fixed-time signal with conflicts;
- bus stop dwell and blockage;
- basic tram movement;
- disconnected demand rejection;
- variant A/B metric comparison.

Golden update требует явной команды, diff метрик, версии backend и одобрения domain reviewer. Автоматическая перезапись в обычном test run запрещена.

## 23. CI/CD

### 23.1. PR gates

- formatting and lints (`rustfmt`, `clippy -D warnings` с управляемыми исключениями);
- unit/property/integration tests;
- docs links/schema examples;
- dependency/license policy;
- no forbidden dependency edges;
- Linux worker integration;
- change-scope checks: schema/metric/ruleset требует changelog/fixtures;
- минимальный benchmark smoke без flaky hard threshold.

### 23.2. Nightly/weekly

- Windows/macOS/Linux full matrix;
- SUMO pinned-version contract suite;
- fuzzing corpus and sanitizers для C/C++ boundary where available;
- full golden scenarios;
- performance trend;
- installer/update smoke;
- malicious fixture suite;
- SBOM/vulnerability scan.

### 23.3. Release

- tagged, reproducible where practical builds;
- signed installers/binaries and checksums;
- bundled/pinned dependency manifest и license notices;
- schema/ruleset/backend compatibility matrix;
- migration tests from every supported project version;
- release notes с known limitations и нормативным coverage;
- clean-machine smoke на трех ОС.

## 24. Ресурсные и performance budgets

До измерений это design budgets:

- UI frame: target 16.7 мс, p95 ≤25 мс;
- application message handling: ≤2 мс/frame budget;
- background compile типового MVP: target ≤1 с, hard acceptance ≤2 с;
- IPC visual stream: bounded queue 2–3 frame batches, oldest visual frame droppable;
- metrics/events: nondroppable, backpressure to worker/disk;
- project open/save: streaming и bounded decompression;
- plugin invocation: explicit fuel/time/memory per call;
- batch scheduler: configurable max workers, default based on physical cores and memory.

Конкретные лимиты фиксируются в performance baseline после milestone M2.

## 25. Эволюция к native Rust backend

Native backend добавляется как новая реализация `SimulationBackend`, а не ветка UI. Этапы:

1. contract compliance на простом free-flow;
2. lane following/car-following;
3. lane change and routing;
4. junction priority/conflicts;
5. signals;
6. pedestrians;
7. transit/rail;
8. калибровка и сопоставление с SUMO/полевыми данными.

CSN и metrics должны быть пригодны обоим backend. Backend-specific extension допускается только namespaced и не может менять общие значения без manifest. Результаты разных engines не объявляются эквивалентными автоматически; validation report описывает расхождения.

## 26. Запрещенные архитектурные сокращения

- хранить source of truth в SUMO XML/OpenDRIVE;
- передавать libsumo type/ID в UI/domain;
- менять Design Model из renderer, importer, rule или backend;
- блокировать main thread на compile/run/export;
- сериализовать Rust memory layout;
- использовать wall-clock/random hash order в model behavior;
- запускать plugin/LLM на каждого агента в каждом tick;
- silently downgrade unsupported feature;
- считать cache частью обязательных данных проекта;
- выполнять команды, пути или код из входного файла;
- обновлять ruleset pin без явной миграции;
- сравнивать метрики разных definition versions без предупреждения.

## 27. Открытые вопросы, требующие ADR до реализации

- ADR-Q01: механизм variant storage — change sets или content-addressed snapshots;
- ADR-Q02: wire schema — Protobuf vs иной schema-first формат;
- ADR-Q03: cross-platform local IPC abstraction;
- ADR-Q04: JSON canonicalization и extension preservation;
- ADR-Q05: конкретные geometry algorithms/tolerances;
- ADR-Q06: policy распределения SUMO и EPL-2.0 по платформам;
- ADR-Q07: формат ruleset artifact и подпись;
- ADR-Q08: egui docking library или собственная раскладка;
- ADR-Q09: Arrow IPC vs shared memory после измерения;
- ADR-Q10: packaging/update mechanism.

Открытый вопрос не блокирует создание прототипа, если решение локально, обратимо и не попадает в публичный контракт. В противном случае сначала ADR/spike.

## 28. Архитектурная Definition of Done

Изменение считается архитектурно завершенным, когда:

- связано с `FR/NFR` или ADR;
- находится в правильном слое без запрещенной зависимости;
- публичные типы/ошибки/versioning документированы;
- happy path и failure/cancel/unsupported cases протестированы;
- determinism/security/resource limits рассмотрены явно;
- schema/migration/changelog обновлены при необходимости;
- performance измерен для горячего пути;
- пользовательская диагностика содержит object refs;
- docs и fixtures обновлены в том же PR;
- отсутствует скрытая зависимость от UI, platform или SUMO semantics.
