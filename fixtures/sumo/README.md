# SUMO fixtures

## Происхождение и лицензия

`minimal/` создан специально для RoadSim и распространяется на условиях
репозитория (`Apache-2.0 OR MIT`). Fixture не копирует upstream SUMO examples и
не содержит сторонних assets.

## Единицы и ожидаемый результат

- координаты и размеры — метры;
- скорость — м/с;
- simulation step задаётся worker session в миллисекундах;
- `netconvert` exact SUMO 1.27.1 создаёт `minimal.net.xml`;
- автомобиль `rs_agent_0` имеет footprint 4,5 × 1,8 м и после пяти шагов
  присутствует в одном bounded visual SoA batch.

Префикс `rs_agent_` является детерминированным backend ID namespace; суффикс —
compact `u32`, который adapter связывает с source map demand при реализации
E10-T06.

## Обновление

Изменение fixture требует запуска ignored exact-engine tests из
`docs/sumo-build.md`, объяснимого diff ожидаемого state и обновления changelog.
Fixture не обновляется только ради зелёного теста.
