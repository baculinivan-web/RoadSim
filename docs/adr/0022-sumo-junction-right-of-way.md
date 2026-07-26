# ADR-022: Explicit SUMO connection table and delegated right-of-way

- Статус: accepted
- Дата: 2026-07-26
- Владельцы: backend/SUMO maintainers
- Reviewers: architecture, domain, determinism
- Связанные требования: FR-040…FR-043, NFR-020, NFR-030
- Связанные задачи: E10-T03, E10-T04, E10-T05, E07-T05, E07-T06

## Контекст

E07-T05/T06 публикуют в CSN детерминированную таблицу semantic movements,
exact cubic connector curves и sparse conflict matrix. E10-T03 экспортирует
только прямые lanes и явно отклоняет CSN с movements. Для M4 нужен экспорт
перекрёстка, при этом Design Model пока не содержит ни junction priority, ни
правил приоритета: нормативная трактовка приоритета — предмет epic E08 и
domain owner, а не backend adapter.

`netconvert` по умолчанию сам достраивает connections и turnarounds по
эвристике на основе геометрии узла. Для CSN это означало бы, что backend может
добавить turn path, которого нет в Design Model, либо потерять авторский.

## Рассмотренные варианты

### Полагаться на эвристику `netconvert`

Минимум кода, но набор turn paths определяется SUMO, а не CSN. Это нарушает
инвариант «SUMO — производный формат» и делает CSN movements декоративными.

### Вывести приоритет из CSN conflict matrix

Conflict matrix — геометрический факт (пересечение траекторий), а не
нормативное правило уступания. Преобразование «есть конфликт → кто уступает»
требует трактовки ПДД/ГОСТ и запрещено агенту без domain owner.

### Полный explicit connection table и делегированный right-of-way

Экспорт полностью описывает связи узла и отключает эвристику, но не выдумывает
числовой приоритет: `netconvert` рассчитывает право проезда между уже
зафиксированными связями по их геометрии.

## Решение

Принят третий вариант.

1. Каждое compiled movement экспортируется ровно как один
   `<connection from to fromLane toLane/>`; ID связей возвращаются в
   `SumoConnectionMapping` вместе с `CompiledMovementId` и `JunctionId`.
2. Bundle обязан использоваться с `SUMO_NETCONVERT_INPUT_ARGUMENTS`, где
   передаётся connections file и `--no-turnarounds true`, поэтому SUMO не
   добавляет связи поверх таблицы.
3. Узел, у которого есть movements, экспортируется как `type="priority"`; все
   edges получают одинаковый `priority="1"`. RoadSim не назначает приоритет,
   пока он не появится в Design Model, и не кодирует нормативные правила в
   backend.
4. Право проезда между экспортированными связями рассчитывает `netconvert` из
   геометрии. Это зафиксированное, а не молчаливое делегирование: оно
   детерминировано для одной pinned версии SUMO `1.27.1`.
5. Неполный набор movements у узла с реальными destination edges — ошибка
   `backend.sumo.junction_movements.incomplete`, а не повод вернуться к
   эвристике. Turnaround исключён из проверки, потому что он отключён явно.
6. Сущности, которые CSN уже выражает, а этот этап ещё не отображает —
   pedestrian graph (E10-T07), stop positions и signal programs (E10-T05) —
   отклоняются с Design object references, а не удаляются из экспорта.

## Последствия

Положительные: набор turn paths полностью принадлежит Design Model; mapping
`movement → connection` losless и пригоден для diagnostics и frame adapter;
отсутствие авторского приоритета видно в контракте, а не спрятано в XML.

Отрицательные: до появления authored priority и E08 rule pack приоритет на
перекрёстке определяется геометрией и версией SUMO, поэтому смена pinned
версии требует пересмотра golden fixtures. Многополосные approach lanes и
lane-to-lane выбор внутри одного corridor остаются за E07/E10 следующих
итераций.

## Проверка

- `crates/roadsim-backend-sumo/tests/network_export.rs`: детерминированный
  экспорт четырёхстороннего узла с 12 связями, отклонение неполного набора
  movements, разорванных endpoints, pedestrian graph и traffic controls.
- Opt-in smoke на exact `netconvert 1.27.1` проверяет, что связи сохранены в
  `roadsim.net.xml` и что узел получил вычисленные `<request>` строки.
