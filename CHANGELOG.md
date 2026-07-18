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
  surface через Metal/DX12/Vulkan и рисует первый `egui` shell со статическим
  дорожным viewport, object tree, inspector и simulation placeholder.

### Changed

Нет.

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
