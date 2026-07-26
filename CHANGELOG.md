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
- Добавлены backend-independent typed UUID/object references, проверяемые
  метрические value types и integer simulation ticks.
- Добавлен корневой domain contract проекта с обязательными CRS, local origin,
  axis order, vertical datum и provenance; degree/foot declared engineering
  units отклоняются стабильным diagnostic code.
- Добавлен backend-independent reference-line contract для line, signed-curvature
  arc и linear-curvature transition с canonical heading, производными station
  ranges и явной классификацией boundary continuity.
- Добавлен `roadsim-geometry` с caller-owned tolerance/integration context,
  аналитической evaluation line/arc, bounded Simpson evaluation переходной
  кривой, signed offsets, lane cross-section continuity evidence и
  детерминированной классификацией segment intersections. Derived overflow и
  offset singularity возвращают стабильные ошибки вместо panic или repair.
- Geometry kernel получил exact cubic Bézier control contract и bounded adaptive
  tessellation с caller-provided chord error, depth и point limit. Polyline
  остаётся производной и не заменяет инженерную кривую.
- Добавлена corridor/cross-section Design Model: corridor-local lane catalog,
  stable lane IDs, semantic direction/use, ordered left/right lane slices и
  piecewise-constant widths по station без meshes или backend IDs.
- Добавлен отдельный `roadsim-commands` contract с atomic working-copy
  transactions, monotonic model revision, stable command diagnostics и
  типизированными create/update/delete corridor operations. Structural catalog
  validation выполняется один раз перед публикацией полного command batch.
- Corridor commands теперь возвращают inverse operations; state-bound
  `CommandHistory` предоставляет bounded undo/redo, сохраняет stable IDs и
  записывает multi-command transaction как одно history entry.
- Design Model расширена backend-independent junction approaches, walking areas,
  corridor-attached sidewalks/crossings и rail alignments с typed UUID refs.
- Traffic control model получил явный `SignalMovementBinding` от signal group к
  паре stable Design lane IDs. Binding не использует compact CSN/backend IDs,
  сортируется детерминированно и сохраняется при serde round-trip.
- Добавлена backend-independent модель знаков, разметки, стоп-линий, сигнальных
  групп/головок, фаз, программ и контроллеров с typed object references и
  проверкой длительностей, intergreen и связей с junction/corridor/lane.
- Добавлен project-level skeleton интервального demand, scenarios и experiments:
  rates имеют явную единицу «в час», timing разделяет warm-up/duration/step/output
  sampling, а каждый одиночный run и replication получает явный root seed.
- Добавлены exact ruleset pins и аудируемые rule exceptions, привязанные к object
  и ruleset SHA-256; изменение любого hash наблюдаемо переводит exception в stale.
  Нормативные требования и трактовки в этот contract не включены.
- Desktop entry point теперь открывает нативное `winit` окно, создаёт `wgpu`
  surface через Metal/DX12/Vulkan и рисует первый `egui` shell с дорожным
  viewport, object tree и inspector.
- Добавлены backend-independent `roadsim-compiled-network` и первый staged
  `roadsim-compiler`: прямой corridor компилируется в compact lane arrays,
  source map, stable content hash и capability requirements; неподдерживаемые
  кривые/переменные сечения возвращают object-linked diagnostics.
- Добавлены versioned `roadsim-backend-api` и deterministic in-memory fake
  backend: capability preflight, distinct handshake/compile/runtime errors,
  pause/resume/cancel lifecycle и batched agent frames на integer ticks.
- Desktop demo проходит полный Design Model → CSN → fake backend → frame overlay
  путь; Start/Pause/Resume/Stop управляют наблюдаемым run, а native smoke требует
  первый batch из 18 агентов, поэтому GPU-only false green больше невозможен.
- Агенты fake simulation теперь несут проверяемый метрический footprint и
  отображаются ориентированными прямоугольниками размера легкового автомобиля,
  а не условными точками.
