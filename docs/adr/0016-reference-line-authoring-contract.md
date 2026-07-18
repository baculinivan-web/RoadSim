# ADR-016: Контракт опорной линии в Design Model

- Статус: accepted
- Дата: 2026-07-18
- Владельцы: domain/geometry maintainers
- Reviewers: architecture, domain, security
- Связанные требования: FR-010, FR-012, NFR-020, NFR-022, NFR-023
- Связанные задачи: E01-T04, E04-T01, E04-T02

## Контекст

Design Model должен хранить backend-independent опорную линию до появления
renderer, CSN и SUMO adapter. Публичный выбор transition semantics нельзя оставить
локальной деталью: от него будут зависеть corridor profiles, команды, storage и
OpenDRIVE interchange. При этом numerical evaluation, offset algorithms и
operation-specific tolerance policy относятся к geometry kernel E04 и ещё не
стабилизированы.

## Драйверы решения

- Прямые, дуги и минимум одна гладкая переходная геометрия обязательны по FR-012.
- Авторские данные используют локальные метры и `f64`; NaN/Inf запрещены.
- Порядок сегментов и station ranges должны быть детерминированными.
- Нельзя дублировать вычисляемые start station/pose каждого сегмента и создавать
  расходящиеся источники истины.
- Continuity проверяется явно с caller-provided tolerance, без глобального epsilon.
- Domain contract не должен экспортировать renderer, `glam`, `geo`, CSN или SUMO
  types.

## Рассмотренные варианты

### Вариант A: Независимо привязанные line/arc/spline segments

Каждый сегмент хранит собственные start station, position и heading. Формат близок
к некоторым interchange-представлениям, но допускает противоречивые границы и
требует хранить производные значения.

### Вариант B: Последовательность curvature profiles от одного start pose

Reference line хранит одну начальную pose и ordered non-empty sequence. Line имеет
нулевую кривизну, circular arc — постоянную signed curvature, transition — signed
curvature, линейно меняющуюся по arc-length station. Station ranges, start pose и
heading последующих элементов выводятся в стабильном порядке.

### Вариант C: Полилиния или renderer spline как авторская истина

Это упростило бы ранний viewport, но не представляет инженерную line/arc semantics,
смешивает authoring truth с tessellation и нарушает FR-012/ADR-003.

## Решение

Принят вариант B. Публичный authoring contract использует:

- finite local-metric start point и canonical heading `[0, 2π)`;
- ordered non-empty sequence `line`, `circular_arc` и
  `linear_curvature_transition`;
- строго положительную длину каждого элемента;
- signed curvature в `1/m`, где положительный знак означает поворот влево;
- линейное изменение curvature transition по station;
- производные contiguous station ranges и один boundary report на каждую соседнюю
  пару.

Position и tangent continuity гарантированы compositional representation. Boundary
report отдельно классифицирует curvature continuity по явно переданному
non-negative tolerance. Прямой стык line→arc допустим как явно наблюдаемый curvature
jump; line→transition→arc с совпадающими curvature является непрерывным по curvature.

`linear_curvature_transition` задаёт authoring semantics, но не фиксирует numerical
quadrature, tessellation, offset или robust predicate algorithm. Эти решения и
tolerance contexts остаются за E04-T01/T02 и последующим дополнением ADR-Q05.

## Последствия

### Положительные

- Невозможно сериализовать расходящиеся start stations/poses соседних элементов.
- Один и тот же ordered input даёт одинаковые station ranges и boundary reports.
- Переходная геометрия выражена независимо от backend/interchange форматов.
- Curvature jump не скрывается и может быть диагностирован потребителем.

### Отрицательные и компромиссы

- Полная position evaluation transition откладывается до E04-T02.
- Импорт формата с независимо заданными start poses должен будет сравнить их с
  вычисленным результатом и сформировать loss/diagnostic report.
- Exact canonicalization запрещает zero-curvature arc и constant-curvature
  transition; импортёр должен нормализовать их явно в line/arc с report.

## Совместимость и миграция

Это первый reference-line contract; опубликованной `.roadsim` schema ещё нет.
Schema, migration, protocol, ruleset и metric definitions не меняются. Serde в
domain tests проверяет invariants, но не объявляет v1 file format: схема будет
зафиксирована в E03-T01.

## Безопасность и воспроизводимость

Constructors и deserialization отклоняют пустую линию, нулевую длину,
неканонический тип сегмента, NaN/Inf, overflow и потерю представимого приращения
производной station/heading.
Tiny positive segments не отклоняются магическим epsilon; near-degenerate
операции будут ограничены geometry context. Вычисления идут в порядке `Vec`; hash
map, wall clock, RNG, filesystem, network и `unsafe` не используются.

Storage E03 всё равно обязан ограничить bytes/depth/count до выделения входного
`Vec`: domain validation не заменяет parser resource limits.

## Проверка решения

- Acceptance tests для line→transition→arc и line→arc boundary classification.
- Property tests contiguous station ranges и matching boundary curvature.
- Regression tests zero/empty/constant/overflow/non-finite inputs и serde bypass.
- Exact Rust 1.88.0 workspace fmt/clippy/test/rustdoc/dependency/license gates.

## Ссылки

- `docs/PROJECT_SPEC.md` §7.1, §8.2
- `docs/ARCHITECTURE.md` §7.3, §17, §27 ADR-Q05
- `docs/IMPLEMENTATION_PLAN.md` E01-T04, E04-T01/T02, PR-005
