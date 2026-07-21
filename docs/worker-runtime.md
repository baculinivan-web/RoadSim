# Worker runtime directories

> Связанные требования: E09-T10, NFR-013, NFR-043; transport описан в ADR-018
> и ADR-019.

## Граница владения

`RunDirectoryManager` получает отдельный root от application runtime и создаёт
только каталоги `run-<16 hex creation sequence>-<16 hex session ID>`. Session ID
может повториться в другом запуске, но пара caller-provided creation sequence и
session ID обязана быть уникальной. Worker запускается с таким каталогом как
`current_dir`; shell interpolation и произвольное имя каталога не используются.

`current_dir` ограничивает относительные output paths, но не является OS sandbox:
worker всё ещё способен открыть абсолютный путь с правами parent process. Реальные
macOS/Windows/Linux sandbox/resource primitives остаются частью E10 packaging и
E16 hardening. До этого worker получает минимальный environment и отдельный root,
а все передаваемые ему relative paths должны проходить нормализацию в adapter.

## State journal v1

Состояние публикуется append-only файлами
`state-<8 lowercase hex transition sequence>.json` по schema
`schemas/worker-runtime/run-state-v1.schema.json`. `create_new` не перезаписывает
предыдущую запись; каждый файл flush-ится через `sync_all` до перехода in-memory
state. Journal имеет configurable hard limit 4…64 entries.

Допустимые переходы:

```text
Starting → Running → Completed
    │          ├──→ Cancelled
    │          ├──→ Failed
    │          └──→ Incomplete
    ├─────────────→ Cancelled
    ├─────────────→ Failed
    └─────────────→ Incomplete
```

Terminal state не может перейти обратно. `Failed` и `Incomplete` требуют stable
diagnostic code; `Completed` и `Cancelled` не маскируют failure diagnostic.
Caller может записать `Completed` только после flush/finalize результатов и
подтверждённого manifest; journal проверяет порядок lifecycle, но не содержимое
result storage.
Drop активного owner best-effort публикует `Incomplete`. После process crash
recovery находит последний contiguous valid journal entry и явно дописывает
`Incomplete`; повторный recovery идемпотентен.

## Retention и cleanup

Количество retained run directories ограничено 1…1024. При достижении limit
автоматически удаляется старейший по caller-provided `creation_sequence`, затем
session ID, но только если state — `Completed` или `Cancelled`. `Failed` и
`Incomplete` не удаляются автоматически: если только они занимают capacity,
новый run блокируется stable error до явного решения пользователя/runtime.

Explicit cleanup также удаляет только `Completed`/`Cancelled`. Перед рекурсивным
удалением повторно проверяются exact parent, generated basename и отсутствие
symlink на месте run directory. Symlink внутри workdir, неизвестная entry в root,
неполный/непоследовательный journal и превышение marker size fail closed.

## Ограничения baseline

- Root создаётся из доверенной application configuration; manager не является
  защитой от symlink race со скомпрометированным concurrent process.
- CPU/memory/disk-byte quotas и cleanup policy diagnostic bundles ещё не
  реализованы.
- Marker содержит только lifecycle metadata, не заменяет `run_manifest.json` и
  не публикует partial results как completed.
- Schema v1 является первой версией; migration предыдущего формата отсутствует.
