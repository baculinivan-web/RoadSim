# Политика Rust toolchain и зависимостей

> Связанные решения: `E00-T02`, `ADR-001`, `NFR-046`, `NFR-050`, `NFR-051`.

## Toolchain и MSRV

- Единственный pin Rust находится в корневом `rust-toolchain.toml`.
- Текущий toolchain — Rust `1.88.0` с минимальным профилем и компонентами
  `rustfmt` и `clippy`.
- `workspace.package.rust-version = "1.88"` является MSRV для всех crates.
- Локальная разработка и CI должны использовать корневой pin, а не независимо
  выбранный `stable`.
- CI запускает Cargo с `--locked`; release artifacts собираются тем же pin.

Обновление toolchain выполняется отдельным PR:

1. одновременно изменить `rust-toolchain.toml` и `workspace.package.rust-version`;
2. объяснить необходимость обновления и влияние на поддерживаемые платформы;
3. осознанно обновить `Cargo.lock`, не смешивая unrelated dependency updates;
4. выполнить `fmt`, `clippy`, workspace tests и cross-platform CI;
5. зафиксировать несовместимости или изменение MSRV в release notes.

## Cargo.lock

- Корневой `Cargo.lock` коммитится, потому что workspace поставляет приложения.
- Отдельные lockfiles внутри crates запрещены.
- Обычные build/test jobs используют `--locked`; изменение lockfile должно быть
  видимым и объяснимым в PR.
- Массовый `cargo update` не совмещается с изменением продуктового поведения.

## Новые зависимости

Зависимость добавляется только после проверки:

- необходимость и отсутствие достаточно малого решения в уже принятом стеке;
- поддерживаемая версия, MSRV и активный upstream;
- минимально необходимые Cargo features и влияние на размер/время сборки;
- лицензия и совместимость с `Apache-2.0 OR MIT` для основного кода;
- известные уязвимости и supply-chain риск;
- native code, `unsafe`, filesystem, network, environment, clock и process access;
- влияние на determinism, сериализацию и архитектурные границы.

Версии общих зависимостей объявляются в `[workspace.dependencies]` и наследуются
crates через `workspace = true`. Wildcard requirements запрещены. Git-зависимости
требуют явной причины и immutable revision; опубликованный crate предпочтительнее.
Path dependencies допустимы только внутри workspace.

Новая native-зависимость, новая лицензия с дополнительными обязательствами,
расширение прав worker/plugin или telemetry требуют отдельного review. Политика
проверяется точным `cargo-deny 0.20.2`: bans/sources, licenses и RustSec advisories
являются отдельными CI steps. Конфигурация находится в `deny.toml`; ignore/skip и
расширение allow-list требуют причины и license/security review.

Архитектурные связи дополнительно проверяются по фактическому `cargo metadata`
скриптом `scripts/ci/check_dependency_graph.py` и policy
`supply-chain/dependency-policy.toml`. Native build/runtime components учитываются
в `supply-chain/native-dependencies.toml`, даже если Cargo не может их обнаружить.
Точный `cargo-cyclonedx 0.5.9` создает CycloneDX 1.5 документы; CI объединяет их с
Rust/native license inventory в скачиваемый artifact. Процедура и pins описаны в
`docs/ci.md`.

## Review зависимостей базовых domain types

Для `E01-T01`–`E01-T03` приняты следующие workspace dependencies:

- `serde 1.0.228` с единственной дополнительной feature `derive` — типизированная
  сериализация value/domain types; MSRV ниже workspace MSRV, native code и runtime
  I/O отсутствуют;
- `uuid 1.24.0` только с feature `serde` — хранение и проверка stable UUID; features
  генерации и RNG намеренно не включены, чтобы создание ID не получило скрытый
  nondeterministic source;
- `serde_json 1.0.150` используется только в tests текущего среза; публичная
  `.roadsim` schema этим не объявляется.

Все версии разрешаются через committed `Cargo.lock`; crates используют только
crates.io. Прямые зависимости имеют `MIT OR Apache-2.0`. Derive toolchain
транзитивно использует `unicode-ident 1.0.24` с выражением
`(MIT OR Apache-2.0) AND Unicode-3.0`: Unicode-3.0 разрешает использование и
распространение при сохранении copyright/permission notice в копиях либо
сопроводительной документации. Лицензия добавлена в явный allow-list и попадет в
license inventory; исключение или `cargo-deny` skip не добавлялись.
Для распространяемого binary/release bundle полный copyright/permission notice
должен войти в third-party notices по `E16-T09`; текущий M0 inventory фиксирует
обязательство, но не заменяет release notices.

Проверены upstream metadata, MSRV/features, полный Cargo graph и RustSec advisory
gate. Эти зависимости не добавляют filesystem, network, process, clock или native
runtime boundary. Семантическая проверка соответствия authority/WKT фактическим
единицам будет выполняться сервисом PROJ в `E14-T06`; текущий domain contract
обязывает явно указать метрическую declared engineering unit и отклоняет
degree/foot. Domain text limits ограничивают состояние модели, но pre-allocation
byte/depth limits недоверенного документа остаются обязанностью `E03` storage
boundary.

Для property-based acceptance E01-T04 принят `proptest 1.11.0` только как
`dev-dependency` `roadsim-domain`. Default features отключены; включена только
feature `std`, поэтому fork/timeout/tempfile test-process capabilities не входят в
выбранный graph. Upstream указывает MSRV 1.85 и лицензию `MIT OR Apache-2.0`;
production crates не получают runtime dependency, RNG или I/O через этот test
framework. Генератор используется только для ограниченных finite inputs, а его
случайный seed не участвует в model behavior или serialized data. Версия
зафиксирована общим workspace requirement и `Cargo.lock`; `cargo deny` проверяет
полный transitive graph.

Источники review: [Serde](https://docs.rs/crate/serde/1.0.228),
[UUID](https://docs.rs/crate/uuid/1.24.0),
[unicode-ident metadata](https://docs.rs/crate/unicode-ident/1.0.24/source/Cargo.toml),
[Unicode License v3](https://spdx.org/licenses/Unicode-3.0.html).
[Proptest](https://docs.rs/crate/proptest/1.11.0).
