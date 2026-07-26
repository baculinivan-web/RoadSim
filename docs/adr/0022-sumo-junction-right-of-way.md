# ADR-022: Explicit SUMO connection и TLS tables, delegated right-of-way

- Статус: accepted
- Дата: 2026-07-26
- Владельцы: backend/SUMO maintainers
- Reviewers: architecture, domain, determinism
- Связанные требования: FR-040…FR-043, NFR-020, NFR-030
- Связанные задачи: E10-T03, E10-T04, E10-T05, E07-T05, E07-T06, E07-T07

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
   pedestrian graph (E10-T07) и stop positions — отклоняются с Design object
   references, а не удаляются из экспорта.

## Решение для сигнального управления (E10-T05)

7. Контроллер экспортируется как один `<tlLogic type="static">`; узел получает
   `type="traffic_light" tl="rs_tls_<n>"`, а связи — `tl`/`linkIndex`.
   `linkIndex` следует compact movement order узла, поэтому state string
   детерминирована для одной CSN.
8. Indication отображается один к одному: `Green → G`, `Amber → y`,
   `RedAmber → u`, `Red → r`, `Dark → O`. Green всегда major (`G`), потому что
   compiler уже блокирует одновременно зелёные геометрически конфликтующие
   movements; minor green выдумывать не требуется.
9. Movement сигнализированного узла, не принадлежащий ни одной группе,
   отклоняется кодом `backend.sumo.signal_movement.unbound`: пропуск ссылки в
   state string молча дал бы ему green.
10. Authored `intergreen` пока не экспортируется. Распределение clearance между
    amber и all-red — нормативная трактовка, которую adapter не имеет права
    выдумывать, поэтому программа с `intergreen > 0` отклоняется кодом
    `backend.sumo.signal_intergreen.unsupported` до появления подтверждённой
    трактовки domain owner (E08/E12-T09). Программы с нулевым intergreen
    экспортируются полностью.
11. Stop positions остаются неподдержанными
    (`backend.sumo.stop_positions.unsupported`): позиция ожидания внутри SUMO
    junction — отдельное решение отображения, а не часть TLS export.

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
- Отдельный opt-in smoke проверяет, что двухфазная fixed-time программа
  доходит до `roadsim.net.xml` как `<tlLogic id="rs_tls_0">` с исходными
  state strings.
