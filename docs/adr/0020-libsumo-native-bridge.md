# ADR-020: Versioned native bridge inside SUMO worker

- Статус: proposed
- Дата: 2026-07-21
- Владельцы: backend/architecture maintainers
- Reviewers: architecture, security, native build, Windows platform
- Связанные требования: FR-040, FR-041, NFR-013, NFR-043
- Связанные задачи: E10-T02, E10-T11, ADR-Q06

## Контекст

libsumo предоставляет C++ static API без стабильного C ABI. Rust worker должен
владеть ровно одной native session, но libsumo types, exceptions и allocator
objects не могут пересекать process/IPC boundary. Exact native artifact пока
собирается отдельно от Cargo workspace и отличается по platform packaging.

## Рассмотренные варианты

### Прямая C++ FFI из каждого Rust call site

Минимум runtime indirection, но распространяет `unsafe`, mangled C++ ABI и
platform linker details по worker implementation.

### Python libsumo worker

Готовые upstream wheels ускоряют spike, но Python оказывается в горячем timestep
и нарушает согласованную границу стека. Этот вариант отклонён.

### Версионированный C ABI bridge, загружаемый worker

Небольшая C++ library владеет вызовами libsumo и переводит их в bounded scalar/C
buffers. Rust загружает только exact packaged library и проверяет ABI/version до
handshake. Цена — отдельный native build и один изолированный unsafe loader.

## Предлагаемое решение

Выбран третий вариант:

- `sumo-worker` — отдельный process и единственный владелец одной active session;
- bridge ABI v2 экспортирует version/revision/start/step/close и один bounded
  vehicle-state batch collector; ABI v1 worker отклоняет до handshake;
- `bundle_path` остаётся нормализованным relative path внутри run workdir;
- C++ exceptions не пересекают ABI, наружу возвращается bounded status;
- runtime vehicle ID обязан иметь форму `rs_agent_<u32>`; неизвестные и
  дублирующиеся IDs блокируют collection вместо неустойчивого string hashing;
- collector сортирует compact IDs, переводит SUMO front-position в центр
  footprint и navigation degrees в mathematical radians local CRS;
- worker публикует stable diagnostic phase и не передаёт native error string в UI;
- `libloading` находится только в `sumo-worker::native`; library handle живёт
  дольше всех скопированных function pointers;
- ABI mismatch/unavailable engine блокирует handshake до session;
- Drop активного native owner best-effort вызывает close, а crash убивает только
  worker process.

Protocol v3 вводит явные `OpenSession(config)`, `StepSession(steps)` и
`CloseSession`. Health `Ping` не используется как simulation step.

## Безопасность

Unsafe разрешён только в одном module с safety comments для каждого load/call и
bounded 512-byte output buffers. Library path задаётся trusted packaging/runtime,
не содержимым проекта. Project-controlled bundle path валидируется как relative
normal components до native call. Shell interpolation и network listener
отсутствуют.

## Проверка и ограничения

- ABI fixture доказывает exact identity, start/ordered steps, двухагентный SoA
  batch, close и native abort isolation на Unix;
- missing bridge даёт `worker.engine.unavailable` до открытия session;
- C++ production bridge вызывает `Simulation::getVersion/start/step/close` и
  vehicle getters, сравнивает runtime version с build pin и не выполняет
  per-agent IPC;
- fixture не является доказательством запуска SUMO. Exact `1.27.1` headless
  bridge build и scenario smoke обязательны до завершения E10-T02;
- Windows ABI fixture и clean-machine native artifacts относятся к E10-T11.

## Ссылки

- [официальный libsumo C++ lifecycle](https://eclipse.dev/sumo/docs/Libsumo.html)
- `docs/ARCHITECTURE.md` §11
- `docs/IMPLEMENTATION_PLAN.md` E10-T02
- ADR-018/019: worker control/data transport
