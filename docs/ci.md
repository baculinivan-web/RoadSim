# CI, dependency gates и supply-chain artifacts

> Work packets: `E00-T05`, `E00-T06`, `E00-T09`, `E00-T10`.
> Требования и решения: `NFR-030`, `NFR-046`, `NFR-050`, `NFR-051`,
> `ADR-001`, `ADR-015`, `ARCHITECTURE.md §4, §20, §23`.

## Обязательные PR gates

Workflow `.github/workflows/ci.yml` запускается для pull request, push в `main` и
вручную. Он имеет только `contents: read`, отключает сохранение checkout credentials
и состоит из независимо наблюдаемых gates:

1. **Rust quality and docs** — точный toolchain, `rustfmt`, Clippy с
   `-D warnings`, workspace tests, rustdoc с `-D warnings` и локальные Markdown
   links;
2. **Forbidden dependency graph** — фактический `cargo metadata` graph и
   regression fixtures проверяются против `supply-chain/dependency-policy.toml`;
3. **Build and smoke** — одна matrix собирает `roadsim-app`/`roadsim-cli`, затем
   desktop app создаёт окно, GPU surface и два UI frame на Ubuntu 24.04 под
   Xvfb, Windows и macOS 14; smoke auto-starts deterministic fake backend и
   завершается успешно только после первого 18-agent simulation batch;
4. **Dependencies, licenses, security, and SBOM** — `cargo-deny` отдельно
   проверяет bans/sources, лицензии и RustSec advisories, после чего публикуются
   CycloneDX и license/native inventory.

Branch protection на GitHub должен требовать все четыре job family. Репозиторий
не может сам включить branch protection без hosting/admin configuration; до ее
настройки workflow все равно возвращает неуспешный run при нарушении gate.

## Зафиксированные версии

| Компонент | Pin | Назначение |
|---|---|---|
| Rust | `1.88.0` | единый local/CI toolchain из `rust-toolchain.toml` |
| `actions/checkout` | commit `de0fac2…` (`v6.0.2`) | immutable source checkout |
| `actions/upload-artifact` | commit `bbbca2d…` (`v7.0.0`) | immutable SBOM/inventory upload |
| `cargo-deny` | `0.20.2` | dependency, source, license и advisory policy |
| `cargo-cyclonedx` | `0.5.9` | CycloneDX 1.5 SBOM generation |

Action pins используют полные commit SHA: GitHub указывает, что это единственный
immutable способ подключить action. Комментарий с release tag оставлен для
Dependabot/review, но исполняется именно SHA. Cargo tools устанавливаются с
`--locked --version`, и их MSRV не выше RoadSim Rust 1.88.0. Обновление любого pin
выполняется отдельным dependency review с проверкой upstream release, полного SHA,
MSRV, лицензии и changelog.

Первичные источники:

- [GitHub: secure use — full-length SHA](https://docs.github.com/en/actions/reference/security/secure-use);
- [GitHub: matrix jobs](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/run-job-variations);
- [`actions/checkout` releases](https://github.com/actions/checkout/releases);
- [`actions/upload-artifact` releases](https://github.com/actions/upload-artifact/releases);
- [`cargo-deny` documentation](https://embarkstudios.github.io/cargo-deny/);
- [`cargo-cyclonedx` upstream](https://github.com/CycloneDX/cyclonedx-rust-cargo).

## Dependency graph policy

Guard работает по разрешенному Cargo graph, а не по именам директорий или
ручному review, и проверяет также транзитивную достижимость запрещенного package.
Нарушение получает стабильный код
`ARCH-FORBIDDEN-DEPENDENCY`, rule ID, source/target crates и rationale.

`fixtures/dependency-graph/forbidden-domain-ui.json` намеренно моделирует
`roadsim-domain → roadsim-editor-ui`. CI считается корректным только если этот
fixture отвергнут. Допустимый fixture защищает от реализации guard, которая
отвергает любой edge.

Локальный запуск:

```text
cargo metadata --format-version 1 --locked --all-features > target/cargo-metadata.json
python3 scripts/ci/check_dependency_graph.py \
  --metadata target/cargo-metadata.json \
  --policy supply-chain/dependency-policy.toml
python3 scripts/ci/test_dependency_graph.py -v
python3 scripts/ci/check_sumo_pin.py
python3 scripts/ci/test_sumo_pin.py -v
```

Реальный libsumo lifecycle не запускается в обычном Rust gate без native
artifact. Его opt-in команда и обязательные `ROADSIM_SUMO_BRIDGE` /
`ROADSIM_NETCONVERT` описаны в [SUMO build boundary](sumo-build.md). Test не
подменяется ABI fixture и остаётся `ignored`, пока E10-T11 не предоставит
проверяемые platform packages для CI matrix.

## SBOM и license inventory

Supply-chain artifact содержит:

- отдельный CycloneDX 1.5 JSON для каждого workspace package со всеми Cargo
  dependencies и target-specific graph;
- `license-inventory.json` со всеми Rust packages из `cargo metadata`;
- тот же inventory содержит явный список native/build/runtime components из
  `supply-chain/native-dependencies.toml`.

Generator заменяет абсолютный checkout path в workspace CycloneDX references на
`/roadsim-workspace`, чтобы Actions artifact не раскрывал путь runner/user и был
сопоставим между checkout locations.

На M0 native list пуст, потому что workspace еще не связывает SUMO, GDAL, PROJ
или иные native components. Пустой список является явным проверяемым состоянием,
а не заявлением о будущей дистрибуции. Добавление native component требует
version/source/license/platform/scope/bundling/review metadata и отдельного
license/security review. Базовый M0 artifact не заменяет release notices и
distribution review `E10-T12`/`E16-T09`.
