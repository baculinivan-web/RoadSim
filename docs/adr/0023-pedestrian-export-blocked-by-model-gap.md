# ADR-023: Pedestrian SUMO export blocked by a missing Design Model relation

- Статус: accepted
- Дата: 2026-07-26
- Владельцы: domain maintainers, backend/SUMO maintainers
- Reviewers: architecture, domain
- Связанные требования: FR-031, UC-03, NFR-020
- Связанные задачи: E10-T07, E01-T06, E07-T04, E12-T04

## Контекст

E10-T07 требует экспортировать пешеходный спрос и переходы в SUMO. CSN уже
содержит pedestrian graph (E07-T04): узлы — walking areas и sidewalks, рёбра —
crossings. Demand (E01-T08, FR-031) задаётся между walking areas.

SUMO выражает пешехода как `<personFlow>` со стадиями `walk`, где endpoint —
**edge** (либо junction/TAZ), а пешеходная инфраструктура — это lane с
`allow="pedestrian"` плюс `<crossing>` у узла.

Чтобы перевести authored поток «walking area A → walking area B» в SUMO, нужно
знать, на какую пешеходную кромку (sidewalk) опирается каждая walking area.

## Обнаруженное противоречие

В Design Model такой связи нет:

- `WalkingArea` — только `id` и polygon boundary;
- `Sidewalk` — `corridor_id`, side, station range, width;
- `Crossing` — corridor, station, width и **две walking areas**.

Sidewalk и WalkingArea не связаны ни одним typed reference. Никакой набор
существующих полей не определяет, какой sidewalk достижим из walking area.

Любое восстановление этой связи по геометрии (ближайшая кромка, попадание
точки в полигон, совпадение станции) — это догадка о намерении пользователя.
AGENTS.md §2 и §4.9 запрещают её: результат менял бы маршруты пешеходов и, как
следствие, метрики уступания UC-03, при этом выглядя как поддержанная функция.

## Рассмотренные варианты

### Сопоставлять walking area и sidewalk по геометрии в компиляторе

Не требует изменения модели, но вводит скрытую эвристику в источник маршрутов.
Отклонено.

### Экспортировать пешеходов через `fromJunction`/`toJunction`

SUMO может маршрутизировать персон между узлами, но узел RoadSim — это
junction дорожной сети, а walking area не привязана к junction ни одним
reference. Проблема та же, просто перенесённая на другой тип.

### Зафиксировать пробел и заблокировать задачу

E10-T07 не может быть корректно выполнена до расширения модели. Задача
блокируется, а экспорт продолжает явно отклонять пешеходную сеть.

## Решение

Принят третий вариант.

1. E10-T07 помечается заблокированной задачей до появления typed связи
   «walking area ↔ pedestrian edge» в Design Model (follow-up к E01-T06).
   Минимальный требуемый контракт: walking area должна ссылаться на набор
   sidewalk/edge endpoints, через которые в неё входят и выходят пешеходы.
2. `roadsim-backend-sumo` продолжает отклонять непустой pedestrian graph кодом
   `backend.sumo.pedestrian_network.unsupported`, а нецелевой demand mode —
   `compiler.demand.mode_unsupported`. Молчаливого удаления пешеходов нет.
3. Milestone M4 закрывается по автомобильной части; пешеходная часть его exit
   gate остаётся открытой и явно связана с этим ADR, а не с «недоделанным
   backend».
4. Решение пересматривается вместе с domain owner при работе над E12-T04
   (pedestrian zones/flows editor), где эта связь всё равно потребуется в UI.

## Последствия

Положительные: маршруты пешеходов не будут определяться скрытой эвристикой
adapter; пробел зафиксирован до того, как на него начали опираться golden
метрики UC-03.

Отрицательные: M4 не закрывается целиком без изменения модели, а E10-T07,
E12-T04 и UC-03 сдвигаются за это изменение.

## Проверка

- `crates/roadsim-backend-sumo/tests/network_export.rs::pedestrian_network_fails_before_t07_instead_of_being_dropped`
- `crates/roadsim-compiler/tests/graphs.rs::pedestrian_demand_is_not_silently_compiled_as_vehicle_demand`
