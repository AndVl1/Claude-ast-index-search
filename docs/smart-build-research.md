# smart-build — Research & Design Document

## Проблема

LLM-кодинг-агенты тратят 15-30% контекстного окна на сырой вывод билд-тулов. Типичный `cargo build` / `gradle build` / `tsc` выдаёт 200-2000 строк, из которых полезны 5-10.

### Масштаб проблемы (данные из исследований)

| Агент | Обрезка вывода | Парсинг ошибок | Проблемы |
|-------|---------------|----------------|----------|
| **Claude Code** | 30K символов, сохранение на диск | Нет | Issue #12054: один вызов getDiagnostics() → 580K токенов, мгновенное "context low" |
| **Cursor** | 250 строк (файлы) | Нет | Работает с сырым текстом |
| **Aider** | По token limit модели | Да (tree-sitter AST) | Единственный с парсингом, но только синтаксис |
| **OpenAI Codex** | 256 строк / 10 KiB (head+tail) | Нет | head+tail прячет середину, где ошибки |
| **GitHub Copilot** | ~64K токенов | Нет | Не может разобрать вывод MSVC |
| **Continue.dev** | По context length | Нет | Ошибки на файлах >12K строк |

**Ключевые находки:**
- Input-токены доминируют в стоимости (исследование OpenReview)
- Больше токенов ≠ выше точность — задачи с бóльшим потреблением обычно решаются **хуже**
- Оптимальная загрузка контекста: **40-60%**, не 100% (Factory.ai)
- `swift test` генерирует 2000+ строк → после xcsift ~15 строк (99.25% сжатие)

## Конкуренты

### Единственный прямой конкурент: xcsift (только Xcode/Swift)

