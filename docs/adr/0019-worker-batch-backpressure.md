# ADR-019: Worker batch pipe and backpressure baseline

- Статус: proposed
- Дата: 2026-07-19
- Владельцы: backend/results/architecture maintainers
- Reviewers: architecture, performance, security, Windows platform
- Связанные требования: FR-047, FR-049, FR-054, NFR-002, NFR-014, NFR-022
- Связанные задачи: E09-T08, E10-T08, ADR-Q09

## Контекст

Control pipe ADR-018 обеспечивает responsive handshake/cancel/watchdog, но один
последовательный поток для control и state batches создаёт head-of-line blocking.
Visual frames разрешено пропускать под backpressure, тогда как metrics и terminal
events терять нельзя. Per-agent IPC запрещён.

Arrow IPC и shared memory требуют измерений, дополнительных зависимостей и разных
platform primitives. До появления libsumo collector нужен исполняемый baseline,
который доказывает batching/loss policy и даёт материал для benchmark ADR-Q09.

## Рассмотренные варианты

### Вариант A: multiplex всех сообщений по stdout

Не требует второго pipe, но заполненная reliable очередь задерживает control
response и может сделать cancel неотличимым от hang.

### Вариант B: второй inherited pipe с JSON SoA batches

На всех целевых OS `Command` уже предоставляет отдельный piped stderr handle.
Worker резервирует его для framed machine data, поэтому control stdout остаётся
независимым. JSON не является конечным throughput решением, но не добавляет
dependency до измерений.

### Вариант C: Arrow IPC/shared memory немедленно

Лучше подходит для больших потоков, но преждевременно фиксирует ADR-Q09 и
platform/dependency footprint без benchmark реального collector.

## Предлагаемое решение

Для E09-T08 предлагается вариант B как baseline:

- stdin/stdout остаются control pipe ADR-018;
- inherited stderr полностью резервируется для data frames и не используется
  как human log stream;
- data frame имеет отдельный limit 16 MiB; исторический v1 сохранён, текущий
  `schemas/worker-protocol/data-v2.schema.json` синхронизирован с control
  handshake version;
- envelope содержит protocol/session/sequence, payload — visual frame, metric
  batch либо terminal event;
- visual frame использует SoA и максимум 100 000 агентов; все arrays имеют одну
  длину, координаты finite, footprint положителен;
- client хранит ровно последний непрочитанный visual frame, replacement увеличивает
  observable dropped-frame counter;
- metrics содержат exact immutable definition ID/version, группируются максимум
  по 4096 samples и направляются в bounded reliable queue capacity 8;
- terminal events используют ту же reliable очередь;
- worker serializes data на отдельном writer thread, поэтому control loop не
  выполняет pipe writes напрямую;
- при заполнении reliable queue reader перестаёт читать data pipe, передавая
  backpressure worker; control reader остаётся независимым;
- graceful shutdown одновременно reap-ит child и переносит уже принятые
  reliable events в локальный backlog до закрытия data pipe;
- Drop/timeout может отбросить непубликованные reliable data только вместе с
  принудительным завершением run, который обязан остаться incomplete/failed.

## Последствия и ограничения

- Loss policy проверяется end-to-end отдельным child process без UI/SUMO types.
- JSON SoA создаёт дополнительное кодирование и не объявляется production-format
  для больших сетей.
- Отдельный data reader thread не блокирует UI thread или control responses.
- Обычный stderr logging запрещён; будущие structured logs идут control/data DTO.
- Schema не выражает равенство длин SoA arrays, поэтому Rust validation остаётся
  обязательной после deserialization.
- Runtime evidence пока macOS-only; Windows/Linux подтверждает CI.

## Проверка решения

- misaligned/non-finite/oversized batch rejection;
- 32 visual frames без consumer дают latest tick 31 и dropped count 31;
- очередь capacity 8 передаёт 12 ordered metric batches без потерь;
- cancel terminal event не теряется;
- graceful shutdown сохраняет все 12 уже находящихся в pipe metric batches;
- control ping/cancel работают при data backpressure;
- exact Rust 1.88 fmt/clippy/test/rustdoc/dependency/license/advisory gates.

## Условия следующего решения ADR-Q09

Перед заменой baseline нужны измерения encode/decode CPU, bytes/frame, latency и
memory для типичных и верхних сценариев. Arrow IPC и shared memory сравниваются
на macOS/Windows/Linux; новый native/unsafe dependency требует отдельного review.

## Ссылки

- `docs/ARCHITECTURE.md` §11.3, §11.5, §23, §27 ADR-Q09
- `docs/IMPLEMENTATION_PLAN.md` E09-T08, E10-T08, PR-020
- ADR-018: worker control framing and child transport
