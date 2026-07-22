# Dependency graph fixtures

Эти синтетические Cargo metadata fixtures принадлежат проекту RoadSim и
лицензируются как `Apache-2.0 OR MIT`. Они не содержат внешних данных или единиц
измерения.

- `allowed-domain-types.json` показывает допустимую связь domain → core types;
- `forbidden-domain-ui.json` намеренно нарушает `ARCHITECTURE.md §4` транзитивной
  связью domain → helper → editor UI;
- `forbidden-geometry-gpu.json` намеренно нарушает ADR-021 прямой связью
  derived geometry → `wgpu`. Оба запрещённых fixture обязаны завершать graph
  guard кодом отказа.

Fixtures обновляются вместе с `supply-chain/dependency-policy.toml`. Запрещенный
fixture нельзя превращать в разрешенный только для получения зеленого CI: каждое
изменение архитектурной границы требует source-of-truth/ADR review.