- **GitHub**: [ldomaradzki/xcsift](https://github.com/ldomaradzki/xcsift)
- Парсит xcodebuild output → JSON/TOON
- TOON формат — 30-60% экономия токенов vs JSON
- Есть MCP-сервер
- **Ограничение**: только Swift/Xcode

### Смежные инструменты

| Инструмент | Что делает | Ограничение |
|-----------|-----------|-------------|
| **XcodeBuildMCP** | MCP-сервер для Xcode, фильтрация 80-95% | Только Xcode |
| **xcpretty** | Форматтер xcodebuild | Только Xcode, нет JSON для ошибок |
| **VS Code Problem Matchers** | Regex-парсеры в tasks.json | Только внутри VS Code |
| **GitHub Actions Problem Matchers** | Regex → аннотации на PR | Только в CI, regex-based |
| **MSBuild Structured Log** | Бинарный лог .NET билда | Только .NET, бинарный формат |
| **cargo_metadata** (Rust crate) | Парсит cargo JSON output | Только Rust, библиотека не CLI |

**Вывод: универсального standalone-инструмента НЕТ.** Каждый решает одну экосистему.

## Стандарты

### SARIF (Static Analysis Results Interchange Format)

OASIS-стандарт JSON для диагностик. Поддерживают:
- GCC 15+ (рекомендуемый формат, JSON deprecated)
- MSVC (`-experimental:log`)
- CMake (`CMAKE_EXPORT_SARIF=ON`)
- .NET/Roslyn (`/errorlog:file.sarif`)
- GitHub Code Scanning

```json
{
  "version": "2.1.0",
  "runs": [{
    "tool": { "driver": { "name": "gcc" } },
    "results": [{
      "ruleId": "Werror=format",
      "level": "error",
      "message": { "text": "format specifies type 'int'..." },
      "locations": [{
        "physicalLocation": {
          "artifactLocation": { "uri": "test.c" },
          "region": { "startLine": 10, "startColumn": 5 }
        }
      }]
    }]
  }]
}
```

### LSP Diagnostics

Стандарт textDocument/publishDiagnostics — все LSP-серверы (rust-analyzer, tsserver, gopls, clangd) уже выдают структурированные диагностики.

## Карта форматов билд-тулов

### Нативный JSON

| Инструмент | Флаг | Каскады | Качество |
|-----------|------|---------|----------|
| **cargo/rustc** | `--message-format=json` | Да (`children[]`) | Отличное — все поля, suggestions |
| **GCC** | `-fdiagnostics-format=sarif-stderr` | Да (`children[]`) | Хорошее (SARIF) |
| **Clang** | `-fdiagnostics-format=json` | Да | Хорошее |
| **dart analyze** | `--format=json` | Нет | Хорошее — severity, location, code |
| **mypy** | `--output=json` | Нет | Хорошее — все поля |
| **ruff** | `--output-format=json` | Нет | Отличное — включая fix suggestions |
| **ESLint** | `--format=json` | Нет | Отличное — ruleId, fix |
| **jest** | `--json` | Нет | Хорошее — по suite |
| **go build** | `-json` (Go 1.24+) | Нет | Обёртка текста — нужен допарсинг |

### Парсируемый текст (стабильный формат)

| Инструмент | Формат | Regex |
|-----------|--------|-------|
| **tsc** | `file(line,col): error TSxxxx: msg` | `^(?<file>.*)\((?<line>\d+),(?<col>\d+)\): (?<sev>error\|warning) (?<code>TS\d+): (?<msg>.*)$` |
| **kotlinc** | `e: file:///path:line:col: msg` | `^(?<sev>[ewi]): file://(?<file>[^:]+):(?<line>\d+):(?<col>\d+): (?<msg>.*)$` |
| **javac** | `file:line: error: msg` | `^(?<file>[^:]+):(?<line>\d+): (?<sev>error\|warning): (?<msg>.*)$` |
| **swiftc** | `file:line:col: error: msg` | `^(?<file>[^:]+):(?<line>\d+):(?<col>\d+): (?<sev>error\|warning\|note): (?<msg>.*)$` |
| **Python traceback** | `File "file", line N` | Многострочный парсинг |
| **pytest** | Нужен плагин `--json-report` | — |

### Проблемные (нужен сложный парсинг)

| Инструмент | Проблема |
|-----------|----------|
| **Gradle** | Мешает download progress, task execution, mixed output. Ошибки: `e:` prefix |
| **Maven** | `[ERROR]` prefix, но много мусора между ошибками |
| **webpack/vite** | Плагины меняют формат. Много цвета и unicode |

## Каскадные ошибки — ключевая фича

### Паттерны по языкам

| Язык | Каскадность | Типичный паттерн | Эвристика |
|------|------------|------------------|-----------|
| **C++** | Очень высокая | 1 missing `#include` → 50 `undeclared identifier` | `#include` not found = root cause |
| **Rust** | Средняя | E0432 unresolved import → N `cannot find` | E0432/E0433 = root cause |
| **TypeScript** | Высокая | TS2307 module not found → N TS2304 cannot find name | TS2307 = root cause |
| **Kotlin/KAPT** | Очень высокая | kapt error → N `Unresolved reference` в generated | Ошибка в kapt фазе = root cause |
| **Java** | Высокая | 1 missing import → 30 `cannot find symbol` | Группировка по имени символа |
| **Go** | Низкая | `undeclared name` → `assignment mismatch` | Компилятор сам подавляет (лимит 10) |

### Универсальные эвристики (из исследований)

1. **"Fix the first, ignore the rest"** — первая ошибка по порядку = наиболее вероятная причина (подтверждено на 21M сообщений компилятора, Becker et al. SIGCSE 2018)
2. **Группировка по символу** — если одно имя в 5+ ошибках → один пропущенный import
3. **Сгенерированный код** — ошибка в `build/generated/` → искать ошибку annotation processor
4. **Фаза компиляции** — parsing > name resolution > type checking > codegen (ранние приоритетнее)
5. **"package/module not found" всегда корневая** — для всех downstream

## Архитектура smart-build

### Принцип

```
smart-build <command>        # обёртка: запускает команду, парсит вывод
smart-build cargo build      # → структурированный вывод
smart-build gradle build     # → структурированный вывод
smart-build npm run build    # → структурированный вывод
```

### Двухуровневый парсинг

```
Уровень 1: Generic regex
  ├─ file:line:col: error: message    (gcc, javac, swiftc, kotlinc, go)
  ├─ file(line,col): error CODE: msg  (tsc)
  └─ Traceback ... File "X", line N   (Python)

Уровень 2: Специфичные парсеры (по auto-detect)
  ├─ cargo   → --message-format=json  (нативный JSON)
  ├─ gcc     → -fdiagnostics-format=sarif-stderr  (SARIF)
  ├─ dart    → --format=json  (нативный JSON)
  ├─ mypy    → --output=json  (нативный JSON)
  ├─ ruff    → --output-format=json  (нативный JSON)
  ├─ eslint  → --format=json  (нативный JSON)
  ├─ jest    → --json  (нативный JSON)
  ├─ gradle  → парсинг e:/w: строк из mixed output
  └─ tsc     → парсинг стабильного текстового формата
```

### Автодетекция билд-тула

По первому слову команды или по маркерным файлам в проекте:
- `cargo` → Rust parser
- `gradle` / `gradlew` → Gradle parser
- `tsc` / `npx tsc` → TypeScript parser
- `go build` / `go test` → Go parser
- `gcc` / `g++` / `clang` / `clang++` → GCC/Clang parser
- `swift build` / `xcodebuild` → Swift parser
- `dotnet build` → .NET parser
- `dart analyze` → Dart parser
- `mvn` / `maven` → Maven parser
- `pytest` → pytest parser
- `jest` / `npx jest` → Jest parser
- Fallback → Generic regex

### Выходной формат

**Text (по умолчанию):**
```
2 errors, 1 warning (from 47 raw lines)

✗ src/db.rs:45:12 error[E0106]: missing lifetime specifier
  → 12 downstream errors will resolve after fixing this

✗ src/api/handler.rs:89:5 error[E0433]: unresolved import `crate::models::UserDto`
  hint: UserDto was renamed to UserResponse

⚠ src/main.rs:3:5 warning: unused import `serde::Deserialize`
```

**JSON (--format json):**
```json
{
  "tool": "cargo",
  "raw_lines": 47,
  "errors": [
    {
      "file": "src/db.rs",
      "line": 45,
      "column": 12,
      "severity": "error",
      "code": "E0106",
      "message": "missing lifetime specifier",
      "is_root_cause": true,
      "downstream_count": 12,
      "suggestion": null
    }
  ],
  "warnings": [...],
  "summary": {
    "root_errors": 2,
    "downstream_errors": 12,
    "warnings": 1,
    "total_raw": 47
  }
}
```

**TOON (--format toon) — token-optimized:**
```
T:cargo L:47
E src/db.rs:45:12 E0106 missing lifetime specifier R:12
E src/api/handler.rs:89:5 E0433 unresolved import crate::models::UserDto
W src/main.rs:3:5 unused import serde::Deserialize
S:2e/12d/1w
```

### CLI

```bash
# Базовое использование
smart-build cargo build
smart-build gradle assembleDebug
smart-build tsc --noEmit
smart-build go build ./...

# Форматы вывода
smart-build --format json cargo build
smart-build --format toon cargo build      # token-optimized

# Опции
smart-build --no-cascade cargo build       # не группировать каскады
smart-build --warnings=hide cargo build    # скрыть warnings
smart-build --limit 5 gradle build         # макс 5 ошибок
smart-build --raw cargo build              # показать и raw output

# Без обёртки (pipe mode)
cargo build 2>&1 | smart-build --tool cargo
gradle build 2>&1 | smart-build --tool gradle
```

## План реализации

### MVP (Phase 1) — ~1500 строк Rust

| Компонент | Строк | Описание |
|-----------|-------|----------|
| CLI каркас (clap) | ~100 | Аргументы, форматы вывода |
| Process runner | ~150 | Запуск команды, capture stdout+stderr |
| Auto-detect | ~100 | Определение билд-тула по команде |
| Generic regex parser | ~200 | Универсальный `file:line:col: severity: msg` |
| Cargo JSON parser | ~200 | Парсинг `--message-format=json` |
| tsc parser | ~150 | Парсинг `file(line,col): error TSxxxx` |
| Gradle parser | ~200 | Парсинг `e:` / `w:` из mixed output |
| Cascade detector | ~200 | Группировка по символу/файлу, root cause |
| Output formatters | ~200 | text, json, toon |
| **Итого** | **~1500** | |

### Phase 2 — расширение парсеров

| Парсер | Приоритет | Строк |
|--------|-----------|-------|
| GCC/Clang SARIF | Высокий | ~200 |
| Go build | Высокий | ~150 |
| Swift/xcodebuild | Средний | ~250 |
| pytest (json-report) | Средний | ~150 |
| jest (json) | Средний | ~150 |
| Maven | Низкий | ~200 |
| dotnet (SARIF) | Низкий | ~150 |
| mypy / ruff | Низкий | ~100 |

### Phase 3 — продвинутые фичи

- **MCP-сервер** — интеграция с Claude Code / Cursor как tool
- **Watch mode** — `smart-build --watch` для автоматического парсинга при пересборке
- **Diff mode** — сравнение с предыдущим билдом ("2 новых ошибки, 1 исправлена")
- **SARIF export** — для интеграции с GitHub Code Scanning
- **Cache** — кэширование результатов для идентичных билдов

## Оценка экономии

| Сценарий | Raw output | smart-build output | Экономия |
|----------|-----------|-------------------|----------|
| cargo build (5 ошибок) | ~100 строк | ~8 строк | **92%** |
| gradle assembleDebug (KAPT cascade) | ~500 строк | ~5 строк | **99%** |
| tsc --noEmit (15 type errors) | ~60 строк | ~15 строк | **75%** |
| swift build (template error) | ~200 строк | ~3 строки | **98%** |
| go build (3 errors) | ~15 строк | ~5 строк | **67%** |

**В токенах:** средняя экономия **85-95%** на каждой итерации build → fix → build.

При 5 итерациях на задачу: **~5000-15000 токенов экономии** на задачу.

## Название и позиционирование

**Варианты:**
- `smart-build` — описательное, но generic
- `build-lens` — фокус на "линзе" для ошибок
- `build-brief` — краткость как ценность
- `bx` — короткое, как `jq` для build output
- `berr` — build errors

**Позиционирование:** "jq for build output" — универсальный парсер ошибок билда для LLM-агентов и людей.

## Ссылки

### Исследования
- [How Do Coding Agents Spend Your Money?](https://openreview.net/forum?id=1bUeVB3fov) — потребление токенов агентами
- [The Context Window Problem](https://factory.ai/news/context-window-problem) — оптимальная загрузка 40-60%
- [Fix the First, Ignore the Rest](https://www.semanticscholar.org/paper/Fix-the-First,-Ignore-the-Rest) — анализ 21M сообщений компилятора
- [Programmers' Build Errors at Google](https://research.google/pubs/programmers-build-errors-a-case-study-at-google/) — 26.6M билдов
- [Budget-Aware Tool-Use](https://arxiv.org/html/2511.17006v1) — метрика стоимости агентов

### Конкуренты и стандарты
- [xcsift](https://github.com/ldomaradzki/xcsift) — парсер Xcode output для AI
- [SARIF 2.1.0 Spec](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html) — стандарт диагностик
- [XcodeBuildMCP](https://github.com/cameroncooke/XcodeBuildMCP) — MCP для Xcode
- [cargo_metadata](https://crates.io/crates/cargo_metadata) — парсер cargo JSON (137M+ downloads)

### Проблемы агентов
- [Claude Code #12054](https://github.com/anthropics/claude-code/issues/12054) — 580K токенов от одного вызова
- [Claude Code #21246](https://github.com/anthropics/claude-code/issues/21246) — запрос на verbosity controls
- [Codex #6426](https://github.com/openai/codex/issues/6426) — head+tail обрезка прячет ошибки
- [Copilot #163666](https://github.com/orgs/community/discussions/163666) — не читает MSVC output
- [Stop Wasting Context on Build Output](https://ldomaradzki.com/blog/stop-wasting-context-build-output) — блог-пост

### Документация билд-тулов
- [rustc JSON output](https://doc.rust-lang.org/rustc/json.html)
- [GCC Diagnostic Formatting](https://gcc.gnu.org/onlinedocs/gcc/Diagnostic-Message-Formatting-Options.html)
- [Dart analyze](https://dart.dev/tools/analysis)
- [ESLint Formatters](https://eslint.org/docs/latest/use/formatters/)
- [Jest JSON](https://jestjs.io/docs/configuration)
