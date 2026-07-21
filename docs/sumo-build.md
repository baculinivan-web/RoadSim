# SUMO engine pin and build boundary

> Связанные требования: E10-T01, FR-040, NFR-030, NFR-043; ADR-Q06 остаётся
> открытым до packaging и distribution review E10-T11/T12.

## Exact pin

Первый production-backend target закреплён на Eclipse SUMO `1.27.1`, source tag
`v1_27_1` и commit `7717f2379d9e314a0c81c5cec748444de06a2a91`.
Machine-readable source of truth — `supply-chain/sumo-engine.toml`; CI отклоняет
short commit, floating tag/version, неполную platform matrix и optional GPL
extras. Обновление любой части pin выполняется отдельным reviewed change вместе
с golden/contract evidence, а не автоматически.

Официальные upstream metadata:

- [release v1_27_1](https://github.com/eclipse-sumo/sumo/releases/tag/v1_27_1);
- [downloads 1.27.1](https://eclipse.dev/sumo/docs/Downloads.html);
- [SUMO version semantics](https://eclipse.dev/sumo/docs/Versioning.html);
- [license и dependency inventory](https://eclipse.dev/sumo/docs/Libraries_Licenses.html).

## Build strategy

Baseline строит headless libsumo из exact source commit в отдельном worker
artifact. `sumo-gui`, optional GPL extras и nightly packages не входят в build.
Homebrew не является источником production artifact: upstream больше не
поддерживает этот installation path. В репозитории пока не публикуются и не
bundle-ятся SUMO binaries; reproducible builder и checksums относятся к
E10-T11.

| Target | Minimum | E10-T01 strategy |
|---|---|---|
| Windows x64 | Windows 11 | exact source commit, Release, headless libsumo |
| macOS arm64 | macOS 14 | exact source commit, Release, headless libsumo |
| macOS x64 | macOS 14 | exact source commit, Release, headless libsumo |
| Linux x64 | Ubuntu 24.04 LTS | exact source commit, Release, headless libsumo |

Эта таблица фиксирует target contract, но не заявляет успешный clean-machine
package smoke: такое evidence требуется в E10-T11.

## Runtime identity contract

Worker protocol v2 добавляет обязательную `EngineIdentity` с machine-safe
`name`, exact `version` и `build_revision`. Client может передать exact required
identity при handshake; несовпадение блокирует session стабильной diagnostic
`worker.engine.identity_mismatch`. Accepted response повторно сверяется client,
поэтому worker не может молча принять другой engine build.

E10-T02 обязан получить runtime version через libsumo API, сопоставить её с
`eclipse.sumo/1.27.1`, встроить exact source commit как build revision и только
затем принять `OpenSession`. Version/build identity затем записывается в run
manifest E13-T02.

Protocol v1 schemas сохранены как исторический wire contract, но runtime не
выполняет downgrade: текущая версия 2 обязательна для обеих control/data pipes.

## License boundary

Upstream source указывает EPL-2.0 и отдельный inventory лицензий bundled/native
dependencies. Текущий срез лишь фиксирует metadata и запрещает optional GPL
extras. Он не является юридическим одобрением распространения. Notices, source
offer/availability, platform packages и полный dependency inventory должны быть
утверждены в E10-T12 после packaging spike E10-T11.
