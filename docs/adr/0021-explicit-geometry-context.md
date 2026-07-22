# ADR-021: Explicit geometry context and bounded evaluation

- Статус: accepted
- Дата: 2026-07-22
- Владельцы: geometry/domain maintainers
- Reviewers: architecture, domain, determinism
- Связанные требования: FR-011…FR-015, NFR-020, NFR-022, NFR-023
- Связанные задачи: E04-T01…T04, E07-T06, ADR-Q05

## Контекст

ADR-016 фиксирует authoring semantics line, circular arc и linear-curvature
transition, но намеренно не определяет numerical evaluation, offsets и
intersection predicates. Эти операции нужны compiler и renderer, при этом
global epsilon, platform libm и неограниченная quadrature нарушают требования к
явным tolerances, воспроизводимости и bounded work.

## Рассмотренные варианты

### Один глобальный epsilon и adaptive algorithms

API короче, но tolerance не учитывает масштаб/операцию, а adaptive branching
может менять порядок вычислений между платформами. Неявный предел работы также
создаёт риск зависания на недоверенной или near-degenerate геометрии.

### Tessellated polyline как геометрическая истина

Упрощает intersections и rendering, но смешивает zoom-dependent representation
с инженерной геометрией и нарушает ADR-003/ADR-016.

### Explicit context и контролируемый kernel

Caller передаёт размерные predicate tolerances и integration limits. Кривые,
offsets и predicates остаются за собственным API; будущая замена внутреннего
алгоритма не меняет Design Model.

## Решение

Принят третий вариант:

- `GeometryContext` не имеет default и содержит distance tolerance в метрах,
  orientation/cross-product tolerance в `m²`, dimensionless parameter tolerance,
  maximum integration step в метрах и чётный bounded panel count;
- caller выбирает context по масштабу и операции и обязан фиксировать его там,
  где результат влияет на semantic compile/golden output;
- line и circular arc вычисляются аналитически через pinned Rust `libm`;
- transition heading/curvature вычисляются аналитически, position — composite
  Simpson в стабильном порядке с обоими явными пределами работы;
- signed offset положителен влево по increasing station; `1 - offset·curvature`
  около нуля или отрицательный блокируется как singular без silent repair;
- piecewise cross-section evaluation сохраняет authored side/lane order и
  публикует explicit `continuous/offset_jump/starts/ends` evidence;
- segment predicate различает none/point/overlap, отдельно обрабатывает
  zero-length input и возвращает error при derived overflow вместо panic/NaN;
- predicate не изменяет координаты и не выполняет snap-rounding автоматически.

## Последствия и ограничения

Kernel не объявляется exact-arithmetic implementation. Для экстремальных finite
координат, где промежуточное `f64` переполняется, операция fail-closed; будущий
adaptive-exact predicate может заменить внутренность с тем же явным контрактом.
Точность transition position зависит от выбранного context и поэтому должна быть
частью compiler/golden policy. Tessellation, spatial index, snapping и automatic
repair остаются E04-T05…T08.

Новых внешних dependencies нет. `roadsim-geometry` зависит только от domain,
types и уже закреплённого `libm`; domain не зависит от derived geometry.

## Совместимость и миграция

Design Model, `.roadsim`, worker protocol, ruleset и metric schemas не меняются.
Это первый geometry-kernel API; persisted derived geometry отсутствует.

## Проверка

- analytic line/quarter-circle fixtures и exact heading/curvature transition;
- profile station, lane width/side/travel orientation и discontinuity evidence;
- crossing, touch, overlap, zero-length и overflow regression tests;
- property tests finite station evaluation и intersection order independence;
- exact Rust 1.88.0 fmt/clippy/test/rustdoc/dependency/license/security gates.

## Ссылки

- `docs/PROJECT_SPEC.md` FR-011…FR-015, NFR-020/NFR-022/NFR-023
- `docs/ARCHITECTURE.md` §7.3
- `docs/IMPLEMENTATION_PLAN.md` E04-T01…T04, E07-T06
- `docs/adr/0016-reference-line-authoring-contract.md`
