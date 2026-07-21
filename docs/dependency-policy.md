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

## Review desktop UI/GPU dependencies

Для E05-T01…T03 выбран последний совместимый с точным Rust 1.88 набор:

- `winit 0.30.13` — lifecycle нативного окна; отключены default features,
  Linux baseline использует X11/XWayland, а macOS/Windows platform backends
  выбираются target-specific кодом crate;
- `egui`, `egui-winit` и `egui-wgpu 0.33.3` — UI, input bridge и renderer bridge;
  более новые 0.35 требуют Rust 1.92 и нарушают текущий MSRV;
- `wgpu 27.0.1` приходит через `egui-wgpu` и соответствует Rust 1.88;
- `pollster 0.4.0` используется только для одноразовой инициализации GPU на
  main thread до начала frame loop, не как application async runtime.

Wayland feature намеренно не включена: совместимая ветка `wayland-scanner`
зависит от `quick-xml 0.39`, для которого опубликованы RUSTSEC-2026-0194/0195.
Linux smoke выполняется через Xvfb; native Wayland возвращается после обновления
MSRV/UI stack. Проектные XML или другие недоверенные данные UI graph не парсит.

Полный выбранный graph проходит license policy после явного разрешения
совместимых `CC0-1.0`, `ISC`, `Zlib`, `OFL-1.1` и `Ubuntu-font-1.0`.
Две unmaintained advisory закреплены узкими исключениями в `deny.toml`:
RUSTSEC-2024-0436 (`paste`, build-time Metal bindings) и RUSTSEC-2026-0192
(`ttf-parser`, только встроенные egui fonts). Пользовательские/project fonts не
загружаются. Оба исключения пересматриваются вместе с MSRV/UI stack; они не
скрывают vulnerability с доступным исправлением.

Источники review: [winit 0.30.13](https://docs.rs/crate/winit/0.30.13),
[wgpu 27.0.1](https://docs.rs/crate/wgpu/27.0.1),
[egui 0.33.3](https://docs.rs/crate/egui/0.33.3),
[RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436),
[RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192).

## Review CSN compiler dependencies

Для минимального E07 compiler slice добавлены две pure-Rust зависимости без
filesystem/network/process/clock доступа:

- `sha2 0.10.9` вычисляет канонический SHA-256 content hash CSN из явно
  упорядоченных bytes; алгоритм и порядок полей закреплены тестовым вектором;
- `libm 0.2.16` вычисляет `sin/cos` straight-reference pose одинаковой
  реализацией вместо platform libc, чтобы compiled coordinates и hash не зависели
  от системной math library.

Обе версии совместимы с Rust 1.88, имеют `MIT OR Apache-2.0`, не требуют native
build и проходят RustSec/license/source gates. `sha2` не используется для
секретов или authentication; content hash является идентификатором детерминированного
артефакта. Источники review: [sha2 0.10.9](https://docs.rs/crate/sha2/0.10.9),
[libm 0.2.16](https://docs.rs/crate/libm/0.2.16).

Для E09 backend trait добавлен `async-trait 0.1.89`. Он нужен только для
object-safe `SimulationBackend`/`SimulationSession` boundaries на Rust 1.88;
executor, I/O, threads, clock и network dependency crate не добавляет. Fake
backend выполняет готовые in-memory futures через caller и не вводит application
async runtime. Crate имеет `MIT OR Apache-2.0`, совместим с workspace MSRV и
проходит RustSec/license/source gates. Источник review:
[async-trait 0.1.89](https://docs.rs/crate/async-trait/0.1.89).

E09 worker control prototype не добавляет внешних зависимостей. Framing повторно
использует уже принятую `serde_json 1.0.150`, теперь как production dependency
только изолированного `roadsim-worker-protocol`; максимальный payload 1 MiB
проверяется до allocation. Process, pipe, bounded channel, timeout и child reap
реализованы стандартной библиотекой. Network, native code, async runtime и
filesystem workdir в этом срезе не добавлены. JSON предназначен только для
малого control plane; E09-T08 обязан отдельно рассмотреть зависимости и license
impact batch transport.

E09-T08 baseline также не добавляет dependency: отдельный inherited data pipe,
SoA DTO, latest-frame slot и reliable bounded queue используют `std` и тот же
bounded `serde_json`. Это измерительный baseline, а не предварительное принятие
JSON вместо Arrow IPC/shared memory; любая такая замена проходит новый MSRV,
license, unsafe/native и vulnerability review по ADR-Q09.

E09-T10 не добавляет новые third-party packages. `roadsim-worker-client`
напрямую использует уже закреплённые `serde`/`serde_json` для bounded journal DTO;
filesystem/process API остаются в этой boundary crate. Marker read проверяет
metadata size до allocation, записи используют `create_new` + `sync_all`, а
paths генерируются из numeric session ID без пользовательской строки.

E10-T01 не добавляет Cargo dependency и ещё не bundle-ит native artifact.
`supply-chain/sumo-engine.toml` закрепляет upstream SUMO `1.27.1` tag и полный
source commit, headless libsumo build и четыре NFR-030 target. CI fail-closed
отклоняет optional GPL extras, short/floating revision и изменение EPL-2.0
metadata; это технический guard, а не замена distribution review E10-T12.
Upstream evidence: [release](https://github.com/eclipse-sumo/sumo/releases/tag/v1_27_1),
[license inventory](https://eclipse.dev/sumo/docs/Libraries_Licenses.html).

E10-T02 делает уже присутствовавший в locked GPU graph `libloading 0.8.9`
прямой dependency только `sumo-worker`. Crate имеет MSRV 1.71 и лицензию ISC;
новой версии/transitive package в lockfile не появляется. Он нужен для загрузки
отдельно собранного exact native bridge без линковки libsumo в editor/CLI.
Unsafe изолирован одним `native` module: ABI version проверяется до вызовов,
library handle переживает function pointers, все C buffers bounded. Ни domain,
CSN, generic worker client, UI, ни renderer не получают native dependency.
Источник review: [libloading 0.8.9](https://docs.rs/crate/libloading/0.8.9).
