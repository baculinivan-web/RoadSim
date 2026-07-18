# ADR-017: Canonical project JSON and extensions

- Статус: proposed
- Дата: 2026-07-18
- Владельцы: storage/domain maintainers
- Reviewers: architecture, domain, security, license
- Связанные требования: FR-001, FR-002, NFR-032, NFR-041, NFR-045, NFR-052
- Связанные задачи: E01-T10, E03-T01, E03-T02, ADR-Q04

## Контекст

`.roadsim` v1 должен получить публичные JSON schema, повторяемые entry hashes и
semantic project hash до ZIP reader/writer. Обычный JSON допускает произвольный
порядок object members и разные представления чисел, поэтому bytes обычного
`serde_json` не являются межъязыковым hash contract. Одновременно silent discard
неизвестных полей запрещён, а сохранение любого неизвестного core field сделало бы
семантику проекта неоднозначной.

Решение является публичным file-format contract и не может остаться локальной
деталью serializer. Rust struct layout и domain serde derives не объявляются
форматом хранения.

## Драйверы решения

- Одинаковый semantic content должен давать одинаковые bytes и SHA-256 на всех
  поддерживаемых платформах.
- UUID, единицы и версии должны иметь одно locale-independent представление.
- Недоверенный JSON обязан fail closed на duplicate keys, unsafe numbers,
  неизвестные core fields и превышение resource limits.
- Extensions должны round-trip без права незаметно менять core semantics.
- Canonicalization не должна получать filesystem, network, clock или RNG.

## Рассмотренные варианты

### Вариант A: Compact `serde_json` и фиксированный порядок Rust fields

Малый dependency footprint, но порядок зависит от конкретных wire structs и map
implementation. Межъязыковая реализация, изменение serializer и object-valued
extension могут дать другие bytes при той же JSON semantics.

### Вариант B: RFC 8785 JSON Canonicalization Scheme

JCS задаёт UTF-8, отсутствие whitespace, ECMAScript-compatible numbers и
рекурсивную сортировку keys по UTF-16 code units. Формат стандартизован и имеет
межъязыковые vectors, но ограничен I-JSON и безопасным диапазоном integer binary64.

### Вариант C: Собственный canonical binary format

Можно точно представить все Rust значения, но это ухудшает открытость authoring
format, требует отдельной спецификации/tooling и расходится с выбранным JSON v1.

## Решение

Предлагается вариант B: canonical profile `roadsim-jcs-rfc8785-v1` соответствует
RFC 8785 с учётом verified errata и дополнительных ограничений RoadSim.
Canonical JSON строится только из отдельного versioned storage DTO и не
сериализует layout domain structs напрямую.

- Semantic project hash: `SHA-256("roadsim:model:v1\0" ||
  canonical_semantic_projection)`; отображение — lowercase `sha256:<64 hex>`.
  Проекция содержит core model fields, sorted `required_features` и payloads
  required extensions. Manifest обязан повторять тот же exact feature set;
  mismatch блокирует open, поэтому удаление feature ID не сохраняет identity.
- Optional/non-semantic extension payloads и manifest provenance/timestamps/
  compression исключены из semantic projection. Они остаются в canonical model
  entry и защищены entry hash, но не меняют инженерную semantic identity.
- Entry hash в manifest: SHA-256 точных uncompressed entry bytes без domain
  prefix. Manifest не входит в собственный entry list.
- Дополнительный профиль RoadSim ограничивает integer JSON numbers диапазоном
  `[-(2^53-1), 2^53-1]`, хотя RFC 8785 допускает и другие binary64-representable
  числа. Более широкие counters/ticks в будущих schema кодируются явно как
  canonical decimal strings, а не округляются. Ограничение рекурсивно действует
  и для extension JSON.
- Floating-point values обязаны быть finite. Domain value constructors уже
  нормализуют signed zero; strict reader дополнительно отклоняет любое number
  spelling, которое вычисляется в IEEE negative zero (`-0`, `-0.0`, `-0e0` и
  эквивалентные формы), по security guidance verified errata RFC 8785.