- Добавлен versioned worker control prototype: bounded JSON framing по inherited
  process pipes, одноразовый handshake token, capability preflight, correlation
  и lifecycle harness для cancel/crash/hang/timeout без сетевого listener. Это
  первый control protocol v1; предыдущей версии и migration нет.
- Добавлен data protocol v1 baseline на отдельном inherited pipe: SoA visual
  batches допускают наблюдаемое latest-wins dropping, а versioned metrics и
  terminal events используют bounded reliable backpressure без per-agent IPC.
- Добавлен worker run-directory journal schema v1 и bounded manager: child
  запускается в отдельном generated workdir, interrupted state восстанавливается
  как `Incomplete`, а retention не удаляет `Failed/Incomplete` молча. Это первый
  marker format; предыдущей версии и migration нет.
- Закреплён SUMO `1.27.1` source tag/commit, headless four-target build matrix и
  pending distribution status; CI отклоняет floating/short pin, optional GPL
  extras и неполную NFR-030 matrix.
- Добавлен отдельный `sumo-worker` и versioned native C ABI для exact libsumo
  identity и lifecycle start/step/close. ABI fixture проверяет process isolation,
  а opt-in macOS arm64 smoke собирает минимальную сеть и выполняет реальный
  пятишаговый run на headless SUMO `1.27.1`; platform packaging остаётся E10-T11.
- Добавлен `roadsim-backend-sumo` straight-network exporter: explicit speed,
  deterministic plain XML, lossless CSN/Design→SUMO lane mapping и object-linked
  rejection неподдерживаемых lane uses. Export проходит exact `netconvert` и
  реальный пятишаговый worker/libsumo smoke.
- SUMO worker теперь после каждого `StepSession` собирает vehicle state одним
  bounded native batch и публикует protocol-v3 SoA visual frame на отдельном
  data pipe. SUMO front-position преобразуется в центр footprint, navigation
  angle — в математический heading; runtime IDs принимаются только в форме
  `rs_agent_<u32>`.
- CSN расширена compact directed lane/pedestrian graphs с source mapping до
  junction approaches, walking areas, sidewalks и crossings. Compiler проверяет
  car/bus/pedestrian demand reachability до backend и возвращает object-linked
  diagnostic для disconnected endpoints.
- CSN получила детерминированную таблицу semantic junction movements с compact
  IDs. Compiler выводит однозначные lane-to-lane движения из junction approaches
  и блокирует неоднозначный выбор нескольких полос одного target corridor с
  object-linked diagnostic вместо неявной эвристики.
- CSN получила exact cubic connector curves, bounded derived tessellation и
  sparse symmetric conflict matrix. Compiler находит crossing/overlap между
  movement centerlines в пределах одного junction, строит консервативные
  width-expanded conflict AABB и останавливается стабильной object-linked
  diagnostic при превышении явных лимитов points/segment tests.
- CSN получила travel-oriented stop positions и полный минимальный fixed-time
  control snapshot: signal groups привязаны к compact movements, программы
  сохраняют authored phase order/states, а controllers — active program.
  Compiler блокирует unbound/unresolved/duplicate movement ownership и
  одновременно зелёные геометрически конфликтующие movements до backend.

- SUMO export теперь переводит compiled junction movements в полный explicit
  connection table: одно movement — ровно одна `<connection>`, узел с movements
  получает `type="priority"`, а `SUMO_NETCONVERT_INPUT_ARGUMENTS` отключает
  turnarounds и эвристические связи netconvert. `SumoConnectionMapping`
  сохраняет `CompiledMovementId → JunctionId → SUMO edge/lane`. Неполный набор
  movements, разорванные endpoints и один узел с двумя junction ID блокируются
  object-linked diagnostics; pedestrian graph и traffic controls отклоняются
  явными кодами вместо молчаливого удаления. ADR-022 фиксирует, что RoadSim не
  выдумывает junction priority, а right-of-way между зафиксированными связями
  считает pinned `netconvert 1.27.1`.

