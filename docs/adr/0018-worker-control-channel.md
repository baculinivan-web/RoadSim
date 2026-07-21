# ADR-018: Worker control framing and child transport

- Статус: proposed
- Дата: 2026-07-19
- Владельцы: backend/architecture maintainers
- Reviewers: architecture, security, Windows platform
- Связанные требования: NFR-002, NFR-005, NFR-013, NFR-014, NFR-043
- Связанные задачи: E09-T04…T08, ADR-Q02, ADR-Q03, ADR-Q09

## Контекст

До подключения libsumo необходим проверяемый lifecycle внешнего worker: version
handshake, capability preflight, correlation, cancellation и изоляция crash/hang.
Транспорт должен работать без сетевого listener и shell interpolation, а
недоверенный output worker не должен инициировать неограниченное выделение памяти.

Высокочастотные agent frames и недропаемые metric batches имеют иные требования
к throughput/backpressure. Их преждевременное включение в control protocol скрыло
бы выбор ADR-Q09 без измерений.

## Рассмотренные варианты

### Вариант A: Protobuf поверх Unix domain socket/named pipe

Компактный schema-first wire format и явный local endpoint, но сразу добавляет
code generation/dependency, две platform implementations и lifecycle stale
endpoint до появления реального SUMO payload.

### Вариант B: bounded JSON frames поверх inherited stdin/stdout

Одинаковая process API на macOS/Linux/Windows, endpoint нельзя повторно открыть
посторонним процессом, schema можно review отдельно. Недостатки — текстовый
формат, отсутствие multiplexing и непригодность для больших state batches.

### Вариант C: loopback TCP

Переносим и хорошо поддерживается tooling, но создаёт listener/race с выбором
порта и расширяет local attack surface без необходимости.

## Предлагаемое решение

Для PR-020 предлагается вариант B как обратимый control-plane contract:

- parent запускает точный executable с массивом аргументов и inherited pipes;
- frame — `u32` little-endian byte length и UTF-8 JSON payload;
- hard limit control frame — 1 MiB, проверяемый до payload allocation;
- JSON shape опубликован как Draft 2020-12. Исторический v1 сохранён, текущий
  `schemas/worker-protocol/control-v2.schema.json` добавляет exact engine identity;
- каждый envelope содержит `protocol_version`, `request_id`, optional
  `session_id` и монотонный `sequence`;
- первый request передаёт 256-bit lowercase hex one-time token и required
  capabilities; token доступен только child environment и redacted в
  `Debug`/`Display`;
- неизвестная capability и version mismatch дают стабильный diagnostic до
  открытия session, без silent downgrade;
- v2 handshake передаёт обязательные worker version и engine
  name/version/build revision; optional exact requirement блокирует иной build
  до открытия session;
- одна mutable client instance разрешает один in-flight control request;
- timeout завершает worker, crash/EOF не распространяется на editor process;
- stdin/stdout зарезервированы протоколом, stderr не попадает в пользовательский
  UI автоматически.

JSON является wire format только control-plane. Он не сериализует Rust layout
и не переносит CSN, per-agent frames, metrics или result artifacts. Для этих
данных E09-T08/ADR-Q09 должен выбрать bounded batch transport и две очереди с
разной loss policy.

## Последствия

### Положительные

- Один код spawn/framing тестируется на всех поддерживаемых OS.
- Нет listener, stale endpoint, port allocation и shell command surface.
- Размер, версия, ordering и correlation являются наблюдаемыми contracts.
- Worker можно принудительно остановить, не меняя Design Model.

### Ограничения

- Текущий spike проверен runtime на macOS; Windows/Linux evidence должен прийти
  из существующей CI matrix до принятия ADR.
- Environment не является secret vault: token защищает accidental cross-talk и
  stale/reused channel, но не процесс с правами чтения environment/debugger того
  же пользователя.
- Blocking pipe reader изолирован отдельным thread; production scheduler и
  bounded multi-worker policy относятся к E09-T10/E16.
- Heartbeat пока представлен request timeout/ping, а не периодической health
  policy application runtime.

## Безопасность и отказоустойчивость

Length проверяется до allocation, malformed JSON закрывает channel, capability
IDs имеют count/length/character limits. Child получает один secret и не получает
network/listener. Client не исполняет строки worker как команды. Timeout и Drop
закрывают pipe, завершают и reap-ят child. Authentication failure завершает stub
worker и не отражает token в response.

## Проверка решения

- frame round-trip, empty/truncated/invalid/oversized input;
- protocol version/engine identity mismatch и неизвестная capability;
- неверный token и handshake-before-session;
- open/cancel/shutdown lifecycle;
- forced worker crash и hang timeout;
- exact Rust 1.88 fmt/clippy/test/rustdoc/dependency/license/advisory gates;
- Windows/Linux CI worker-process tests перед переводом ADR в `accepted`.

## Нерешённые вопросы

- E09-T08/ADR-Q09: Arrow IPC, shared memory или иной batch transport;
- политика периодического heartbeat и UI diagnostic bundle;
- sandbox/resource primitives и отдельный workdir E09-T10;
- нужно ли заменить JSON control payload на Protobuf после измерения и до
  стабильного external-worker API.

## Ссылки

- `docs/PROJECT_SPEC.md` NFR-002/NFR-005/NFR-013/NFR-014/NFR-043
- `docs/ARCHITECTURE.md` §11.3, §21, §27 ADR-Q02/Q03/Q09
- `docs/IMPLEMENTATION_PLAN.md` E09-T04…T08, PR-020