- Arrays сохраняют semantic order. Unordered domain collections сортируются по
  stable ID до wire conversion; JCS сортирует только object keys.
- Core objects используют `additionalProperties: false`. Расширения разделены на
  явные maps `extensions.required` и `extensions.optional`; key — lowercase
  reverse-DNS name, value — JCS-safe JSON. Reader сохраняет неизвестные values как
  JSON semantics с последующей JCS rewrite, а не исходные whitespace/key-order
  bytes.
- Extension не может переопределять core field. Если без extension нельзя
  корректно интерпретировать проект, его ID одновременно присутствует в
  sorted/deduplicated `required_features` model/manifest и unsupported reader
  блокирует open до замены active project.
- Provenance timestamps находятся в manifest, задаются caller и исключены из
  semantic model hash. Serializer не читает wall clock самостоятельно.

## Последствия

### Положительные

- Hash contract воспроизводим независимо от JSON map insertion order и locale.
- Wire schema может мигрировать независимо от внутреннего Rust refactoring.
- Unknown core semantics не принимаются молча; namespaced data сохраняются явно.
- SHA-256 и canonicalization не связывают storage с UI, filesystem или backend.

### Отрицательные и компромиссы

- Нужны reviewed dependencies для JCS и SHA-256 и отдельные conformance vectors.
- `u64` нельзя безусловно писать JSON number; будущие DTO обязаны выбрать safe
  number или decimal string на уровне schema.
- Generic `serde_json::Value` parser сам по себе не обнаруживает duplicate keys и
  не ограничивает depth/bytes. Strict bounded parser остаётся обязательным в
  E03-T03 и предшествует deserialization недоверенного container entry.

## Совместимость и миграция

Это первый опубликованный project JSON contract; migration существующих
`.roadsim` artifacts отсутствует. Любое несовместимое изменение canonical profile,
core schema или hash preimage создаёт новый schema major и migration/report.
Additive extension не меняет core schema только при соблюдении namespace и
required-feature policy.

ADR остаётся `proposed` до полного inventory semantic fields E01-T06…T09,
фиксации schema/examples и conformance vectors, dependency/license/security
review и доказательства матрицы hash inclusion. До выполнения этих blockers
`.roadsim` v1 и accepted E01-T10/E03-T01/T02 на это решение не ссылаются.

## Безопасность и воспроизводимость

Canonicalization выполняется после byte/depth/count/duplicate-key validation для
недоверенного ввода. До E03-T03 API canonical serializer принимает только уже
проверенные in-memory DTO. Schema устанавливает лимиты строк, массивов и safe
integers; container limits остаются отдельным более ранним барьером.

Hash обеспечивает целостность, но не подлинность. Optional signatures требуют
отдельного key/trust policy. Secrets запрещены в model, manifest и extensions.
Wall clock, random hash order, filesystem paths и machine-local data не входят в
semantic hash.

## Проверка решения

- Официальные RFC 8785 number/key-order vectors и regression для unsafe integer.
- Одинаковый model DTO с разным insertion order extensions даёт одинаковые bytes.
- JSON Schema Draft 2020-12 meta-validation и positive/negative example corpus.
- Model encode/decode сохраняет current semantic Project и stable UUID.
- Exact Rust 1.88.0 fmt/clippy/test/rustdoc/dependency/license/advisory gates.

## Ссылки

- `docs/PROJECT_SPEC.md` FR-001/FR-002, NFR-032/NFR-041/NFR-045/NFR-052
- `docs/ARCHITECTURE.md` §6.2, §13, §20, §27 ADR-Q04
- `docs/IMPLEMENTATION_PLAN.md` E01-T10, E03-T01/T02, PR-009
- [RFC 8785: JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785)
- [RFC 8785 verified errata](https://www.rfc-editor.org/errata/rfc8785)
