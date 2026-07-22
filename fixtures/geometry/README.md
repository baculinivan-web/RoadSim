# Geometry fixtures

Этот каталог предназначен для аналитических и legally shareable geometry fixtures
RoadSim. Все координаты и длины задаются в локальных метрах, углы — в радианах,
curvature — в `1/m`.

Для E01-T04 внешние serialized fixtures намеренно не добавлены: `.roadsim` v1 и
canonical model schema появятся только в E03. Boundary cases находятся в
`crates/roadsim-domain/tests/reference_line.rs` и генерируются из аналитических
line/arc/linear-curvature inputs; сторонние данные и лицензируемые материалы не
используются.

E01-T05 аналогично использует аналитический двухсторонний corridor и generated
monotonic station profiles в `crates/roadsim-domain/tests/corridor.rs`. Постоянные
widths заданы в метрах; serialized fixture появится только вместе с versioned
project schema E03.

E04-T01…T04 используют legally shareable analytic fixtures в
`crates/roadsim-geometry/tests/kernel.rs`: прямая, четверть окружности, линейное
изменение кривизны, signed offset, piecewise lane profile, crossing/touch/overlap
segments, zero-length segment и derived overflow. Ожидаемые координаты выводятся
из аналитических формул; transition position проверяется на finite bounded
evaluation, а heading/curvature — по точным формулам. Все units локальные метры,
радианы и `1/m`; внешнего источника и отдельной лицензии нет.

При добавлении fixture рядом должен быть указан источник, лицензия, units,
ожидаемый результат и процедура осознанного обновления. Golden output нельзя
перезаписывать обычным test run.