- SUMO export переводит активную fixed-time программу каждого контроллера в
  `roadsim.tll.xml`: `<tlLogic type="static">` с сохранённым порядком фаз,
  длительностями и per-group indication, узел `type="traffic_light"`, связи с
  `tl`/`linkIndex` в compact movement order и `SumoSignalMapping` для обратного
  отображения. Movement сигнализированного узла без группы, контроллер
  неизвестного узла и authored `intergreen > 0` отклоняются стабильными кодами;
  трактовка clearance между amber и all-red остаётся за domain owner.

- Добавлен backend-independent compiled demand contract (`CompiledDemandTable`,
  schema v1) и `compile_demand`: authored corridor endpoints резолвятся в
  единственную boundary lane опубликованной CSN, а неизвестный профиль,
  неоднозначный endpoint, недостижимая пара и нецелевой mode блокируются
  object-linked diagnostics. Спрос остаётся состоянием сценария и не входит в
  CSN.
- SUMO export переводит compiled demand в `roadsim.rou.xml`: один `<vType>` с
  явными габаритами и по одному `<flow>` на authored interval с сохранёнными
  `begin`/`end`/`vehsPerHour`; `SumoFlowMapping` хранит обратное отображение.
  Non-car режимы и demand, скомпилированный против другой сети, отклоняются.

- Зафиксирован пробел Design Model, блокирующий пешеходный SUMO export
  (ADR-023): между `WalkingArea` и `Sidewalk` нет typed связи, поэтому endpoint
  для `walk` нельзя построить без геометрической догадки. Экспорт продолжает
  явно отклонять пешеходную сеть и pedestrian demand.

- Добавлен `roadsim-application` с backend-agnostic `RunOrchestrator`: полный
  lifecycle одного run, единственный terminal outcome, перезапуск из terminal
  состояния и стабильные диагностики вместо panic на невозможном переходе.
  State machine не выполняет I/O и возвращает caller ровно один `RunIntent`.
- Desktop shell берёт enablement кнопок симуляции из оркестратора, поэтому UI
  не предлагает переход, который run отклонит.

- Добавлен `FrameSnapshotAdapter`: backend frame переводится в GPU-ready SoA с
  переиспользуемыми буферами и явным bound по числу агентов; отклонённый кадр
  не разрушает предыдущий snapshot, а backend agent/lane ID сохраняются.

### Changed

- SUMO plain-network export contract повышен с v2 до v3: bundle содержит
  четвёртый документ `roadsim.tll.xml`, `SUMO_NETCONVERT_INPUT_ARGUMENTS`
  включает `--tllogic-files`, а общий код
  `backend.sumo.traffic_controls.unsupported` заменён на
  `backend.sumo.stop_positions.unsupported`,
  `backend.sumo.signal_intergreen.unsupported`,
  `backend.sumo.signal_movement.unbound` и
  `backend.sumo.signal_junction.unknown`. Persisted export artifacts не
  публиковались, runtime migration не вводится.
- SUMO plain-network export contract повышен с v1 до v2: bundle содержит третий
  документ `roadsim.con.xml`, `export_straight_network` заменён на
  `export_network`, а код `backend.sumo.junction_movements.unsupported` удалён в
  пользу `backend.sumo.junction_movements.incomplete`,
  `backend.sumo.movement.endpoints_disconnected`,
  `backend.sumo.junction_node.ambiguous`,
  `backend.sumo.pedestrian_network.unsupported` и
  `backend.sumo.traffic_controls.unsupported`. Persisted export artifacts ещё не
  публиковались, поэтому runtime migration не вводится; callers обязаны
  передавать `SUMO_NETCONVERT_INPUT_ARGUMENTS`.
- In-memory CSN schema повышена с v1 до v2: semantic content hash теперь включает
  lane/pedestrian adjacency и их source maps. V1 runtime migration не вводится,
  поскольку persisted CSN artifacts ещё не публиковались.
