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
проверяется `cargo deny`/vulnerability tooling после добавления соответствующих
CI gates в `E00-T05` и инвентаризации в `E00-T10`.
