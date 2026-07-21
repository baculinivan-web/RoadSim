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

Worker protocol v3 сохраняет обязательную `EngineIdentity` с machine-safe
`name`, exact `version` и `build_revision`. Client может передать exact required
identity при handshake; несовпадение блокирует session стабильной diagnostic
`worker.engine.identity_mismatch`. Accepted response повторно сверяется client,
поэтому worker не может молча принять другой engine build.

`sumo-worker` загружает versioned native bridge только внутри отдельного process.
C++ bridge получает runtime version через libsumo API, сопоставляет её с
`eclipse.sumo/1.27.1`, публикует exact source commit как build revision и только
затем разрешает handshake. Version/build identity затем записывается в run
manifest E13-T02. Явные protocol commands `OpenSession`, `StepSession` и
`CloseSession` не смешиваются с health `Ping`.

Protocol v1/v2 schemas сохранены как исторические wire contracts, но runtime не
выполняет downgrade: текущая версия 3 обязательна для обеих control/data pipes.

Native build выполняется отдельно от Cargo:

```text
cmake -S workers/roadsim-sumo-worker/native -B <build-dir> \
  -DROADSIM_SUMO_HOME=<exact-source-build-root>
cmake --build <build-dir> --config Release
```

ABI fixture в workspace проверяет loader/lifecycle/crash isolation, но не
считается SUMO smoke. E10-T02 остаётся частичным, пока production bridge не
собран против exact headless artifact и не запущен на минимальном `.sumocfg`.

## License boundary

Upstream source указывает EPL-2.0 и отдельный inventory лицензий bundled/native
dependencies. Текущий срез лишь фиксирует metadata и запрещает optional GPL
extras. Он не является юридическим одобрением распространения. Notices, source
offer/availability, platform packages и полный dependency inventory должны быть
утверждены в E10-T12 после packaging spike E10-T11.