- In-memory CSN schema повышена с v2 до v3: semantic content hash теперь включает
  junction movements, а `CompiledNetwork::new_with_graphs` требует, чтобы каждое
  movement имело backing edge в coarse lane graph. Persisted CSN artifacts
  по-прежнему не публиковались, поэтому runtime migration не вводится. Straight
  SUMO exporter явно отклоняет movements до E10-T04 вместо silent topology
  downgrade.
- In-memory CSN schema повышена с v3 до v4: semantic content hash включает exact
  movement curves и conflict zones, `compile_project` требует явный numerical/
  resource policy, а `CompiledNetwork::new_with_graphs` принимает сгруппированный
  `CompiledTopology`. Persisted CSN artifacts ещё не публиковались, поэтому
  runtime migration не вводится; priority/yield и SUMO junction export остаются
  отдельными последующими стадиями.
- In-memory CSN schema повышена с v4 до v5: semantic content hash включает stop
  positions, signal movement bindings, fixed-time programs/controllers и
  capability `signals.fixed_time`. `CompiledNetwork::new_with_graphs` теперь
  принимает отдельный `CompiledControlTable`. Persisted CSN artifacts всё ещё не
  публиковались, поэтому runtime migration не вводится; SUMO TLS export остаётся
  E10-T05 и до него capability preflight не допускает silent signal downgrade.

- Worker control/data protocol повышен с v1 до v2: handshake обязательно
  публикует bounded exact engine name/version/build revision и может потребовать
  точное совпадение до открытия session. V1 schemas сохранены для истории;
  runtime migration/downgrade отсутствует, несовпадение версии отклоняется.
- Worker protocol повышен до v3 с backend-neutral relative bundle config и
  отдельными `OpenSession`, `StepSession`, `CloseSession`; `Ping` остаётся только
  health check. V1/v2 schemas неизменяемо сохранены, downgrade отсутствует.
- Внутренний native SUMO bridge ABI повышен с v1 до v2 добавлением bounded
  vehicle batch collector. Worker намеренно отклоняет ABI v1 до handshake;
  worker control/data protocol остаётся v3 и migration не требует.

### Fixed

Нет.

### Security

- Включены deny-by-default checks неизвестных Cargo sources, RustSec advisories и
  лицензионной совместимости; GitHub Actions закреплены полными commit SHA.
- Ограничен размер текста, принимаемого domain-моделью; invariants finite values,
  bounded text и declared metric engineering CRS повторно проверяются при
  deserialize. Pre-allocation parser limits остаются частью E03 storage boundary.
- Reference-line constructors и deserialization отклоняют нулевые/вырожденные
  primitives, non-finite angular change, station overflow и потерю представимого
  station increment; tolerance непрерывности всегда передаётся вызывающей стороной.
- Worker control reader отклоняет empty, malformed и превышающий 1 MiB frame до
  payload allocation; version/auth/capability failures имеют стабильные codes.
- Worker runtime отклоняет symlink/unknown root entries, oversized/corrupt state
  journals и invalid lifecycle transitions до cleanup или публикации нового state.
- Corridor validation повторно проверяется при deserialization и отклоняет
  dangling/duplicate lane references, нулевые widths, неупорядоченные или выходящие
  за reference line sections и непредставимую суммарную ширину.
- Failed, aborted, empty, wrong-state, stale, domain-invalid и revision-overflow
  transactions не публикуют working project и не изменяют model revision;
  command envelopes привязаны к конкретной model lineage.
- History отклоняет чужую model lineage и незаписанные изменения revision;
  failed undo/redo не сдвигает history stacks.
- Aggregate validation отклоняет dangling corridor/walking-area refs, повторное
  подключение corridor endpoint и station за пределами reference line. Corridor
  commands сохраняют несвязанные multimodal entities и не удаляют referenced data.
